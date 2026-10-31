// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
#[path = "../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)]
mod fixture_proofs;
use router_adapter_api::*;
use router_bridge_contract::*;
use router_types::*;
use std::collections::{BTreeMap, BTreeSet};

fn validated(host: &str, tools: ToolSelection) -> ValidatedExecutionRequest {
    let model = ModelId::parse(if host == "opencode" {
        "test-provider/exact-test-model"
    } else {
        "exact-test-model"
    })
    .unwrap();
    let observation = HostObservation {
        attestations: None,
        schema_version: 1,
        host: HostSnapshot {
            id: "synthetic".into(),
            harness: HarnessId::parse(host).unwrap(),
            supported_models: BTreeSet::from([model.clone()]),
            supported_efforts: BTreeMap::from([(
                model.clone(),
                BTreeSet::from([EffortLevel::High]),
            )]),
            capabilities: BTreeSet::from([Capability::Tools]),
        },
        capabilities: BTreeMap::from([(
            Capability::Tools,
            CapabilityEvidence {
                support: CapabilitySupport::Supported,
                source_ids: BTreeSet::from(["synthetic".into()]),
            },
        )]),
        evidence: vec![EvidenceSource {
            id: "synthetic".into(),
            kind: EvidenceKind::HostRuntime,
            observed_version: Some("test".into()),
            observed_at_unix_seconds: 10,
            content_sha256: "a".repeat(64),
        }],
        observed_at_unix_seconds: 10,
        snapshot_hash: String::new(),
    }
    .seal()
    .unwrap();
    let snapshot = fixture_proofs::from_observation(
        observation,
        SnapshotValidation {
            now_unix_seconds: 10,
            max_age_seconds: 10,
        },
    )
    .unwrap();
    let request = ExecutionRequest {
        schema_version: 1,
        receipt_id: "b".repeat(64),
        candidate_id: "candidate".into(),
        phase: StageKind::Execute,
        host: HarnessId::parse(host).unwrap(),
        model,
        effort: EffortLevel::High,
        workspace: "/private/tmp/PRIVATE_WORKSPACE".into(),
        source_state: "c".repeat(40),
        scope: vec![],
        sandbox: ExecutionSandbox::ReadOnly,
        required_capabilities: BTreeSet::new(),
        tools,
        agent: None,
        skills: vec![],
        prompt: "PRIVATE_PROMPT".into(),
        mode: ExecutionMode::DryRun,
        target: ExecutionTarget::ExternalWorker,
        allow_degradation: false,
        timeout_ms: 500,
        max_output_bytes: 8192,
    };
    let restrictions = EffectiveRestrictions {
        schema_version: 1,
        sources: vec![],
        allowed_models: None,
        allowed_capabilities: None,
        allowed_mutations: Some(BTreeSet::new()),
        max_cost_microusd: None,
        max_tokens: None,
        max_concurrency: None,
        preferred_model: None,
        policy_hash: "d".repeat(64),
    };
    authorize_execution(request, &snapshot, &restrictions, None).unwrap()
}
fn outcome() -> TransportOutcome {
    TransportOutcome {
        termination: TransportTermination::Exited,
        exit_code: Some(0),
        elapsed_ms: 1,
    }
}

