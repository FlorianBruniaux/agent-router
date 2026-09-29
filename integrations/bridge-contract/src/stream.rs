use router_adapter_api::{
    AdapterError, MAX_TRANSPORT_BYTES, MAX_TRANSPORT_EVENTS, content_digest, safe_identifier,
    strict_json,
};
use router_types::{
    ModelId, ObservedModel, RuntimeEvidence, TransportEvent, TransportEventKind, TransportOutcome,
    TransportTermination, VerificationOutcome,
};

/// True when `bytes` end on a frame boundary: a blank line once CRLF is read as
/// LF. The decoder and the transport share this one rule.
pub(crate) fn ends_at_frame_boundary(bytes: &[u8]) -> bool {
    let tail = &bytes[bytes.len().saturating_sub(4)..];
    String::from_utf8_lossy(tail)
        .replace("\r\n", "\n")
        .ends_with("\n\n")
}

/// Bounded SSE normalization. Provider text is discarded, and no assistant
/// completion claim becomes objective or validation evidence.
pub(crate) fn decode_sse(
    bytes: &[u8],
    outcome: TransportOutcome,
    max_bytes: usize,
    openai: bool,
) -> Result<RuntimeEvidence, AdapterError> {
    if max_bytes == 0 || max_bytes > MAX_TRANSPORT_BYTES || bytes.len() > max_bytes {
        return Err(AdapterError::OutputLimit);
    }
    if outcome.termination == TransportTermination::Exited && outcome.exit_code.is_none() {
        return Err(AdapterError::Invalid("transport status"));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AdapterError::Protocol)?
        .replace("\r\n", "\n");
    let mut events = Vec::new();
    let mut models = Vec::new();
    let mut terminal = false;
    let mut partial = false;
    for frame in text.split("\n\n").filter(|frame| !frame.trim().is_empty()) {
        if frame.len() > 256 * 1024 || events.len() >= MAX_TRANSPORT_EVENTS {
            return Err(AdapterError::OutputLimit);
        }
        let final_unterminated = !text.ends_with("\n\n") && text.ends_with(frame);
        if final_unterminated {
            partial = true;
        }
        let mut data = String::new();
        let mut declared_type = None;
        for line in frame.lines() {
            if let Some(value) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
            } else if let Some(value) = line.strip_prefix("event:") {
                if declared_type.is_some() {
                    return Err(AdapterError::Protocol);
                }
                declared_type = Some(value.trim());
            } else if line.starts_with(':')
                || line.starts_with("id:")
                || line.starts_with("retry:")
                || line.is_empty()
            {
            } else {
                return Err(AdapterError::Protocol);
            }
        }
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            continue;
        } // Sentinel alone cannot prove an API result.
        let value = match strict_json(data.as_bytes()) {
            Ok(v) => v,
            Err(_) if final_unterminated && outcome.termination != TransportTermination::Exited => {
                break;
            }
            Err(_) => return Err(AdapterError::Protocol),
        };
        let kind = value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or(AdapterError::Protocol)?;
        if declared_type.is_some_and(|declared| declared != kind) {
            return Err(AdapterError::Protocol);
        }
        let (event, model_pointer) = match (openai, kind) {
            (true, "response.created") => (TransportEventKind::SessionStarted, None),
            (true, "response.output_text.delta" | "response.output_text.done") => {
                (TransportEventKind::AssistantMessage, None)
            }
            (true, "response.completed")
                if value.pointer("/response/status").and_then(|v| v.as_str())
                    == Some("completed") =>
            {
                terminal = true;
                (TransportEventKind::TurnCompleted, Some("/response/model"))
            }
            (true, "response.failed" | "response.incomplete") | (false, "error") => {
                terminal = true;
                (TransportEventKind::HostError, None)
            }
            (false, "message_start")
                if value.pointer("/message/type").and_then(|v| v.as_str()) == Some("message")
                    && value.pointer("/message/role").and_then(|v| v.as_str())
                        == Some("assistant") =>
            {
                (TransportEventKind::SessionStarted, Some("/message/model"))
            }
            (false, "content_block_delta") => (TransportEventKind::AssistantMessage, None),
            (false, "message_stop") => {
                terminal = true;
                (TransportEventKind::TurnCompleted, None)
            }
            _ => (TransportEventKind::Ignored, None),
        };
        if let Some(model) = model_pointer
            .and_then(|pointer| value.pointer(pointer))
            .and_then(|v| v.as_str())
            .filter(|model| safe_identifier(model))
        {
            let model = ModelId::parse(model).map_err(|_| AdapterError::Protocol)?;
            if !models.iter().any(|m: &ObservedModel| m.model == model) {
                models.push(ObservedModel {
                    model,
                    source: if openai {
                        "supervised.openai.response.model"
                    } else {
                        "supervised.anthropic.message.model"
                    }
                    .into(),
                    source_sha256: content_digest(data.as_bytes()),
                });
            }
        }
        events.push(TransportEvent {
            sequence: events.len(),
            kind: event,
        });
    }
    // Anything after the last blank line, even whitespace, is an unfinished frame.
    partial |= !ends_at_frame_boundary(bytes);
    models.sort_by(|a, b| a.model.cmp(&b.model));
    let mut missing = vec![
        "validation.not_run".into(),
        "objective.not_verified".into(),
        "effort.runtime_unknown".into(),
    ];
    if models.is_empty() {
        missing.push("model.runtime_unknown".into());
    }
    if partial || !terminal {
        missing.push("stream.incomplete".into());
    }
    Ok(RuntimeEvidence {
        schema_version: 1,
        transport_success: outcome.termination == TransportTermination::Exited
            && outcome.exit_code == Some(0),
        transport: outcome,
        stream_complete: terminal && !partial,
        validation: VerificationOutcome::Unknown,
        objective: VerificationOutcome::Unknown,
        executed_models: models,
        executed_effort: None,
        events,
        missing_evidence: missing,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum Dialect {
    Copilot,
    OpenCode,
}

pub(crate) fn decode(
    bytes: &[u8],
    outcome: TransportOutcome,
    max_bytes: usize,
    dialect: Dialect,
) -> Result<RuntimeEvidence, AdapterError> {
    if max_bytes == 0 || max_bytes > MAX_TRANSPORT_BYTES || bytes.len() > max_bytes {
        return Err(AdapterError::OutputLimit);
    }
    if outcome.termination == TransportTermination::Exited && outcome.exit_code.is_none() {
        return Err(AdapterError::Invalid("transport status"));
    }
    let mut events = Vec::new();
    let mut models = Vec::new();
    let mut truncated = false;
    let mut terminal = false;
    for (sequence, line) in bytes
        .split(|b| *b == b'\n')
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
                truncated = true;
                break;
            }
            Err(_) => return Err(AdapterError::Protocol),
        };
        let kind = value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or(AdapterError::Protocol)?;
        let event = match (dialect, kind) {
            (Dialect::Copilot, "session.start") => TransportEventKind::SessionStarted,
            (Dialect::Copilot, "assistant.turn_start") | (Dialect::OpenCode, "step_start") => {
                TransportEventKind::TurnStarted
            }
            (Dialect::Copilot, "assistant.message" | "assistant.message_delta")
            | (Dialect::OpenCode, "text" | "reasoning") => TransportEventKind::AssistantMessage,
            (Dialect::Copilot, "session.idle") => {
                terminal = true;
                TransportEventKind::TurnCompleted
            }
            (Dialect::OpenCode, "step_finish") => {
                if value.pointer("/part/reason").and_then(|v| v.as_str()) == Some("stop") {
                    terminal = true;
                }
                TransportEventKind::TurnCompleted
            }
            (Dialect::Copilot, "session.error") | (Dialect::OpenCode, "error") => {
                terminal = true;
                TransportEventKind::HostError
            }
            (Dialect::Copilot, "tool.execution_start" | "tool.execution_complete")
            | (Dialect::OpenCode, "tool_use") => TransportEventKind::ToolActivity,
            (Dialect::Copilot, "assistant.usage") => {
                // Child-agent usage is separate and never proves the worker model.
                if value.get("agentId").is_none() && value.pointer("/data/initiator").is_none() {
                    if let Some(id) = value
                        .pointer("/data/model")
                        .and_then(|v| v.as_str())
                        .filter(|id| safe_identifier(id))
                    {
                        let model = ModelId::parse(id).map_err(|_| AdapterError::Protocol)?;
                        if !models.iter().any(|m: &ObservedModel| m.model == model) {
                            models.push(ObservedModel {
                                model,
                                source: "supervised.copilot.assistant.usage.model".into(),
                                source_sha256: content_digest(line),
                            });
                        }
                    }
                }
                TransportEventKind::Ignored
            }
            _ => TransportEventKind::Ignored,
        };
        events.push(TransportEvent {
            sequence,
            kind: event,
        });
    }
    models.sort_by(|a, b| a.model.cmp(&b.model));
    let mut missing = vec![
        "validation.not_run".into(),
        "objective.not_verified".into(),
        "effort.runtime_unknown".into(),
    ];
    if models.is_empty() {
        missing.push("model.runtime_unknown".into());
    }
    if !terminal || truncated {
        missing.push("stream.incomplete".into());
    }
    Ok(RuntimeEvidence {
        schema_version: 1,
        transport_success: outcome.termination == TransportTermination::Exited
            && outcome.exit_code == Some(0),
        transport: outcome,
        stream_complete: terminal && !truncated,
        validation: VerificationOutcome::Unknown,
        objective: VerificationOutcome::Unknown,
        executed_models: models,
        executed_effort: None,
        events,
        missing_evidence: missing,
    })
}
