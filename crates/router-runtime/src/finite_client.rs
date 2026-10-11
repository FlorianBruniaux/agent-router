//! Review-bound finite-client preparation. Never OS or provider admission.
use crate::private_program::PrivateProgram;
use crate::{HostState, ResourceLimits, Result, RuntimeError, StoredCapabilityProofVerifier};
use router_adapter_api::{
    CapabilityProofBinding, CapabilityProofTier, CapabilityProofVerifier, EvidenceSource,
    content_digest, is_digest,
};
use router_types::{
    Capability, ExecutionMode, ExecutionTarget, FiniteClientFile, FiniteClientManifest,
    HostSnapshot, PreparedRun,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
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

// Owned copies are preparation assets, not an immutable view or OS authority.
// Their directories must be excluded from worker writes by the qualified sandbox.
struct FiniteClientAssets {
    copies: [PrivateProgram; 5],
    pins: [FiniteClientFile; 6],
    identities: [PinnedIdentity; 6],
    directories: [PinnedIdentity; 5],
}
impl FiniteClientAssets {
    fn prepare(manifest: &FiniteClientManifest, run: &PreparedRun) -> Result<Self> {
        let store = Path::new(&manifest.private_state);
        let mut copies = Vec::new();
        let mut asset_pins = Vec::new();
        let mut directories = Vec::new();
        for (pin, executable) in pins(manifest).into_iter().take(5) {
            let copy = if executable {
                PrivateProgram::copy(Path::new(&pin.path), &pin.sha256, store)?
            } else {
                PrivateProgram::copy_profile(Path::new(&pin.path), &pin.sha256, store)?
            };
            if copy.directory().starts_with(&run.cwd)
                || Path::new(&run.cwd).starts_with(copy.directory())
            {
                return Err(RuntimeError::Scope);
            }
            let path = copy
                .verify(&pin.sha256)?
                .to_str()
                .ok_or(RuntimeError::Invalid("finite client copy path"))?
                .to_owned();
            directories.push(directory(
                copy.directory()
                    .to_str()
                    .ok_or(RuntimeError::Invalid("finite client copy directory"))?,
                true,
            )?);
            asset_pins.push(FiniteClientFile {
                path,
                sha256: pin.sha256.clone(),
            });
            copies.push(copy);
        }
        // Codex consumes config.toml at the producer-owned home. Moving it to a
        // second arbitrary path would not qualify native configuration consumption.
        asset_pins.push(manifest.configuration.clone());
        let identities: Vec<PinnedIdentity> = asset_pins
            .iter()
            .enumerate()
            .map(|(index, pin)| file_identity(pin, index < 4))
            .collect::<Result<_>>()?;
        let assets = Self {
            copies: copies
                .try_into()
                .map_err(|_| RuntimeError::Invalid("finite client copy count"))?,
            pins: asset_pins
                .try_into()
                .map_err(|_| RuntimeError::Invalid("finite client asset count"))?,
            identities: identities
                .try_into()
                .map_err(|_| RuntimeError::Invalid("finite client identity count"))?,
            directories: directories
                .try_into()
                .map_err(|_| RuntimeError::Invalid("finite client directory count"))?,
        };
        assets.check()?;
        Ok(assets)
    }
    fn check(&self) -> Result<()> {
        for ((copy, pin), expected) in self.copies.iter().zip(&self.pins).zip(&self.directories) {
            copy.verify(&pin.sha256)?;
            if directory(
                copy.directory()
                    .to_str()
                    .ok_or(RuntimeError::Invalid("finite client copy directory"))?,
                true,
            )?
            .identity
                != expected.identity
            {
                return Err(RuntimeError::Integrity(
                    "finite client copy directory changed",
                ));
            }
        }
        for (index, (pin, expected)) in self.pins.iter().zip(&self.identities).enumerate() {
            if file_identity(pin, index < 4)?.identity != expected.identity {
                return Err(RuntimeError::Integrity("finite client copy changed"));
            }
        }
        Ok(())
    }
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
    resource_arguments: Vec<String>,
    owned_state: Option<FiniteCodexState>,
    owned_assets: Option<FiniteClientAssets>,
}
impl FiniteClientReview {
    pub fn new(
        manifest: FiniteClientManifest,
        run: &PreparedRun,
        broker: OwnedBroker,
    ) -> Result<Self> {
        Self::with_resources(manifest, run, broker, ResourceLimits::default())
    }
    /// Concrete helper arguments are validated and bound into the approval.
    pub fn with_resources(
        manifest: FiniteClientManifest,
        run: &PreparedRun,
        broker: OwnedBroker,
        limits: ResourceLimits,
    ) -> Result<Self> {
        let resource_arguments = limits.args()?;
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
        let bytes = serde_json::to_vec(&(
            "finite-client-review-v2",
            &manifest,
            &run_digest,
            &resource_arguments,
        ))?;
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
            "finite-client-review-v2",
            &resource_arguments,
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
            resource_arguments,
            owned_state: None,
            owned_assets: None,
        })
    }
    fn check_files(&self) -> Result<()> {
        self.broker.check(&self.manifest)?;
        if let Some(assets) = &self.owned_assets {
            assets.check()?;
        }
        if let Some(state) = &self.owned_state {
            state.check(&self.run, &self.broker)?;
        }
        if directory(&self.manifest.private_state, true)?.identity != self.state_identity.identity
            || directory(&self.run.cwd, false)?.identity != self.workspace_identity.identity
        {
            return Err(RuntimeError::Integrity("finite client directory changed"));
        }
        for ((pin, executable), expected) in pins(&self.manifest).iter().zip(&self.identities) {
            if file_identity(pin, *executable)?.identity != expected.identity {
                return Err(RuntimeError::Integrity("finite client file identity"));
            }
        }
        Ok(())
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
            adapter_id: if r.owned_assets.is_some() {
                "finite-client-contract-v3"
            } else {
                "finite-client-contract-v2"
            }
            .into(),
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
    /// It does not construct a process, claim an OS canary or authorize a send.
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
        r.check_files()?;
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

/// Runtime-created, credential-free Codex home. Uses the existing HostState
/// lifecycle, and must be consumed by a review to remain alive with approval.
pub struct FiniteCodexState {
    state: HostState,
    configuration: FiniteClientFile,
    tmp_identity: PinnedIdentity,
}
impl FiniteCodexState {
    pub fn path(&self) -> &Path {
        self.state.path()
    }
    pub fn configuration(&self) -> &FiniteClientFile {
        &self.configuration
    }
    fn check(&self, run: &PreparedRun, broker: &OwnedBroker) -> Result<()> {
        let tmp = self.path().join("tmp");
        let tmp = tmp
            .to_str()
            .ok_or(RuntimeError::Invalid("finite client tmp path"))?;
        if directory(tmp, true)?.identity != self.tmp_identity.identity
            || self.configuration.sha256 != content_digest(&codex_configuration(run, broker)?)
        {
            return Err(RuntimeError::Integrity("finite client owned configuration"));
        }
        Ok(())
    }
}
fn codex_configuration(run: &PreparedRun, broker: &OwnedBroker) -> Result<Vec<u8>> {
    // JSON string escaping is also valid for these bounded ASCII TOML strings.
    // Reject non-token model names rather than permit configuration injection.
    if !token(run.requested_model.as_str(), 128) {
        return Err(RuntimeError::Invalid("finite client configuration model"));
    }
    let model = serde_json::to_string(run.requested_model.as_str())?;
    let effort = serde_json::to_string(&run.requested_effort)?;
    let address = broker.listener.local_addr()?;
    let config = format!(
        "model = {model}\nmodel_provider = \"local_fixture\"\nmodel_reasoning_effort = {effort}\napproval_policy = \"on-request\"\napprovals_reviewer = \"user\"\nsandbox_mode = \"read-only\"\nweb_search = \"disabled\"\n[features]\nshell_tool = false\nunified_exec = false\napps = false\n[model_providers.local_fixture]\nname = \"Finite client preparation, local fixture only\"\nbase_url = \"http://{address}/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\nrequest_max_retries = 0\nstream_max_retries = 0\n"
    );
    Ok(config.into_bytes())
}
impl OwnedBroker {
    /// Creates only private local files. No native invocation or provider call.
    pub fn prepare_codex_state(&self, store: &Path, run: &PreparedRun) -> Result<FiniteCodexState> {
        prepared_digest(run)?;
        let store_text = store
            .to_str()
            .ok_or(RuntimeError::Invalid("finite client store path"))?;
        directory(store_text, true)?;
        // Even a read-only store ancestor would let generated files alias task input.
        if store.starts_with(&run.cwd) || Path::new(&run.cwd).starts_with(store) {
            return Err(RuntimeError::Scope);
        }
        let bytes = codex_configuration(run, self)?;
        let state =
            HostState::create(store, "codex", &BTreeMap::new())?.ok_or(RuntimeError::Sandbox)?;
        let tmp = state.path().join("tmp");
        fs::DirBuilder::new().mode(0o700).create(&tmp)?;
        let path = state.path().join("config.toml");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o400)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let configuration = FiniteClientFile {
            path: path
                .to_str()
                .ok_or(RuntimeError::Invalid("finite client config path"))?
                .into(),
            sha256: content_digest(&bytes),
        };
        file_identity(&configuration, false)?;
        let tmp_identity = directory(
            tmp.to_str()
                .ok_or(RuntimeError::Invalid("finite client tmp path"))?,
            true,
        )?;
        Ok(FiniteCodexState {
            state,
            configuration,
            tmp_identity,
        })
    }
}
impl FiniteClientReview {
    /// Consumes the producer's state. An arbitrary manifest path cannot opt in
    /// to launch preparation or reconstruct this nonserializable owner.
    pub fn with_codex_state(
        manifest: FiniteClientManifest,
        run: &PreparedRun,
        broker: OwnedBroker,
        limits: ResourceLimits,
        state: FiniteCodexState,
    ) -> Result<Self> {
        if Path::new(&manifest.private_state) != state.path()
            || manifest.configuration != state.configuration
        {
            return Err(RuntimeError::Integrity("finite client state binding"));
        }
        state.check(run, &broker)?;
        let mut review = Self::with_resources(manifest, run, broker, limits)?;
        let assets = FiniteClientAssets::prepare(&review.manifest, run)?;
        let bytes = serde_json::to_vec(&(
            "finite-client-copy-review-v1",
            &review.approval,
            &assets.pins,
            assets
                .identities
                .iter()
                .map(|pin| &pin.identity)
                .collect::<Vec<_>>(),
            assets
                .directories
                .iter()
                .map(|pin| &pin.identity)
                .collect::<Vec<_>>(),
        ))?;
        if bytes.len() > MAX_CONTRACT {
            return Err(RuntimeError::Limit);
        }
        review.approval = content_digest(&bytes);
        review.owned_assets = Some(assets);
        review.owned_state = Some(state);
        review.check_files()?;
        Ok(review)
    }
}
impl ApprovedFiniteClient {
    /// Checks the helper's concrete current limits as well as the live contract.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_with_resources<'a>(
        &'a self,
        run: &PreparedRun,
        host: &HostSnapshot,
        source: &EvidenceSource,
        verifier: &StoredCapabilityProofVerifier,
        now: u64,
        limits: &ResourceLimits,
    ) -> Result<FiniteClientValidation<'a>> {
        if limits.args()? != self.reviewed.resource_arguments {
            return Err(RuntimeError::Integrity("finite client resource limits"));
        }
        self.validate(run, host, source, verifier, now)
    }
}