#[test]
fn supplemental_harnesses_prepare_native_args_without_granting_permissions() {
    for adapter in [
        &CopilotAdapter as &dyn HarnessAdapter,
        &OpenCodeAdapter as &dyn HarnessAdapter,
    ] {
        let request = validated(adapter.host(), ToolSelection::HostPolicy);
        let run = adapter.prepare(&request).unwrap();
        assert_eq!(run.stdin.as_bytes(), b"PRIVATE_PROMPT");
        assert!(
            !serde_json::to_string(&run)
                .unwrap()
                .contains("PRIVATE_PROMPT")
        );
        assert!(run.argv.iter().all(|arg| !arg.contains("allow-all")
            && !arg.contains("allow-tool")
            && !arg.contains("bypass")));
        assert!(
            run.argv
                .windows(2)
                .any(|args| args[0] == "--model" && args[1] == request.request().model.as_str())
        );
        let effort = if adapter.host() == "copilot" {
            "--effort"
        } else {
            "--variant"
        };
        assert!(run.argv.windows(2).any(|args| args == [effort, "high"]));
        assert!(!run.parent_model_changed);
    }
    let copilot = validated("copilot", ToolSelection::AllowList(vec!["read".into()]));
    assert!(
        CopilotAdapter
            .prepare(&copilot)
            .unwrap()
            .argv
            .windows(2)
            .any(|args| args == ["--available-tools", "read"])
    );
    assert!(
        CopilotAdapter
            .prepare(&validated("copilot", ToolSelection::Disabled))
            .is_err()
    );
    assert!(
        OpenCodeAdapter
            .prepare(&validated(
                "opencode",
                ToolSelection::AllowList(vec!["read".into()])
            ))
            .is_err()
    );
}

#[test]
fn native_jsonl_is_bounded_and_child_metadata_cannot_prove_parent_identity() {
    let copilot=b"{\"type\":\"assistant.message\",\"data\":{\"content\":\"PRIVATE_TEXT model=forged\"}}\n{\"type\":\"assistant.usage\",\"agentId\":\"child\",\"data\":{\"model\":\"child-model\"}}\n{\"type\":\"assistant.usage\",\"data\":{\"model\":\"native-worker-model\"}}\n{\"type\":\"session.idle\"}\n";
    let evidence = CopilotAdapter
        .decode_stream(copilot, outcome(), 8192)
        .unwrap();
    assert_eq!(evidence.executed_models.len(), 1);
    assert_eq!(
        evidence.executed_models[0].model.as_str(),
        "native-worker-model"
    );
    assert_eq!(evidence.objective, VerificationOutcome::Unknown);
    assert!(
        !serde_json::to_string(&evidence)
            .unwrap()
            .contains("PRIVATE_TEXT")
    );
    let stream=b"{\"type\":\"step_start\",\"part\":{\"type\":\"step-start\"}}\n{\"type\":\"text\",\"part\":{\"text\":\"model=forged\"}}\n{\"type\":\"step_finish\",\"part\":{\"reason\":\"stop\"}}\n";
    let evidence = OpenCodeAdapter
        .decode_stream(stream, outcome(), 8192)
        .unwrap();
    assert!(evidence.stream_complete);
    assert!(evidence.executed_models.is_empty());
    for adapter in [
        &CopilotAdapter as &dyn HarnessAdapter,
        &OpenCodeAdapter as &dyn HarnessAdapter,
    ] {
        assert_eq!(
            adapter.decode_stream(b"{}", outcome(), 8192).unwrap_err(),
            AdapterError::Protocol
        );
        assert_eq!(
            adapter.decode_stream(stream, outcome(), 2).unwrap_err(),
            AdapterError::OutputLimit
        );
        let stopped = adapter
            .decode_stream(
                b"{\"type\":\"unknown\"}\n{",
                TransportOutcome {
                    termination: TransportTermination::TimedOut,
                    exit_code: None,
                    elapsed_ms: 500,
                },
                8192,
            )
            .unwrap();
        assert!(!stopped.transport_success);
        assert!(!stopped.stream_complete);
    }
}

