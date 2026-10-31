// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0

//! Regression scenarios from the claude-router analysis (issue #143), applied to
//! the Claude stream decoder. Rows: T06, T07, T11.
//! See `docs/claude-router-regression-matrix.md`.
//!
//! Streams are synthetic. A decoded stream is transport evidence, never proof
//! of the applied effort, the objective or the validation outcome.
use adapter_claude::ClaudeAdapter;
use router_adapter_api::HarnessAdapter;
use router_types::{
    RuntimeEvidence, TransportEventKind, TransportOutcome, TransportTermination,
    VerificationOutcome,
};
use serde_json::{Value, json};

fn init(model: &str) -> Value {
    json!({"type":"system","subtype":"init","claude_code_version":"2.1.283","model":model,"per_turn_effort_active":true})
}

fn assistant(model: &str) -> Value {
    json!({"type":"assistant","message":{"type":"message","role":"assistant","model":model}})
}

fn subagent_assistant(model: &str, marker: &str) -> Value {
    let mut event = assistant(model);
    event[marker] = json!("subagent-marker-1");
    event
}

fn tool_error() -> Value {
    json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":"permission denied"}]}})
}

fn result(is_error: bool) -> Value {
    json!({"type":"result","subtype":if is_error {"error_during_execution"} else {"success"},"is_error":is_error})
}

fn decode(events: &[Value], code: i32) -> RuntimeEvidence {
    let stream: String = events
        .iter()
        .map(|event| event.to_string() + "\n")
        .collect();
    ClaudeAdapter
        .decode_stream(
            stream.as_bytes(),
            TransportOutcome {
                termination: TransportTermination::Exited,
                exit_code: Some(code),
                elapsed_ms: 1,
            },
            65536,
        )
        .unwrap()
}

fn models(evidence: &RuntimeEvidence) -> Vec<&str> {
    evidence
        .executed_models
        .iter()
        .map(|observed| observed.model.as_str())
        .collect()
}

fn has(evidence: &RuntimeEvidence, code: &str) -> bool {
    evidence.missing_evidence.iter().any(|value| value == code)
}

/// T07: the model named at session start is a request or an alias. Only the
/// root assistant message names the served model, and effort is never inferred.
#[test]
fn t07_requested_alias_is_not_promoted_to_the_served_model() {
    let served = decode(
        &[
            init("claude-requested-alias"),
            assistant("claude-served-model-20260101"),
            result(false),
        ],
        0,
    );
    assert_eq!(models(&served), ["claude-served-model-20260101"]);
    assert!(!has(&served, "model.runtime_unknown"));
    assert!(
        !serde_json::to_string(&served)
            .unwrap()
            .contains("requested-alias")
    );

    // Without a root assistant message nothing was observed, even though the
    // init event names a model and the process succeeded.
    let unobserved = decode(&[init("claude-requested-alias"), result(false)], 0);
    assert!(unobserved.executed_models.is_empty());
    assert!(has(&unobserved, "model.runtime_unknown"));
    assert!(unobserved.transport_success);

    // An identifier that is not a safe label is dropped rather than recorded.
    let unsafe_label = decode(
        &[
            init("claude-requested-alias"),
            assistant("bad model/../x"),
            result(false),
        ],
        0,
    );
    assert!(unsafe_label.executed_models.is_empty());
    assert!(has(&unsafe_label, "model.runtime_unknown"));
}

/// T07: a fallback that changes the served model mid-run keeps both
/// identities, sorted and without duplicates. Effort stays unknown and
/// the objective stays unverified, whatever the stream claims.
#[test]
fn t07_mixed_served_models_stay_separate_and_effort_stays_unknown() {
    let evidence = decode(
        &[
            init("claude-requested-alias"),
            assistant("claude-served-b"),
            assistant("claude-served-a"),
            assistant("claude-served-b"),
            result(false),
        ],
        0,
    );
    assert_eq!(models(&evidence), ["claude-served-a", "claude-served-b"]);
    assert_eq!(evidence.executed_effort, None);
    assert!(has(&evidence, "effort.runtime_unknown"));
    assert_eq!(evidence.validation, VerificationOutcome::Unknown);
    assert_eq!(evidence.objective, VerificationOutcome::Unknown);
    assert!(has(&evidence, "objective.not_verified"));
}

/// T06: subagent, task and sampling events never name the root worker's model.
#[test]
fn t06_subagent_models_do_not_overwrite_the_root_identity() {
    for marker in ["parent_tool_use_id", "agentId", "subagent_type"] {
        let evidence = decode(
            &[
                init("claude-requested-alias"),
                assistant("claude-root-model"),
                subagent_assistant("claude-subagent-model", marker),
                result(false),
            ],
            0,
        );
        assert_eq!(models(&evidence), ["claude-root-model"], "{marker}");
    }
    // Only a subagent named a model: the root identity is unobserved.
    let only_subagent = decode(
        &[
            init("claude-requested-alias"),
            subagent_assistant("claude-subagent-model", "parent_tool_use_id"),
            result(false),
        ],
        0,
    );
    assert!(only_subagent.executed_models.is_empty());
    assert!(has(&only_subagent, "model.runtime_unknown"));
}

/// T11: two identical tool errors are tool activity. They do not become host
/// errors, and a clean result is not turned into a failed attempt.
#[test]
fn t11_identical_tool_errors_are_not_host_errors() {
    let count = |evidence: &RuntimeEvidence, kind: TransportEventKind| {
        evidence
            .events
            .iter()
            .filter(|event| event.kind == kind)
            .count()
    };
    let recovered = decode(
        &[
            init("claude-requested-alias"),
            assistant("claude-root-model"),
            tool_error(),
            tool_error(),
            result(false),
        ],
        0,
    );
    assert_eq!(count(&recovered, TransportEventKind::ToolActivity), 2);
    assert_eq!(count(&recovered, TransportEventKind::HostError), 0);
    assert!(recovered.transport_success);

    // Positive control: a real error result is exactly one host error, however
    // many tool errors preceded it.
    let failed = decode(
        &[
            init("claude-requested-alias"),
            assistant("claude-root-model"),
            tool_error(),
            tool_error(),
            result(true),
        ],
        1,
    );
    assert_eq!(count(&failed, TransportEventKind::ToolActivity), 2);
    assert_eq!(count(&failed, TransportEventKind::HostError), 1);
    assert!(!failed.transport_success);
}