/// Private review data only. No conversion to Command, execution receipt or grant.
#[derive(serde::Serialize)]
pub struct FiniteClientInvocation {
    program: String,
    argv: Vec<String>,
    environment: BTreeMap<String, String>,
    clear_environment: bool,
    cwd: String,
}
#[derive(serde::Serialize)]
pub struct FiniteClientLaunchPlan {
    supervised: FiniteClientInvocation,
    native: FiniteClientInvocation,
    stdin_sha256: String,
    timeout_ms: u64,
    max_output_bytes: usize,
    read_only_assets: Vec<FiniteClientFile>,
}
impl FiniteClientValidation<'_> {
    /// Derives the known Seatbelt/helper composition as a private preview.
    /// This remains unavailable on other OSes and never constructs a process.
    pub fn launch_plan(&self) -> Result<FiniteClientLaunchPlan> {
        let r = &self.approved.reviewed;
        r.owned_state.as_ref().ok_or(RuntimeError::Sandbox)?;
        if !cfg!(target_os = "macos") {
            return Err(RuntimeError::Sandbox);
        }
        r.check_files()?;
        let assets = r.owned_assets.as_ref().ok_or(RuntimeError::Sandbox)?;
        let home = r.manifest.private_state.clone();
        let environment = BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), home.clone()),
            ("CODEX_HOME".into(), home.clone()),
            ("TMPDIR".into(), format!("{home}/tmp")),
        ]);
        let native = FiniteClientInvocation {
            program: assets.pins[1].path.clone(),
            argv: vec!["app-server".into(), "--listen".into(), "stdio://".into()],
            environment: environment.clone(),
            clear_environment: true,
            cwd: r.run.cwd.clone(),
        };
        let mut argv = r.resource_arguments.clone();
        argv.extend([
            "--supervise-v1".into(),
            assets.pins[3].path.clone(),
            "-f".into(),
            assets.pins[4].path.clone(),
            assets.pins[0].path.clone(),
        ]);
        argv.extend(r.run.argv.clone());
        let mut worker_environment = environment;
        worker_environment.insert("ROUTER_RUNTIME_SUPERVISION".into(), "framed-v1".into());
        worker_environment.insert(
            "ROUTER_RUNTIME_CONTAINMENT".into(),
            "process-group-v1".into(),
        );
        Ok(FiniteClientLaunchPlan {
            supervised: FiniteClientInvocation {
                program: assets.pins[2].path.clone(),
                argv,
                environment: worker_environment,
                clear_environment: true,
                cwd: r.run.cwd.clone(),
            },
            native,
            stdin_sha256: content_digest(r.run.stdin.as_bytes()),
            timeout_ms: r.run.timeout_ms,
            max_output_bytes: r.run.max_output_bytes,
            read_only_assets: assets.pins.to_vec(),
        })
    }
}

