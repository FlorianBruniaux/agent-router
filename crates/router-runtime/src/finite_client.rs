//! Review-bound finite-client preparation. Never OS or provider admission.
use crate::{Result, RuntimeError, StoredCapabilityProofVerifier};
use router_adapter_api::{
    CapabilityProofBinding, CapabilityProofTier, CapabilityProofVerifier, EvidenceSource,
    content_digest, is_digest,
};
use router_types::{
    Capability, ExecutionMode, ExecutionTarget, FiniteClientFile, FiniteClientManifest,
    HostSnapshot, PreparedRun,
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Read,
    net::{IpAddr, Ipv4Addr, TcpListener},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

const MAX_CONTRACT: usize = 32 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;
const MAX_CONFIGURATION: u64 = 64 * 1024;

/// Ownership of an already-bound loopback listener, not a port number from JSON.
/// This object is neither serializable nor cloneable. A preparation guard borrows it.
pub struct OwnedBroker {
    listener: TcpListener,
    endpoint: String,
    instance: String,
}
impl OwnedBroker {
    pub fn from_listener(listener: TcpListener) -> Result<Self> {
        let address = listener.local_addr()?;
        if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) || address.port() == 0 {
            return Err(RuntimeError::Sandbox);
        }
        let mut nonce = [0; 32];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        Ok(Self {
            listener,
            endpoint: format!("tcp://{address}"),
            instance: content_digest(&nonce),
        })
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn instance_sha256(&self) -> &str {
        &self.instance
    }
    fn check(&self, manifest: &FiniteClientManifest) -> Result<()> {
        if manifest.broker_endpoint != self.endpoint
            || manifest.broker_instance_sha256 != self.instance
            || format!("tcp://{}", self.listener.local_addr()?) != self.endpoint
        {
            return Err(RuntimeError::Integrity("finite client broker binding"));
        }
        Ok(())
    }
}

#[derive(Eq, PartialEq, serde::Serialize)]
struct FileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    owner: u32,
}
impl FileIdentity {
    fn from(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            owner: metadata.uid(),
        }
    }
}
// Retaining the descriptor prevents inode reuse after unlink. It does not
// prevent another writer from changing the bytes of the same open inode.
struct PinnedIdentity {
    identity: FileIdentity,
    _file: File,
}
fn canonical(path: &str) -> Result<&Path> {
    if path.is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err(RuntimeError::Invalid("finite client path"));
    }
    let path = Path::new(path);
    if !path.is_absolute() || fs::canonicalize(path)? != path {
        return Err(RuntimeError::Sandbox);
    }
    Ok(path)
}
fn directory(path: &str, private: bool) -> Result<PinnedIdentity> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_DIRECTORY)
        .open(canonical(path)?)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & if private { 0o077 } else { 0o022 } != 0
    {
        return Err(RuntimeError::Sandbox);
    }
    Ok(PinnedIdentity {
        identity: FileIdentity::from(&metadata),
        _file: file,
    })
}
fn file_identity(pin: &FiniteClientFile, executable: bool) -> Result<PinnedIdentity> {
    if !is_digest(&pin.sha256) {
        return Err(RuntimeError::Invalid("finite client file digest"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(canonical(&pin.path)?)?;
    let metadata = file.metadata()?;
    let max = if executable {
        MAX_BINARY
    } else {
        MAX_CONFIGURATION
    };
    if !metadata.is_file()
        || metadata.len() > max
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o7000 != 0
        || (metadata.uid() != 0 && metadata.uid() != rustix::process::geteuid().as_raw())
        || (executable && metadata.mode() & 0o111 == 0)
    {
        return Err(RuntimeError::Sandbox);
    }
    let identity = FileIdentity::from(&metadata);
    let mut bytes = Vec::new();
    (&file).take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(RuntimeError::Limit);
    }
    if content_digest(&bytes) != pin.sha256
        || FileIdentity::from(&file.metadata()?) != identity
        || FileIdentity::from(&fs::symlink_metadata(&pin.path)?) != identity
    {
        return Err(RuntimeError::Integrity("finite client file changed"));
    }
    Ok(PinnedIdentity {
        identity,
        _file: file,
    })
}
fn token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn prepared_digest(run: &PreparedRun) -> Result<String> {
    if run.schema_version != 1
        || run.host.as_str() != "codex"
        || run.program != "finite-client"
        || run.target != ExecutionTarget::ExternalWorker
        || run.mode != ExecutionMode::Execute
        || run.parent_model_changed
        || !run.degradations.is_empty()
        || !run.missing_runtime_evidence.is_empty()
        || run.stdin.as_bytes().len() > MAX_CONTRACT
        || run.argv.len() > 64
        || run
            .argv
            .iter()
            .any(|s| s.len() > 4096 || s.chars().any(char::is_control))
        || run.scope.len() > 64
        || run.scope.iter().any(|s| {
            s.is_empty()
                || s.len() > 4096
                || s.starts_with('/')
                || s.contains('\\')
                || s.chars().any(char::is_control)
                || s.split('/').any(|part| matches!(part, "" | "." | ".."))
        })
        || run.scope.iter().collect::<BTreeSet<_>>().len() != run.scope.len()
        || run.timeout_ms == 0
        || run.timeout_ms > 60_000
        || run.max_output_bytes == 0
        || run.max_output_bytes > 1024 * 1024
        || [
            &run.receipt_id,
            &run.authorization_digest,
            &run.snapshot_hash,
        ]
        .iter()
        .any(|s| !is_digest(s))
        || !matches!(run.source_state.len(), 40 | 64)
        || !run.source_state.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(RuntimeError::Invalid("finite client prepared contract"));
    }
    canonical(&run.cwd)?;
    let bytes = serde_json::to_vec(&(run, content_digest(run.stdin.as_bytes())))?;
    if bytes.len() > MAX_CONTRACT {
        return Err(RuntimeError::Limit);
    }
    Ok(content_digest(&bytes))
}
fn pins(manifest: &FiniteClientManifest) -> [(&FiniteClientFile, bool); 6] {
    [
        (&manifest.client, true),
        (&manifest.native, true),
        (&manifest.helper, true),
        (&manifest.wrapper, true),
        (&manifest.profile, false),
        (&manifest.configuration, false),
    ]
}

/// Private manifest and prepared bytes are frozen for a separate, exact review.
/// This review does not change the durable runtime's approval state.
pub struct FiniteClientReview {
    manifest: FiniteClientManifest,
    run: PreparedRun,
    run_digest: String,
    approval: String,
    broker: OwnedBroker,
    identities: Vec<PinnedIdentity>,
    state_identity: PinnedIdentity,
    workspace_identity: PinnedIdentity,
}
impl FiniteClientReview {
    pub fn new(
        manifest: FiniteClientManifest,
        run: &PreparedRun,
        broker: OwnedBroker,
    ) -> Result<Self> {
        if manifest.schema_version != 1
            || !token(&manifest.task_id, 128)
            || !token(&manifest.host_snapshot_id, 128)
            || manifest.native_version.is_empty()
            || manifest.native_version.len() > 128
            || !manifest
                .native_version
                .bytes()
                .all(|b| b.is_ascii_graphic())
            || manifest.broker_endpoint.len() > 128
            || !is_digest(&manifest.broker_instance_sha256)
        {
            return Err(RuntimeError::Invalid("finite client manifest"));
        }
        if manifest.private_state.is_empty()
            || manifest.private_state.len() > 4096
            || pins(&manifest).iter().any(|(pin, _)| {
                pin.path.is_empty() || pin.path.len() > 4096 || !is_digest(&pin.sha256)
            })
        {
            return Err(RuntimeError::Invalid(
                "finite client manifest paths or digests",
            ));
        }
        let run_digest = prepared_digest(run)?;
        let bytes = serde_json::to_vec(&("finite-client-review-v1", &manifest, &run_digest))?;
        if bytes.len() > MAX_CONTRACT {
            return Err(RuntimeError::Limit);
        }
        broker.check(&manifest)?;
        let state_identity = directory(&manifest.private_state, true)?;
        let workspace_identity = directory(&run.cwd, false)?;
        if Path::new(&manifest.private_state).starts_with(&run.cwd)
            || Path::new(&run.cwd).starts_with(&manifest.private_state)
        {
            return Err(RuntimeError::Scope);
        }
        let asset_pins = pins(&manifest);
        if asset_pins
            .iter()
            .map(|(p, _)| &p.path)
            .collect::<BTreeSet<_>>()
            .len()
            != asset_pins.len()
            || asset_pins
                .iter()
                .any(|(p, _)| Path::new(&p.path).starts_with(&run.cwd))
        {
            return Err(RuntimeError::Scope);
        }
        let identities: Vec<PinnedIdentity> = asset_pins
            .iter()
            .map(|(pin, executable)| file_identity(pin, *executable))
            .collect::<Result<_>>()?;
        let bytes = serde_json::to_vec(&(
            "finite-client-review-v1",
            &manifest,
            &run_digest,
            &state_identity.identity,
            &workspace_identity.identity,
            identities
                .iter()
                .map(|pin| &pin.identity)
                .collect::<Vec<_>>(),
        ))?;
        if bytes.len() > MAX_CONTRACT {
            return Err(RuntimeError::Limit);
        }
        Ok(Self {
            manifest,
            run: run.clone(),
            run_digest,
            approval: content_digest(&bytes),
            broker,
            identities,
            state_identity,
            workspace_identity,
        })
    }
    pub fn approval_digest(&self) -> &str {
        &self.approval
    }
    pub fn approve(self, approved_digest: &str) -> Result<ApprovedFiniteClient> {
        if approved_digest != self.approval {
            return Err(RuntimeError::Approval);
        }
        Ok(ApprovedFiniteClient { reviewed: self })
    }
}

/// Nonserializable preparation handle, not `ApprovedRun` or permission to execute.
pub struct ApprovedFiniteClient {
    reviewed: FiniteClientReview,
}
impl ApprovedFiniteClient {
    /// Expected pins for the existing proof store. The argument-template slot
    /// binds the complete reviewed task, native/configuration/private-state and
    /// retained broker instance, not a digest declared by an observation.
    pub fn proof_binding(&self, host_snapshot_id: &str) -> Result<CapabilityProofBinding> {
        let r = &self.reviewed;
        let m = &r.manifest;
        if host_snapshot_id != m.host_snapshot_id {
            return Err(RuntimeError::Integrity("finite client reviewed host"));
        }
        let binding = CapabilityProofBinding {
            harness_id: r.run.host.as_str().into(),
            host_snapshot_id: host_snapshot_id.into(),
            binary_sha256: m.client.sha256.clone(),
            observed_version: m.native_version.clone(),
            adapter_id: "finite-client-contract-v1".into(),
            argument_template_sha256: r.approval.clone(),
            sandbox_helper_sha256: m.helper.sha256.clone(),
            sandbox_wrapper_sha256: m.wrapper.sha256.clone(),
            sandbox_profile_sha256: m.profile.sha256.clone(),
            write_roots: r.run.scope.iter().cloned().collect(),
            network: true,
            os: std::env::consts::OS.into(),
        };
        binding
            .digest()
            .map_err(|_| RuntimeError::Invalid("finite client proof binding"))?;
        Ok(binding)
    }
    /// Reopens the live authenticated store and rechecks every input. Callers
    /// must retain the returned borrow through their preparation boundary.
    /// It does not construct a command, claim an OS canary or authorize a send.
    pub fn validate<'a>(
        &'a self,
        run: &PreparedRun,
        host: &HostSnapshot,
        source: &EvidenceSource,
        verifier: &StoredCapabilityProofVerifier,
        now: u64,
    ) -> Result<FiniteClientValidation<'a>> {
        let r = &self.reviewed;
        if prepared_digest(run)? != r.run_digest
            || host.harness != run.host
            || host.id != r.manifest.host_snapshot_id
            || !host.supported_models.contains(&run.requested_model)
            || host
                .supported_efforts
                .get(&run.requested_model)
                .is_none_or(|efforts| !efforts.contains(&run.requested_effort))
            || !host.capabilities.contains(&Capability::Network)
        {
            return Err(RuntimeError::Integrity("finite client current task"));
        }
        r.broker.check(&r.manifest)?;
        if directory(&r.manifest.private_state, true)?.identity != r.state_identity.identity
            || directory(&run.cwd, false)?.identity != r.workspace_identity.identity
        {
            return Err(RuntimeError::Integrity("finite client directory changed"));
        }
        for ((pin, executable), expected) in pins(&r.manifest).iter().zip(&r.identities) {
            if file_identity(pin, *executable)?.identity != expected.identity {
                return Err(RuntimeError::Integrity("finite client file identity"));
            }
        }
        verifier
            .require_live()
            .map_err(|_| RuntimeError::Integrity("finite client historical proof"))?;
        let proof = verifier
            .verify(source, Capability::Network, host, now)
            .map_err(|_| RuntimeError::Integrity("finite client authenticated proof"))?;
        if proof.binding != self.proof_binding(&host.id)?
            || proof.tier != CapabilityProofTier::Proven
            || proof.observed_at_unix_seconds > now
            || proof.expires_at_unix_seconds <= now
        {
            return Err(RuntimeError::Integrity(
                "finite client proof binding or freshness",
            ));
        }
        Ok(FiniteClientValidation { approved: self })
    }
}

/// A borrow keeps the reviewed listener alive; it is not executable authority.
/// File checks and a later path-based exec would remain separate steps (TOCTOU).
pub struct FiniteClientValidation<'a> {
    approved: &'a ApprovedFiniteClient,
}
impl FiniteClientValidation<'_> {
    pub fn broker_endpoint(&self) -> &str {
        self.approved.reviewed.broker.endpoint()
    }
    /// Trusted controller access only. No listener is transferred to the worker.
    pub fn broker_listener(&self) -> &TcpListener {
        &self.approved.reviewed.broker.listener
    }
}
