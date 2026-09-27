use crate::{BridgeError, descriptor};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use router_adapter_api::{
    ValidatedExecutionRequest, content_digest, prepared_run, safe_identifier,
};
use router_types::*;
use std::collections::BTreeMap;

/// The parent runtime holds this key; it is never passed to a child or provider.
/// Possessing a key does not replace receipt, budget, endpoint or user approval.
pub struct EnvelopeAuthority(SigningKey);
impl EnvelopeAuthority {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }
    pub fn verification_key(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }
}
impl std::fmt::Debug for EnvelopeAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EnvelopeAuthority([REDACTED])")
    }
}

pub trait ProviderAdapter {
    fn descriptor(&self) -> BridgeDescriptor;
    fn prepare(
        &self,
        request: &ValidatedExecutionRequest,
        authority: &EnvelopeAuthority,
        run_id: &str,
        max_output_tokens: u32,
    ) -> Result<PreparedProviderRequest, BridgeError>;
    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, BridgeError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OpenAiResponsesAdapter;
#[derive(Clone, Copy, Debug, Default)]
pub struct AnthropicMessagesAdapter;

impl ProviderAdapter for OpenAiResponsesAdapter {
    fn descriptor(&self) -> BridgeDescriptor {
        descriptor(SupplementalBridgeKind::OpenAiCompatibleApi)
    }
    fn prepare(
        &self,
        r: &ValidatedExecutionRequest,
        a: &EnvelopeAuthority,
        id: &str,
        t: u32,
    ) -> Result<PreparedProviderRequest, BridgeError> {
        prepare(r, a, id, t, self.descriptor().kind)
    }
    fn decode_stream(
        &self,
        b: &[u8],
        o: TransportOutcome,
        m: usize,
    ) -> Result<RuntimeEvidence, BridgeError> {
        super::stream::decode_sse(b, o, m, true).map_err(BridgeError::from)
    }
}
impl ProviderAdapter for AnthropicMessagesAdapter {
    fn descriptor(&self) -> BridgeDescriptor {
        descriptor(SupplementalBridgeKind::AnthropicCompatibleApi)
    }
    fn prepare(
        &self,
        r: &ValidatedExecutionRequest,
        a: &EnvelopeAuthority,
        id: &str,
        t: u32,
    ) -> Result<PreparedProviderRequest, BridgeError> {
        prepare(r, a, id, t, self.descriptor().kind)
    }
    fn decode_stream(
        &self,
        b: &[u8],
        o: TransportOutcome,
        m: usize,
    ) -> Result<RuntimeEvidence, BridgeError> {
        super::stream::decode_sse(b, o, m, false).map_err(BridgeError::from)
    }
}

fn prepare(
    validated: &ValidatedExecutionRequest,
    authority: &EnvelopeAuthority,
    run_id: &str,
    max_output_tokens: u32,
    provider: SupplementalBridgeKind,
) -> Result<PreparedProviderRequest, BridgeError> {
    let request = validated.request();
    let openai = provider == SupplementalBridgeKind::OpenAiCompatibleApi;
    if request.host.as_str()
        != if openai {
            "openai-api"
        } else {
            "anthropic-api"
        }
        || !safe_identifier(run_id)
        || max_output_tokens == 0
        || max_output_tokens > 32_768
    {
        return Err(BridgeError::Invalid);
    }
    // Provider transport does not supply a workspace, host agent or tool loop.
    if request.sandbox != ExecutionSandbox::ReadOnly
        || request.target != ExecutionTarget::ExternalWorker
        || request.agent.is_some()
        || !matches!(request.tools, ToolSelection::Disabled)
    {
        return Err(BridgeError::Unsupported);
    }
    if !openai
        && !matches!(
            request.effort,
            EffortLevel::Low
                | EffortLevel::Medium
                | EffortLevel::High
                | EffortLevel::Xhigh
                | EffortLevel::Max
        )
    {
        return Err(BridgeError::Unsupported);
    }
    if openai
        && !matches!(
            request.effort,
            EffortLevel::None
                | EffortLevel::Minimal
                | EffortLevel::Low
                | EffortLevel::Medium
                | EffortLevel::High
                | EffortLevel::Xhigh
        )
    {
        return Err(BridgeError::Unsupported);
    }
    let input = prepared_run(validated, "provider-payload", vec![], vec![])?;
    let prompt = std::str::from_utf8(input.stdin.as_bytes()).map_err(|_| BridgeError::Invalid)?;
    let body = if openai {
        serde_json::json!({"model":request.model,"input":prompt,"reasoning":{"effort":request.effort},"max_output_tokens":max_output_tokens,"stream":true,"store":false})
    } else {
        serde_json::json!({"model":request.model,"messages":[{"role":"user","content":prompt}],"output_config":{"effort":request.effort},"max_tokens":max_output_tokens,"stream":true})
    };
    let body = serde_json::to_vec(&body).map_err(|_| BridgeError::Invalid)?;
    let context = serde_json::to_vec(request).map_err(|_| BridgeError::Invalid)?;
    let envelope = ProviderRouteEnvelope {
        schema_version: 1,
        run_id: run_id.into(),
        receipt_id: request.receipt_id.clone(),
        provider,
        requested_model: request.model.clone(),
        requested_effort: request.effort,
        mode: request.mode,
        payload_sha256: content_digest(&body),
        context_sha256: content_digest(&context),
        authorization_digest: validated.authorization_digest().into(),
    };
    let signature = authority.0.sign(&signing_bytes(&envelope)?);
    let mut headers = BTreeMap::from([("content-type".into(), "application/json".into())]);
    if !openai {
        headers.insert("anthropic-version".into(), "2023-06-01".into());
    }
    Ok(PreparedProviderRequest {
        signed_route: SignedProviderRouteEnvelope {
            envelope,
            signature_hex: hex::encode(signature.to_bytes()),
        },
        method: "POST".into(),
        path: if openai {
            "/v1/responses"
        } else {
            "/v1/messages"
        }
        .into(),
        static_headers: headers,
        credential_header_name: if openai { "Authorization" } else { "x-api-key" }.into(),
        body: PreparedInput::new(body),
        timeout_ms: request.timeout_ms,
        max_output_bytes: request.max_output_bytes,
    })
}

/// The expected key and execution request must come from the supervising
/// runtime's trust store, not fields supplied by the provider or child.
pub fn verify_provider_request(
    prepared: &PreparedProviderRequest,
    expected_key: &[u8; 32],
    validated: &ValidatedExecutionRequest,
    expected_run_id: &str,
) -> Result<(), BridgeError> {
    let envelope = &prepared.signed_route.envelope;
    let request = validated.request();
    let context = serde_json::to_vec(request).map_err(|_| BridgeError::Invalid)?;
    if envelope.schema_version != 1
        || envelope.run_id != expected_run_id
        || envelope.receipt_id != request.receipt_id
        || envelope.requested_model != request.model
        || envelope.requested_effort != request.effort
        || envelope.mode != request.mode
        || envelope.authorization_digest != validated.authorization_digest()
        || envelope.context_sha256 != content_digest(&context)
        || envelope.payload_sha256 != content_digest(prepared.body.as_bytes())
        || prepared.timeout_ms != request.timeout_ms
        || prepared.max_output_bytes != request.max_output_bytes
    {
        return Err(BridgeError::Signature);
    }
    let expected_host = match envelope.provider {
        SupplementalBridgeKind::OpenAiCompatibleApi => "openai-api",
        SupplementalBridgeKind::AnthropicCompatibleApi => "anthropic-api",
        _ => return Err(BridgeError::Signature),
    };
    if request.host.as_str() != expected_host {
        return Err(BridgeError::Signature);
    }
    let key = VerifyingKey::from_bytes(expected_key).map_err(|_| BridgeError::Signature)?;
    let bytes =
        hex::decode(&prepared.signed_route.signature_hex).map_err(|_| BridgeError::Signature)?;
    let signature = Signature::from_slice(&bytes).map_err(|_| BridgeError::Signature)?;
    key.verify_strict(&signing_bytes(envelope)?, &signature)
        .map_err(|_| BridgeError::Signature)?;
    // Routing metadata cannot be used to inject an endpoint or secret header.
    let openai = envelope.provider == SupplementalBridgeKind::OpenAiCompatibleApi;
    let mut headers = BTreeMap::from([("content-type".into(), "application/json".into())]);
    if !openai {
        headers.insert("anthropic-version".into(), "2023-06-01".into());
    }
    if prepared.method != "POST"
        || prepared.path
            != if openai {
                "/v1/responses"
            } else {
                "/v1/messages"
            }
        || prepared.credential_header_name != if openai { "Authorization" } else { "x-api-key" }
        || prepared.static_headers != headers
    {
        return Err(BridgeError::Signature);
    }
    Ok(())
}

fn signing_bytes(envelope: &ProviderRouteEnvelope) -> Result<Vec<u8>, BridgeError> {
    let mut bytes = b"agent-router:provider-route:v1\0".to_vec();
    bytes.extend(serde_json::to_vec(envelope).map_err(|_| BridgeError::Invalid)?);
    Ok(bytes)
}