#[cfg(test)]
mod resource_binding_tests {
    use super::*;
    use router_types::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn same_listener_and_assets_require_new_digest_for_each_resource_change() {
        let temp = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = temp.path().canonicalize().unwrap();
        let workspace = root.join("workspace");
        let state = root.join("state");
        for path in [&workspace, &state] {
            fs::DirBuilder::new().mode(0o700).create(path).unwrap();
        }
        let pin = |name: &str, executable: bool| {
            let path = root.join(name);
            fs::write(&path, name).unwrap();
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if executable { 0o500 } else { 0o400 }),
            )
            .unwrap();
            FiniteClientFile {
                path: path.to_str().unwrap().into(),
                sha256: content_digest(name.as_bytes()),
            }
        };
        let broker =
            OwnedBroker::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap()).unwrap();
        let manifest = FiniteClientManifest {
            schema_version: 1,
            task_id: "resource-task".into(),
            host_snapshot_id: "fixture-host".into(),
            client: pin("client", true),
            native: pin("native", true),
            helper: pin("helper", true),
            wrapper: pin("wrapper", true),
            profile: pin("profile", false),
            configuration: pin("config", false),
            private_state: state.to_str().unwrap().into(),
            native_version: "fixture-1".into(),
            broker_endpoint: broker.endpoint().into(),
            broker_instance_sha256: broker.instance_sha256().into(),
        };
        let run = PreparedRun {
            schema_version: 1,
            receipt_id: "a".repeat(64),
            source_state: "b".repeat(40),
            scope: vec!["patch.txt".into()],
            authorization_digest: "c".repeat(64),
            snapshot_hash: "d".repeat(64),
            host: HarnessId::parse("codex").unwrap(),
            requested_model: ModelId::parse("fixture-model").unwrap(),
            requested_effort: EffortLevel::Low,
            program: "finite-client".into(),
            argv: vec!["--worker".into()],
            cwd: workspace.to_str().unwrap().into(),
            stdin: PreparedInput::new(b"task".to_vec()),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            sandbox: ExecutionSandbox::WorkspaceWrite,
            timeout_ms: 3000,
            max_output_bytes: 4096,
            degradations: vec![],
            missing_runtime_evidence: vec![],
            parent_model_changed: false,
        };
        let limits = ResourceLimits::default();
        let mut review =
            FiniteClientReview::with_resources(manifest.clone(), &run, broker, limits.clone())
                .unwrap();
        let approval = review.approval_digest().to_owned();
        for changed in [
            ResourceLimits {
                cpu_seconds: 301,
                ..limits.clone()
            },
            ResourceLimits {
                address_space_bytes: None,
                ..limits.clone()
            },
            ResourceLimits {
                max_open_files: 257,
                ..limits.clone()
            },
            ResourceLimits {
                max_processes: 257,
                ..limits.clone()
            },
            ResourceLimits {
                max_file_bytes: 1024,
                ..limits.clone()
            },
        ] {
            // Move the exact listener instance, retaining the same files and modes.
            let changed_review =
                FiniteClientReview::with_resources(manifest.clone(), &run, review.broker, changed)
                    .unwrap();
            assert_ne!(approval, changed_review.approval_digest());
            review = FiniteClientReview::with_resources(
                manifest.clone(),
                &run,
                changed_review.broker,
                limits.clone(),
            )
            .unwrap();
            assert_eq!(approval, review.approval_digest());
        }
    }
}