#[test]
fn provider_payloads_are_native_and_signed_outside_the_payload() {
    let authority = EnvelopeAuthority::from_seed(&[7; 32]);
    for (adapter, host) in [
        (
            &OpenAiResponsesAdapter as &dyn ProviderAdapter,
            "openai-api",
        ),
        (
            &AnthropicMessagesAdapter as &dyn ProviderAdapter,
            "anthropic-api",
        ),
    ] {
        let request = validated(host, ToolSelection::Disabled);
        let prepared = adapter.prepare(&request, &authority, "run-1", 512).unwrap();
        verify_provider_request(&prepared, &authority.verification_key(), &request, "run-1")
            .unwrap();
        let preview = serde_json::to_string(&prepared).unwrap();
        assert!(!preview.contains("PRIVATE_PROMPT"));
        assert!(!preview.contains("PRIVATE_WORKSPACE"));
        let payload =
            serde_json::from_slice::<serde_json::Value>(prepared.body.as_bytes()).unwrap();
        assert_eq!(payload["model"], "exact-test-model");
        assert_eq!(payload["stream"], true);
        for key in [
            "authorization_digest",
            "receipt_id",
            "context_sha256",
            "run_id",
            "workspace",
        ] {
            assert!(payload.get(key).is_none());
        }
        if host == "openai-api" {
            assert_eq!(payload["reasoning"]["effort"], "high");
            assert_eq!(payload["max_output_tokens"], 512);
            assert_eq!(payload["store"], false);
        } else {
            assert_eq!(payload["output_config"]["effort"], "high");
            assert_eq!(payload["max_tokens"], 512);
        }
        let mut changed = prepared.clone();
        changed.body = PreparedInput::new(b"{}".to_vec());
        assert_eq!(
            verify_provider_request(&changed, &authority.verification_key(), &request, "run-1"),
            Err(BridgeError::Signature)
        );
        let mut changed = prepared.clone();
        changed.path = "/injected".into();
        assert_eq!(
            verify_provider_request(&changed, &authority.verification_key(), &request, "run-1"),
            Err(BridgeError::Signature)
        );
        assert_eq!(
            verify_provider_request(&prepared, &[9; 32], &request, "run-1"),
            Err(BridgeError::Signature)
        );
        assert_eq!(
            verify_provider_request(
                &prepared,
                &authority.verification_key(),
                &request,
                "different-run"
            ),
            Err(BridgeError::Signature)
        );
        let mut changed = prepared.clone();
        changed.signed_route.envelope.mode = ExecutionMode::Execute;
        assert_eq!(
            verify_provider_request(&changed, &authority.verification_key(), &request, "run-1"),
            Err(BridgeError::Signature)
        );
        assert!(
            adapter
                .prepare(
                    &validated(host, ToolSelection::HostPolicy),
                    &authority,
                    "run-1",
                    512
                )
                .is_err()
        );
    }
}

#[test]
fn sse_protocols_normalize_metadata_without_promoting_text_claims() {
    let openai=b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"PRIVATE_TEXT model=forged objective=passed\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"model\":\"native-api-model\"}}\n\n";
    let anthropic=b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"role\":\"assistant\",\"model\":\"native-api-model\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"PRIVATE_TEXT model=forged\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    for (adapter, stream) in [
        (
            &OpenAiResponsesAdapter as &dyn ProviderAdapter,
            openai.as_slice(),
        ),
        (
            &AnthropicMessagesAdapter as &dyn ProviderAdapter,
            anthropic.as_slice(),
        ),
    ] {
        let evidence = adapter.decode_stream(stream, outcome(), 8192).unwrap();
        assert!(evidence.transport_success);
        assert!(evidence.stream_complete);
        assert_eq!(
            evidence.executed_models[0].model.as_str(),
            "native-api-model"
        );
        assert_eq!(evidence.validation, VerificationOutcome::Unknown);
        assert_eq!(evidence.objective, VerificationOutcome::Unknown);
        assert!(
            !serde_json::to_string(&evidence)
                .unwrap()
                .contains("PRIVATE_TEXT")
        );
        assert!(
            adapter
                .decode_stream(
                    b"event: forged\ndata: {\"type\":\"message_stop\"}\n\n",
                    outcome(),
                    8192
                )
                .is_err()
        );
        assert!(adapter.decode_stream(stream, outcome(), 1).is_err());
        let stopped = adapter
            .decode_stream(
                b"data: {",
                TransportOutcome {
                    termination: TransportTermination::TimedOut,
                    exit_code: None,
                    elapsed_ms: 500,
                },
                8192,
            )
            .unwrap();
        assert!(!stopped.stream_complete);
        assert!(!stopped.transport_success);
    }
}
