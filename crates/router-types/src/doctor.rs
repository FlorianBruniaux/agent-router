//! Redacted diagnostics and audit exchange contracts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorArtifactKind {
    JsonConfiguration,
    Catalog,
    HostSnapshot,
    RetrievalIndex,
    Database,
    Hook,
    Plugin,
    ManagedFile,
    AuthMarker,
    Executable,
    Lease,
    BackendStatus,
    SandboxEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorArtifact {
    pub id: String,
    pub relative_path: String,
    pub kind: DoctorArtifactKind,
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub require_private: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorRequest {
    pub schema_version: u32,
    pub root: String,
    pub artifacts: Vec<DoctorArtifact>,
    pub max_snapshot_age_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticState {
    Pass,
    Warning,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorCode {
    NotConfigured,
    FileValidated,
    FileUnavailable,
    PathRejected,
    PermissionsBroad,
    DriftDetected,
    SchemaInvalid,
    InputLimit,
    SecretPatternDetected,
    AuthPresenceOnly,
    ExecutablePresenceOnly,
    DatabaseHeaderOnly,
    HookNotInvoked,
    PluginNotLoaded,
    ManagedFilePresent,
    HostObserved,
    HostEffortUnknown,
    RetrievalValidated,
    LeaseLivenessUnknown,
    LeaseStale,
    BackendHealthUnknown,
    SandboxEnforcementUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorCheck {
    pub subject_sha256: String,
    pub kind: DoctorArtifactKind,
    pub configuration: DiagnosticState,
    pub runtime: DiagnosticState,
    pub code: DoctorCode,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub observed_at_unix_seconds: u64,
    pub checks: Vec<DoctorCheck>,
    pub overall: DiagnosticState,
    pub secrets_scan_is_heuristic: bool,
    pub network_requests: u32,
    pub processes_launched: u32,
    pub files_modified: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditArtifactKind {
    EffectiveConfig,
    Catalog,
    Decision,
    Events,
    Evidence,
    Versions,
}

/// Only fixed metadata and numbers survive projection. Arbitrary JSON values,
/// user labels, file paths, prompts and provider text cannot enter this type.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditMetadata {
    pub schema_version: Option<u32>,
    pub top_level_entries: usize,
    pub contained_secret_pattern: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    pub sequence: u64,
    pub kind: AuditArtifactKind,
    pub content_sha256: String,
    pub byte_count: usize,
    pub metadata: AuditMetadata,
    pub previous_hash: String,
    pub record_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditBundle {
    pub schema_version: u32,
    pub records: Vec<AuditRecord>,
    pub root_hash: String,
    pub raw_content_included: bool,
    pub signature_verified: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplementalBridgeKind {
    GitHubCopilotCli,
    OpenCode,
    OpenAiCompatibleApi,
    AnthropicCompatibleApi,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeLayer {
    Harness,
    Provider,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDescriptor {
    pub schema_version: u32,
    pub kind: SupplementalBridgeKind,
    pub layer: BridgeLayer,
    pub capabilities: std::collections::BTreeMap<crate::Capability, crate::CapabilitySupport>,
    pub transport: crate::CapabilitySupport,
    pub authentication: crate::CapabilitySupport,
    pub parent_model_change: crate::CapabilitySupport,
}

/// Context is kept outside the provider payload. Hashes bind the intended
/// values but do not authenticate the envelope; the provider gate must verify
/// runtime authority before using one. No raw context or credentials here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRouteEnvelope {
    pub schema_version: u32,
    pub run_id: String,
    pub receipt_id: String,
    pub provider: SupplementalBridgeKind,
    pub requested_model: crate::ModelId,
    pub requested_effort: crate::EffortLevel,
    pub mode: crate::ExecutionMode,
    pub payload_sha256: String,
    pub context_sha256: String,
    pub authorization_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedProviderRouteEnvelope {
    pub envelope: ProviderRouteEnvelope,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreparedProviderRequest {
    pub signed_route: SignedProviderRouteEnvelope,
    pub method: String,
    pub path: String,
    pub static_headers: std::collections::BTreeMap<String, String>,
    pub credential_header_name: String,
    #[serde(skip_serializing)]
    pub body: crate::PreparedInput,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}
