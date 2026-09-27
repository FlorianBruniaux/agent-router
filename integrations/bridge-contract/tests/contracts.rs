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
