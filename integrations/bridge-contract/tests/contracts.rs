use router_adapter_api::{AdapterError, HarnessAdapter};
use router_bridge_contract::*;
use router_types::*;

#[test]
fn every_extra_surface_is_explicitly_unknown_and_layered() {
    for kind in [
        SupplementalBridgeKind::GitHubCopilotCli,
        SupplementalBridgeKind::OpenCode,
        SupplementalBridgeKind::OpenAiCompatibleApi,
        SupplementalBridgeKind::AnthropicCompatibleApi,
    ] {
        let report = descriptor(kind);
        assert_eq!(report.transport, CapabilitySupport::Unknown);
        assert_eq!(report.authentication, CapabilitySupport::Unknown);
        assert!(
            report
                .capabilities
                .values()
                .all(|value| *value == CapabilitySupport::Unknown)
        );
        assert_eq!(report.capabilities.len(), 5);
        if report.layer == BridgeLayer::Harness {
            let bridge = DeferredHarnessBridge::new(kind).unwrap();
            assert_eq!(
                bridge
                    .decode_stream(
                        b"{\"success\":true}",
                        TransportOutcome {
                            termination: TransportTermination::Exited,
                            exit_code: Some(0),
                            elapsed_ms: 1
                        },
                        1024
                    )
                    .unwrap_err(),
                AdapterError::Unsupported("native-protocol-unverified")
            );
            assert_eq!(
                DeferredProviderBridge::new(kind).unwrap_err(),
                BridgeError::Layer
            );
        } else {
            assert_eq!(
                DeferredHarnessBridge::new(kind).unwrap_err(),
                BridgeError::Layer
            );
            let bridge = DeferredProviderBridge::new(kind).unwrap();
            let envelope = ProviderRouteEnvelope {
                schema_version: 1,
                run_id: "claimed".into(),
                receipt_id: "a".repeat(64),
                provider: kind,
                requested_model: ModelId::parse("claimed-model").unwrap(),
                requested_effort: EffortLevel::High,
                mode: ExecutionMode::DryRun,
                payload_sha256: "b".repeat(64),
                context_sha256: "c".repeat(64),
                authorization_digest: "d".repeat(64),
            };
            assert_eq!(bridge.prepare(&envelope), Err(BridgeError::Unverified));
        }
    }
}

#[test]
fn only_the_final_segment_can_be_a_truncated_tail() {
    let cut = TransportOutcome {
        termination: TransportTermination::TimedOut,
        exit_code: None,
        elapsed_ms: 1,
    };
    // A malformed line equal to the tail, but not the tail: a protocol error,
    // not a cut stream.
    assert!(matches!(
        CopilotAdapter.decode_stream(b"bad\nbad", cut.clone(), 8192),
        Err(AdapterError::Protocol)
    ));
    assert!(matches!(
        OpenAiResponsesAdapter.decode_stream(b"data: bad\n\ndata: bad", cut.clone(), 8192),
        Err(BridgeError::Adapter(AdapterError::Protocol))
    ));
    // The same malformed text as the real tail is still a cut stream.
    let copilot = CopilotAdapter
        .decode_stream(b"{\"type\":\"session.start\"}\nbad", cut.clone(), 8192)
        .unwrap();
    assert!(!copilot.stream_complete);
    let openai = OpenAiResponsesAdapter
        .decode_stream(
            b"data: {\"type\":\"response.created\"}\n\ndata: {\"type\":\"resp",
            cut,
            8192,
        )
        .unwrap();
    assert!(!openai.stream_complete);
}
