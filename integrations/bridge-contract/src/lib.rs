//! Native host preparation and provider protocol translation. Runtime support
//! remains evidence-driven; these libraries do not activate or contact a host.
#![forbid(unsafe_code)]

mod harness;
pub mod omnigent;
mod provider;
mod stream;
#[cfg(feature = "loopback-transport")]
pub mod transport;
pub use harness::{CopilotAdapter, OpenCodeAdapter};
pub use provider::{
    AnthropicMessagesAdapter, EnvelopeAuthority, OpenAiResponsesAdapter, ProviderAdapter,
    verify_provider_request,
};

use router_adapter_api::{
    ALL_CAPABILITIES, AdapterError, HarnessAdapter, ValidatedExecutionRequest,
};
use router_types::{
    BridgeDescriptor, BridgeLayer, CapabilitySupport, PreparedRun, ProviderRouteEnvelope,
    RuntimeEvidence, SupplementalBridgeKind, TransportOutcome,
};

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum BridgeError {
    #[error("bridge.layer.invalid")]
    Layer,
    #[error("bridge.provider.native_contract_and_authentication_unknown")]
    Unverified,
    #[error("bridge.input.invalid")]
    Invalid,
    #[error("bridge.signature.invalid")]
    Signature,
    #[error("bridge.capability.unsupported")]
    Unsupported,
    #[error(transparent)]
    Adapter(#[from] AdapterError),
}

pub fn descriptor(kind: SupplementalBridgeKind) -> BridgeDescriptor {
    let layer = match kind {
        SupplementalBridgeKind::GitHubCopilotCli | SupplementalBridgeKind::OpenCode => {
            BridgeLayer::Harness
        }
        _ => BridgeLayer::Provider,
    };
    BridgeDescriptor {
        schema_version: 1,
        kind,
        layer,
        capabilities: ALL_CAPABILITIES
            .into_iter()
            .map(|capability| (capability, CapabilitySupport::Unknown))
            .collect(),
        transport: CapabilitySupport::Unknown,
        authentication: CapabilitySupport::Unknown,
        parent_model_change: CapabilitySupport::Unknown,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeferredHarnessBridge {
    kind: SupplementalBridgeKind,
}
impl DeferredHarnessBridge {
    pub fn new(kind: SupplementalBridgeKind) -> Result<Self, BridgeError> {
        if descriptor(kind).layer != BridgeLayer::Harness {
            return Err(BridgeError::Layer);
        }
        Ok(Self { kind })
    }
    pub fn descriptor(&self) -> BridgeDescriptor {
        descriptor(self.kind)
    }
}
impl HarnessAdapter for DeferredHarnessBridge {
    fn host(&self) -> &'static str {
        match self.kind {
            SupplementalBridgeKind::GitHubCopilotCli => "copilot",
            SupplementalBridgeKind::OpenCode => "opencode",
            _ => "unsupported",
        }
    }
    fn prepare(&self, _: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        Err(AdapterError::Unsupported("native-contract-unverified"))
    }
    fn decode_stream(
        &self,
        _: &[u8],
        _: TransportOutcome,
        _: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        Err(AdapterError::Unsupported("native-protocol-unverified"))
    }
}

/// Provider protocols transport messages; they do not own workspaces, native
/// agents or a tool loop. A future transport must authenticate a runtime-bound
/// envelope, normalize provider events and keep context outside its payload.
pub trait UnverifiedProviderAdapter {
    fn descriptor(&self) -> BridgeDescriptor;
    fn prepare(&self, envelope: &ProviderRouteEnvelope) -> Result<(), BridgeError>;
}

#[derive(Clone, Copy, Debug)]
pub struct DeferredProviderBridge {
    kind: SupplementalBridgeKind,
}
impl DeferredProviderBridge {
    pub fn new(kind: SupplementalBridgeKind) -> Result<Self, BridgeError> {
        if descriptor(kind).layer != BridgeLayer::Provider {
            return Err(BridgeError::Layer);
        }
        Ok(Self { kind })
    }
}
impl UnverifiedProviderAdapter for DeferredProviderBridge {
    fn descriptor(&self) -> BridgeDescriptor {
        descriptor(self.kind)
    }
    fn prepare(&self, _: &ProviderRouteEnvelope) -> Result<(), BridgeError> {
        Err(BridgeError::Unverified)
    }
}
