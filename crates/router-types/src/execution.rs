//! Host preparation and observed transport, separate from objective verification.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Capability, EffortLevel, HarnessId, ModelId, SkillId, SkillTrust, StageKind};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    DryRun,
    Shadow,
    Execute,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSandbox {
    ReadOnly,
    WorkspaceWrite,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTarget {
    ExternalWorker,
    NativeSubagent,
    ParentConversation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSelection {
    HostPolicy,
    Disabled,
    AllowList(Vec<String>),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSkillKind {
    Instruction,
    Executable,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSkill {
    pub id: SkillId,
    pub content_sha256: String,
    pub kind: ExecutionSkillKind,
    pub trust: SkillTrust,
    pub body: String,
}

impl fmt::Debug for ExecutionSkill {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionSkill")
            .field("id", &self.id)
            .field("body", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRequest {
    pub schema_version: u32,
    pub receipt_id: String,
    pub candidate_id: String,
    pub phase: StageKind,
    pub host: HarnessId,
    pub model: ModelId,
    pub effort: EffortLevel,
    pub workspace: String,
    pub source_state: String,
    pub scope: Vec<String>,
    pub sandbox: ExecutionSandbox,
    pub required_capabilities: BTreeSet<Capability>,
    pub tools: ToolSelection,
    pub agent: Option<String>,
    pub skills: Vec<ExecutionSkill>,
    pub prompt: String,
    pub mode: ExecutionMode,
    pub target: ExecutionTarget,
    pub allow_degradation: bool,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

impl fmt::Debug for ExecutionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionRequest")
            .field("receipt_id", &self.receipt_id)
            .field("host", &self.host)
            .field("model", &self.model)
            .field("prompt", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterDegradation {
    pub code: String,
    pub requested: String,
    pub applied: String,
}

/// Omitted from serialised previews and debug logs. The supervisor alone feeds
/// these bytes to the child. A preview cannot be deserialised into an execution.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedInput(Vec<u8>);

impl PreparedInput {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for PreparedInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreparedRun {
    pub schema_version: u32,
    pub receipt_id: String,
    pub source_state: String,
    pub scope: Vec<String>,
    pub authorization_digest: String,
    pub snapshot_hash: String,
    pub host: HarnessId,
    pub requested_model: ModelId,
    pub requested_effort: EffortLevel,
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: String,
    #[serde(skip_serializing)]
    pub stdin: PreparedInput,
    pub mode: ExecutionMode,
    pub target: ExecutionTarget,
    pub sandbox: ExecutionSandbox,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub degradations: Vec<AdapterDegradation>,
    pub missing_runtime_evidence: Vec<String>,
    /// The parent conversation is never reconfigured by an external worker.
    pub parent_model_changed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportTermination {
    Exited,
    TimedOut,
    Cancelled,
    Killed,
    SpawnFailed,
}

/// Supplied by the supervising process, never inferred from assistant text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportOutcome {
    pub termination: TransportTermination,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Unknown,
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportEventKind {
    SessionStarted,
    TurnStarted,
    AssistantMessage,
    ToolActivity,
    TurnCompleted,
    HostError,
    Ignored,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportEvent {
    pub sequence: usize,
    pub kind: TransportEventKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedModel {
    pub model: ModelId,
    pub source: String,
    pub source_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeEvidence {
    pub schema_version: u32,
    pub transport: TransportOutcome,
    pub transport_success: bool,
    pub stream_complete: bool,
    pub validation: VerificationOutcome,
    pub objective: VerificationOutcome,
    pub executed_models: Vec<ObservedModel>,
    pub executed_effort: Option<EffortLevel>,
    pub events: Vec<TransportEvent>,
    pub missing_evidence: Vec<String>,
}
