//! Shared preparation gates. The runtime must authenticate inputs, recompute the
//! route receipt and establish approval before supplying an approval digest.

use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

use router_types::{
    AdapterDegradation, Capability, CapabilitySupport, EffectiveRestrictions, ExecutionMode,
    ExecutionRequest, ExecutionSandbox, ExecutionSkillKind, ExecutionTarget, ModelId, Mutation,
    ObservedModel, PreparedInput, PreparedRun, RuntimeEvidence, SkillTrust, ToolSelection,
    TransportEvent, TransportEventKind, TransportOutcome, TransportTermination,
    VerificationOutcome,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{ValidatedHostObservation, strict_json};

pub const MAX_EXECUTION_INPUT_BYTES: usize = 128 * 1024;
pub const MAX_TRANSPORT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TRANSPORT_EVENTS: usize = 2048;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AdapterError {
    #[error("adapter.input.invalid: {0}")]
    Invalid(&'static str),
    #[error("adapter.host.mismatch")]
    HostMismatch,
    #[error("adapter.model.unsupported")]
    ModelUnsupported,
    #[error("adapter.effort.unsupported")]
    EffortUnsupported,
    #[error("adapter.capability.unknown_or_denied")]
    CapabilityDenied,
    #[error("adapter.permission.denied")]
    PermissionDenied,
    #[error("adapter.approval.required_or_mismatched")]
    ApprovalRequired,
    #[error("adapter.unsupported: {0}")]
    Unsupported(&'static str),
    #[error("adapter.output.limit")]
    OutputLimit,
    #[error("adapter.protocol.invalid")]
    Protocol,
}

/// Cannot be deserialised from an untrusted receipt or a provider response.
#[derive(Clone, Debug)]
pub struct ValidatedExecutionRequest {
    request: ExecutionRequest,
    snapshot_hash: String,
    authorization_digest: String,
}

impl ValidatedExecutionRequest {
    pub fn request(&self) -> &ExecutionRequest {
        &self.request
    }
    pub fn snapshot_hash(&self) -> &str {
        &self.snapshot_hash
    }
    pub fn authorization_digest(&self) -> &str {
        &self.authorization_digest
    }
}

pub trait HarnessAdapter {
    fn host(&self) -> &'static str;
    fn requires_native_version(&self) -> bool {
        false
    }
    /// Compare a freshly supervised --version response to the approved pin.
    fn validate_native_version(&self, _stdout: &[u8]) -> Result<(), AdapterError> {
        Err(AdapterError::Unsupported("native-version"))
    }
    /// Complete native answer text for an explicitly declared private artifact.
    /// Implementations must exclude tools, reasoning and incomplete streams.
    fn final_text(
        &self,
        _bytes: &[u8],
        _outcome: TransportOutcome,
        _max_bytes: usize,
    ) -> Result<Option<String>, AdapterError> {
        Err(AdapterError::Unsupported("native-final-text"))
    }
    fn prepare(&self, request: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError>;
    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, AdapterError>;
}

pub fn execution_authorization_digest(
    request: &ExecutionRequest,
    snapshot: &ValidatedHostObservation,
    restrictions: &EffectiveRestrictions,
) -> Result<String, AdapterError> {
    let mut hash = Sha256::new();
    hash.update(b"agent-router:execution-authorization:v1\0");
    hash.update(
        serde_json::to_vec(&(request, snapshot.hash(), restrictions))
            .map_err(|_| AdapterError::Invalid("serialization"))?,
    );
    Ok(hex::encode(hash.finalize()))
}

pub fn authorize_execution(
    request: ExecutionRequest,
    snapshot: &ValidatedHostObservation,
    restrictions: &EffectiveRestrictions,
    approved_digest: Option<&str>,
) -> Result<ValidatedExecutionRequest, AdapterError> {
    validate_request(&request)?;
    if restrictions.schema_version != 1 || !is_digest(&restrictions.policy_hash) {
        return Err(AdapterError::Invalid("restrictions"));
    }
    if request.host != snapshot.host().harness {
        return Err(AdapterError::HostMismatch);
    }
    if !snapshot.host().supported_models.contains(&request.model)
        || restrictions
            .allowed_models
            .as_ref()
            .is_some_and(|models| !models.contains(&request.model))
    {
        return Err(AdapterError::ModelUnsupported);
    }
    if !snapshot
        .host()
        .supported_efforts
        .get(&request.model)
        .is_some_and(|efforts| efforts.contains(&request.effort))
    {
        return Err(AdapterError::EffortUnsupported);
    }
    let mut required = request.required_capabilities.clone();
    if !matches!(request.tools, ToolSelection::Disabled) {
        required.insert(Capability::Tools);
    }
    if request.sandbox == ExecutionSandbox::WorkspaceWrite {
        required.insert(Capability::WorkspaceWrite);
    }
    let evidence = snapshot.capabilities();
    for capability in &required {
        if evidence.get(capability) != Some(&CapabilitySupport::Supported)
            || restrictions
                .allowed_capabilities
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(capability))
        {
            return Err(AdapterError::CapabilityDenied);
        }
    }
    for (needed, mutation) in [
        (
            request.sandbox == ExecutionSandbox::WorkspaceWrite,
            Mutation::WorkspaceWrite,
        ),
        (required.contains(&Capability::Network), Mutation::Network),
    ] {
        if needed
            && !restrictions
                .allowed_mutations
                .as_ref()
                .is_some_and(|allowed| allowed.contains(&mutation))
        {
            return Err(AdapterError::PermissionDenied);
        }
    }
    let authorization_digest = execution_authorization_digest(&request, snapshot, restrictions)?;
    if request.mode == ExecutionMode::Execute
        && approved_digest != Some(authorization_digest.as_str())
    {
        return Err(AdapterError::ApprovalRequired);
    }
    Ok(ValidatedExecutionRequest {
        request,
        snapshot_hash: snapshot.hash().into(),
        authorization_digest,
    })
}

fn validate_request(request: &ExecutionRequest) -> Result<(), AdapterError> {
    if request.schema_version != 1
        || !is_digest(&request.receipt_id)
        || !safe_identifier(&request.candidate_id)
        || !safe_identifier(request.model.as_str())
        || matches!(
            request.model.as_str(),
            "opus" | "sonnet" | "haiku" | "fable"
        )
        || !matches!(request.source_state.len(), 40 | 64)
        || !request
            .source_state
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AdapterError::Invalid("identity"));
    }
    let workspace = Path::new(&request.workspace);
    if !workspace.is_absolute()
        || workspace.components().count() < 2
        || workspace
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || request.workspace.contains(['\0', '\n', '\r'])
    {
        return Err(AdapterError::Invalid("workspace"));
    }
    if request.scope.len() > 256
        || (request.sandbox == ExecutionSandbox::WorkspaceWrite && request.scope.is_empty())
    {
        return Err(AdapterError::Invalid("scope"));
    }
    let mut scopes = BTreeSet::new();
    for scope in &request.scope {
        if scope.is_empty()
            || scope.len() > 1024
            || scope.contains(['\0', '\n', '\r', '\\'])
            || !Path::new(scope)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            || !scopes.insert(scope)
        {
            return Err(AdapterError::Invalid("scope"));
        }
    }
    if request.target == ExecutionTarget::ParentConversation {
        return Err(AdapterError::Unsupported("parent-model-change"));
    }
    if request
        .agent
        .as_ref()
        .is_some_and(|agent| !safe_identifier(agent))
    {
        return Err(AdapterError::Invalid("agent"));
    }
    if let ToolSelection::AllowList(tools) = &request.tools {
        let unique: BTreeSet<_> = tools.iter().collect();
        if tools.is_empty()
            || tools.len() > 64
            || unique.len() != tools.len()
            || tools.iter().any(|tool| !safe_identifier(tool))
        {
            return Err(AdapterError::Invalid("tools"));
        }
    }
    if request.prompt.trim().is_empty()
        || request.prompt.contains('\0')
        || request.skills.len() > 16
        || request.timeout_ms == 0
        || request.timeout_ms > 86_400_000
        || request.max_output_bytes == 0
        || request.max_output_bytes > MAX_TRANSPORT_BYTES
    {
        return Err(AdapterError::Invalid("bounds"));
    }
    let mut size = request.prompt.len();
    let mut skills = BTreeSet::new();
    for skill in &request.skills {
        if skill.kind != ExecutionSkillKind::Instruction {
            return Err(AdapterError::Unsupported("executable-skill"));
        }
        if !matches!(
            skill.trust,
            SkillTrust::Builtin | SkillTrust::TrustedLocal | SkillTrust::VerifiedThirdParty
        ) {
            return Err(AdapterError::PermissionDenied);
        }
        if !safe_identifier(skill.id.as_str())
            || !skills.insert(&skill.id)
            || skill.body.contains('\0')
            || !is_digest(&skill.content_sha256)
            || content_digest(skill.body.as_bytes()) != skill.content_sha256
        {
            return Err(AdapterError::Invalid("skill"));
        }
        size = size
            .checked_add(skill.body.len())
            .ok_or(AdapterError::Invalid("bounds"))?;
    }
    if size > MAX_EXECUTION_INPUT_BYTES {
        return Err(AdapterError::Invalid("bounds"));
    }
    Ok(())
}

pub fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

pub fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
pub fn content_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn worker_degradations(
    request: &ExecutionRequest,
    native_agent: bool,
) -> Result<Vec<AdapterDegradation>, AdapterError> {
    let mut result = Vec::new();
    if request.target == ExecutionTarget::NativeSubagent {
        if !request.allow_degradation {
            return Err(AdapterError::Unsupported("native-subagent-dispatch"));
        }
        result.push(AdapterDegradation {
            code: "target.external_worker".into(),
            requested: "native_subagent".into(),
            applied: "external_worker".into(),
        });
    }
    if request.agent.is_some() && !native_agent {
        if !request.allow_degradation {
            return Err(AdapterError::Unsupported("named-agent"));
        }
        result.push(AdapterDegradation {
            code: "agent.prompt_role".into(),
            requested: request.agent.clone().unwrap_or_default(),
            applied: "generic_worker_with_role_instruction".into(),
        });
    }
    Ok(result)
}

pub fn prepared_run(
    validated: &ValidatedExecutionRequest,
    program: &str,
    argv: Vec<String>,
    degradations: Vec<AdapterDegradation>,
) -> Result<PreparedRun, AdapterError> {
    let request = validated.request();
    let mut input = request.prompt.clone();
    if let Some(agent) = &request.agent {
        input.push_str(&format!("\n\nRequested role: {agent}\n"));
    }
    for skill in &request.skills {
        input.push_str(&format!(
            "\n\nSelected instruction {} (sha256={}):\n{}\n",
            skill.id, skill.content_sha256, skill.body
        ));
    }
    if input.len() > MAX_EXECUTION_INPUT_BYTES {
        return Err(AdapterError::Invalid("rendered prompt bounds"));
    }
    Ok(PreparedRun {
        schema_version: 1,
        receipt_id: request.receipt_id.clone(),
        authorization_digest: validated.authorization_digest.clone(),
        source_state: request.source_state.clone(),
        scope: request.scope.clone(),
        snapshot_hash: validated.snapshot_hash.clone(),
        host: request.host.clone(),
        requested_model: request.model.clone(),
        requested_effort: request.effort,
        program: program.into(),
        argv,
        cwd: request.workspace.clone(),
        stdin: PreparedInput::new(input.into_bytes()),
        mode: request.mode,
        target: ExecutionTarget::ExternalWorker,
        sandbox: request.sandbox,
        timeout_ms: request.timeout_ms,
        max_output_bytes: request.max_output_bytes,
        degradations,
        missing_runtime_evidence: vec![
            "sandbox.enforced_by_supervisor_and_host_not_proven_by_preparation".into(),
            "model.execution.not_observed".into(),
            "objective.not_verified".into(),
        ],
        parent_model_changed: false,
    })
}

#[derive(Clone, Copy)]
pub enum StreamDialect {
    Codex,
    Claude,
}

pub fn decode_transport(
    bytes: &[u8],
    outcome: TransportOutcome,
    max_bytes: usize,
    dialect: StreamDialect,
) -> Result<RuntimeEvidence, AdapterError> {
    if max_bytes == 0 || max_bytes > MAX_TRANSPORT_BYTES || bytes.len() > max_bytes {
        return Err(AdapterError::OutputLimit);
    }
    if outcome.termination == TransportTermination::Exited && outcome.exit_code.is_none() {
        return Err(AdapterError::Invalid("transport status"));
    }
    let mut events = Vec::new();
    let mut models = Vec::new();
    let mut stream_complete = true;
    for (sequence, line) in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        if sequence >= MAX_TRANSPORT_EVENTS || line.len() > 256 * 1024 {
            return Err(AdapterError::OutputLimit);
        }
        let value = match strict_json(line) {
            Ok(value) => value,
            Err(_)
                if outcome.termination != TransportTermination::Exited
                    && !bytes.ends_with(b"\n")
                    && bytes.ends_with(line) =>
            {
                stream_complete = false;
                break;
            }
            Err(_) => return Err(AdapterError::Protocol),
        };
        let kind = value
            .get("type")
            .and_then(|value| value.as_str())
            .ok_or(AdapterError::Protocol)?;
        let event = match (dialect, kind) {
            (StreamDialect::Codex, "thread.started") => TransportEventKind::SessionStarted,
            (StreamDialect::Codex, "turn.started") => TransportEventKind::TurnStarted,
            (StreamDialect::Codex, "turn.completed") => TransportEventKind::TurnCompleted,
            (StreamDialect::Codex, "turn.failed" | "error") => TransportEventKind::HostError,
            (StreamDialect::Codex, "item.completed")
                if value.pointer("/item/type").and_then(|v| v.as_str())
                    == Some("agent_message") =>
            {
                TransportEventKind::AssistantMessage
            }
            (StreamDialect::Codex, "item.started" | "item.completed" | "item.updated") => {
                TransportEventKind::ToolActivity
            }
            (StreamDialect::Claude, "system")
                if value.get("subtype").and_then(|v| v.as_str()) == Some("init") =>
            {
                TransportEventKind::SessionStarted
            }
            (StreamDialect::Claude, "assistant") => {
                if value.pointer("/message/role").and_then(|v| v.as_str()) == Some("assistant")
                    && value.pointer("/message/type").and_then(|v| v.as_str()) == Some("message")
                {
                    if let Some(model) = value
                        .pointer("/message/model")
                        .and_then(|v| v.as_str())
                        .filter(|id| safe_identifier(id))
                    {
                        let model = ModelId::parse(model).map_err(|_| AdapterError::Protocol)?;
                        if !models
                            .iter()
                            .any(|observed: &ObservedModel| observed.model == model)
                        {
                            models.push(ObservedModel {
                                model,
                                source: "supervised-native-assistant-message.model".into(),
                                source_sha256: content_digest(line),
                            });
                        }
                    }
                }
                TransportEventKind::AssistantMessage
            }
            (StreamDialect::Claude, "result")
                if value.get("is_error").and_then(|v| v.as_bool()) == Some(true) =>
            {
                TransportEventKind::HostError
            }
            (StreamDialect::Claude, "result")
                if value.get("is_error").and_then(|v| v.as_bool()) == Some(false) =>
            {
                TransportEventKind::TurnCompleted
            }
            (StreamDialect::Claude, "result") => return Err(AdapterError::Protocol),
            (StreamDialect::Claude, "user") => TransportEventKind::ToolActivity,
            _ => TransportEventKind::Ignored,
        };
        events.push(TransportEvent {
            sequence,
            kind: event,
        });
    }
    models.sort_by(|left, right| left.model.cmp(&right.model));
    stream_complete &= events.iter().any(|event| {
        matches!(
            event.kind,
            TransportEventKind::TurnCompleted | TransportEventKind::HostError
        )
    });
    let mut missing_evidence = vec![
        "validation.not_run".into(),
        "objective.not_verified".into(),
        "effort.runtime_unknown".into(),
    ];
    if models.is_empty() {
        missing_evidence.push("model.runtime_unknown".into());
    }
    if !stream_complete {
        missing_evidence.push("stream.truncated".into());
    }
    Ok(RuntimeEvidence {
        schema_version: 1,
        transport_success: outcome.termination == TransportTermination::Exited
            && outcome.exit_code == Some(0),
        transport: outcome,
        stream_complete,
        validation: VerificationOutcome::Unknown,
        objective: VerificationOutcome::Unknown,
        executed_models: models,
        executed_effort: None,
        events,
        missing_evidence,
    })
}
