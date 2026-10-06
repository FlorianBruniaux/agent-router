//! A version pin is a requirement, never evidence until this probe succeeds.
use crate::{
    Cancellation, NetworkAuthority, Result, RuntimeError, SandboxPolicy, SandboxSupervisor,
    leases::LeaseStorage, process::capture,
};
use router_adapter_api::content_digest;
use router_types::{
    ExecutionSandbox, PreparedInput, PreparedRun, ProcessCleanupEvidence, TransportOutcome,
    TransportTermination,
};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TIMEOUT_MS: u64 = 5_000;
const MAX_BYTES: usize = 8_192;

struct ProbeDirectory(std::path::PathBuf);
impl Drop for ProbeDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Deliberately not serializable or Debug: a hostile version command may print
/// sensitive data. Only a host-specific strict parser should inspect stdout.
pub struct NativeVersionProbe {
    stdout: Vec<u8>,
    pub output_sha256: String,
    pub outcome: TransportOutcome,
    pub cleanup: ProcessCleanupEvidence,
}
impl NativeVersionProbe {
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }
}

/// Execute only `--version`, with no prompt, no network and a read-only sandbox.
/// The trusted supervisor rechecks the approved binary and helper identities.
/// This proves neither authentication nor model/effort availability.
pub async fn probe_native_version(
    supervisor: &dyn SandboxSupervisor,
    prepared: &PreparedRun,
    private_dir: &Path,
    cancel: &Cancellation,
) -> Result<NativeVersionProbe> {
    probe_native_version_inner(supervisor, prepared, private_dir, cancel, None).await
}

pub(crate) async fn probe_native_version_recorded(
    supervisor: &dyn SandboxSupervisor,
    prepared: &PreparedRun,
    private_dir: &Path,
    cancel: &Cancellation,
    preparation: &crate::store::Preparation<'_>,
) -> Result<NativeVersionProbe> {
    probe_native_version_inner(supervisor, prepared, private_dir, cancel, Some(preparation)).await
}

async fn probe_native_version_inner(
    supervisor: &dyn SandboxSupervisor,
    prepared: &PreparedRun,
    private_dir: &Path,
    cancel: &Cancellation,
    preparation: Option<&crate::store::Preparation<'_>>,
) -> Result<NativeVersionProbe> {
    if !supervisor.parent_lifetime_enforced() || !supervisor.resource_limits_enforced() {
        return Err(RuntimeError::Sandbox);
    }
    let meta = fs::symlink_metadata(private_dir)?;
    if !meta.is_dir()
        || meta.permissions().mode() & 0o077 != 0
        || meta.uid() != rustix::process::geteuid().as_raw()
        || fs::canonicalize(private_dir)? != private_dir
    {
        return Err(RuntimeError::Integrity("version probe directory"));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeError::Invalid("clock"))?
        .as_nanos();
    let id = format!(
        "version-{}-{}-{}",
        std::process::id(),
        stamp,
        PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    // Keep project-local configuration out of a version-only invocation. The
    // child can read this empty working directory, not the runtime store root.
    let cwd = private_dir.join(&id);
    fs::DirBuilder::new().mode(0o700).create(&cwd)?;
    // Removed on every exit, including errors and a dropped future. The
    // unique name means only this call ever creates or reads it.
    let _cwd_guard = ProbeDirectory(cwd.clone());
    let mut run = prepared.clone();
    run.argv = vec!["--version".into()];
    run.stdin = PreparedInput::new(Vec::new());
    run.cwd = cwd
        .to_str()
        .ok_or(RuntimeError::Invalid("utf8 probe path"))?
        .into();
    run.scope.clear();
    run.sandbox = ExecutionSandbox::ReadOnly;
    run.timeout_ms = TIMEOUT_MS;
    run.max_output_bytes = MAX_BYTES;
    let host_state = supervisor.create_host_state(private_dir, &run)?;
    let mut command = supervisor.command(
        &run,
        &SandboxPolicy {
            network: NetworkAuthority::Deny,
            host_state: host_state.as_ref(),
        },
    )?;
    let leases = LeaseStorage::open(private_dir)?;
    let registered = if let Some(preparation) = preparation {
        Some(preparation.register_probe()?)
    } else {
        None
    };
    let local = if registered.is_none() {
        Some(leases.create(&id, 0)?)
    } else {
        None
    };
    let path = registered
        .as_ref()
        .map(|(_, p)| p)
        .or_else(|| local.as_ref().map(|(_, p)| p))
        .ok_or(RuntimeError::Integrity("missing probe lease"))?;
    command.env("ROUTER_RUNTIME_LEASE", path);
    let captured = capture(command, Vec::new(), TIMEOUT_MS, MAX_BYTES, 100, cancel).await;
    // Invalidate an unstarted lease too, so a delayed helper cannot launch after
    // a failed spawn. Active or ambiguous cleanup never authorizes execution.
    let cleanup = match (preparation, registered.as_ref(), local.as_ref()) {
        (Some(preparation), Some((id, _)), None) => preparation.resolve_probe(id)?,
        (None, None, Some((record, _))) => leases.cleanup_proof(&id, 0, record)?.evidence,
        _ => return Err(RuntimeError::Integrity("probe registration")),
    };
    if let Some(state) = host_state {
        state.cleanup().map_err(|_| RuntimeError::CleanupUnknown)?;
    }
    let captured = captured?;
    if !matches!(
        cleanup,
        ProcessCleanupEvidence::GroupTerminated | ProcessCleanupEvidence::NamespaceTerminated
    ) {
        return Err(RuntimeError::CleanupUnknown);
    }
    if captured.output_limited {
        return Err(RuntimeError::Limit);
    }
    // Capture may observe exit before cancellation arrives during stream
    // collection. Keep cleanup, output bounds and an observed timeout intact.
    if captured.outcome.termination == TransportTermination::Cancelled
        || (captured.outcome.termination == TransportTermination::Exited && cancel.is_cancelled())
    {
        return Err(RuntimeError::Cancelled);
    }
    if captured.outcome.termination != TransportTermination::Exited
        || captured.outcome.exit_code != Some(0)
        || !captured.stderr.is_empty()
    {
        return Err(RuntimeError::Process);
    }
    Ok(NativeVersionProbe {
        output_sha256: content_digest(&captured.stdout),
        stdout: captured.stdout,
        outcome: captured.outcome,
        cleanup,
    })
}
