//! Supervised HTTP transport for a verified `PreparedProviderRequest`, limited
//! to a loopback endpoint the caller approved. Enabled by the
//! `loopback-transport` feature; nothing in the default build reaches a socket.
//!
//! The transport returns decoded `RuntimeEvidence` and nothing else. It grants
//! no workspace, runs no tool and never sees provider text: the stream decoder
//! discards it. There is no TLS, so a non-loopback endpoint cannot be approved.
mod http;
pub mod redact;

use crate::{ProviderAdapter, verify_provider_request};
pub use redact::{MAX_SECRET_BYTES, MIN_SECRET_BYTES, Redactor};
use router_adapter_api::{
    AdapterError, MAX_TRANSPORT_BYTES, ValidatedExecutionRequest, content_digest,
};
use router_types::{
    PreparedProviderRequest, RuntimeEvidence, SupplementalBridgeKind, TransportEventKind,
    TransportOutcome, TransportTermination,
};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Ceiling applied to the request timeout carried by the verified envelope.
pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// Normalized failures. Messages are fixed strings: no response bytes, header
/// values or secret ever reach them.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("transport.endpoint.refused")]
    Refused,
    #[error("transport.credential.invalid")]
    Credential,
    #[error("transport.envelope.invalid")]
    Envelope,
    #[error("transport.unauthorized")]
    Unauthorized,
    #[error("transport.rate_limited")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("transport.server_error")]
    ServerError { status: u16 },
    #[error("transport.rejected")]
    Rejected { status: u16 },
    #[error("transport.timeout")]
    Timeout,
    #[error("transport.cancelled")]
    Cancelled,
    #[error("transport.truncated")]
    Truncated,
    #[error("transport.malformed")]
    Malformed,
    #[error("transport.limit")]
    Limit,
    #[error("transport.secret_echo")]
    SecretEcho,
    #[error("transport.io")]
    Io,
    /// The provider ended the stream with a failure event (`response.failed`,
    /// `response.incomplete`, Anthropic `error`).
    #[error("transport.provider_failed")]
    ProviderFailed,
}

/// An endpoint the supervising runtime approved for one provider. The signed
/// envelope binds the provider host label, not a socket address, so this value
/// must come from the runtime's own trust store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovedEndpoint {
    provider: SupplementalBridgeKind,
    addr: SocketAddr,
}

impl ApprovedEndpoint {
    /// Loopback IP and a non-zero port only. An IPv6 scope id is refused: the
    /// `Host` header would carry a raw `%`, and loopback needs no scope.
    pub fn new(provider: SupplementalBridgeKind, addr: SocketAddr) -> Result<Self, TransportError> {
        if !addr.ip().is_loopback() || addr.port() == 0 {
            return Err(TransportError::Refused);
        }
        if matches!(addr, SocketAddr::V6(v6) if v6.scope_id() != 0) {
            return Err(TransportError::Refused);
        }
        Ok(Self { provider, addr })
    }

    /// Accepts `ip:port` or `[ip]:port` only. A host name never parses.
    pub fn parse(provider: SupplementalBridgeKind, text: &str) -> Result<Self, TransportError> {
        Self::new(
            provider,
            SocketAddr::from_str(text).map_err(|_| TransportError::Refused)?,
        )
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// A credential value. Its `Debug` output is fixed, it has no `Display`, no
/// `Serialize` and no accessor outside this module. It is not zeroized on drop.
pub struct Secret(String);

impl Secret {
    /// Visible ASCII only (usable in one header value), `MIN_SECRET_BYTES` to
    /// `MAX_SECRET_BYTES` long.
    pub fn new(value: String) -> Result<Self, TransportError> {
        if value.len() < MIN_SECRET_BYTES
            || value.len() > MAX_SECRET_BYTES
            || !value.bytes().all(|b| (33..=126).contains(&b))
        {
            return Err(TransportError::Credential);
        }
        Ok(Self(value))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

/// Cooperative cancellation, checked before every socket call.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub struct ProviderTransport {
    endpoint: ApprovedEndpoint,
    credential: Secret,
    /// Built once. `None` only if the credential patterns cannot be built, which
    /// `execute` reports as `Credential`.
    redactor: Option<Redactor>,
}

impl ProviderTransport {
    pub fn new(endpoint: ApprovedEndpoint, credential: Secret) -> Self {
        let redactor = Redactor::new(&[credential.0.as_str()]).ok();
        Self {
            endpoint,
            credential,
            redactor,
        }
    }

    /// Verifies the envelope, sends the exact verified body and decodes the
    /// stream. Every failure is closed: no partial evidence is returned.
    pub fn execute(
        &self,
        adapter: &dyn ProviderAdapter,
        prepared: &PreparedProviderRequest,
        expected_key: &[u8; 32],
        validated: &ValidatedExecutionRequest,
        run_id: &str,
        cancel: &CancelToken,
    ) -> Result<RuntimeEvidence, TransportError> {
        let started = Instant::now();
        let provider = prepared.signed_route.envelope.provider;
        if adapter.descriptor().kind != provider {
            return Err(TransportError::Envelope);
        }
        if self.endpoint.provider != provider {
            return Err(TransportError::Refused);
        }
        let limit = prepared.max_output_bytes.min(MAX_TRANSPORT_BYTES);
        if limit == 0 {
            return Err(TransportError::Limit);
        }
        let deadline = started + Duration::from_millis(prepared.timeout_ms.min(MAX_TIMEOUT_MS));
        let redactor = self.redactor.as_ref().ok_or(TransportError::Credential)?;
        let (wire, body_at) = self.wire(prepared)?;
        // Revalidation sits directly before the connection. The wire bytes come
        // from the same immutable `prepared`, and their body part is hashed
        // against the signed digest so the sent octets are the verified ones.
        verify_provider_request(prepared, expected_key, validated, run_id)
            .map_err(|_| TransportError::Envelope)?;
        if content_digest(&wire[body_at..]) != prepared.signed_route.envelope.payload_sha256 {
            return Err(TransportError::Envelope);
        }
        let mut session = http::Session::connect(self.endpoint.addr, deadline, cancel)?;
        let written = session.write_all(&wire);
        drop(wire);
        if let Err(TransportError::Io) = written {
            // A server that answers early and closes makes the write fail. The
            // status it sent is still the better answer.
            if let Ok((head, _)) = session.read_head() {
                if head.status != 200 {
                    return Err(status_error(&head));
                }
            }
        }
        written?;
        let (head, pending) = session.read_head()?;
        if head.status != 200 {
            return Err(status_error(&head));
        }
        let framing = head.success_framing(limit)?;
        let body = session.read_body(framing, pending, limit)?;
        if body.len() > limit {
            return Err(TransportError::Limit);
        }
        let check = || {
            if cancel.is_cancelled() {
                Err(TransportError::Cancelled)
            } else if Instant::now() >= deadline {
                Err(TransportError::Timeout)
            } else {
                Ok(())
            }
        };
        if redactor.scan_sse(&body, &check)? {
            return Err(TransportError::SecretEcho);
        }
        if !crate::stream::ends_at_frame_boundary(&body) {
            return Err(TransportError::Truncated);
        }
        let outcome = TransportOutcome {
            termination: TransportTermination::Exited,
            exit_code: Some(0),
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        let evidence =
            adapter
                .decode_stream(&body, outcome, limit)
                .map_err(|error| match error {
                    crate::BridgeError::Adapter(AdapterError::OutputLimit) => TransportError::Limit,
                    _ => TransportError::Malformed,
                })?;
        if !evidence.stream_complete {
            return Err(TransportError::Truncated);
        }
        if evidence
            .events
            .iter()
            .any(|event| event.kind == TransportEventKind::HostError)
        {
            return Err(TransportError::ProviderFailed);
        }
        let printed = serde_json::to_vec(&evidence).map_err(|_| TransportError::Malformed)?;
        check()?;
        if redactor.leaks(&printed) {
            return Err(TransportError::SecretEcho);
        }
        Ok(evidence)
    }

    /// Request bytes and the offset of the body inside them.
    fn wire(&self, prepared: &PreparedProviderRequest) -> Result<(Vec<u8>, usize), TransportError> {
        let unsafe_text = |text: &str| text.bytes().any(|b| b < 0x20 || b == 0x7f);
        // Defense in depth. `verify_provider_request` pins method and path to
        // fixed values and runs right after this function, so a failure here and
        // a failure there are the same `Envelope` error: this guard cannot be
        // told apart from the verification by an input, it only keeps the
        // request line safe if the verification is ever relaxed.
        if [&prepared.method, &prepared.path]
            .iter()
            .any(|part| part.is_empty() || unsafe_text(part) || part.contains(' '))
        {
            return Err(TransportError::Envelope);
        }
        let mut head = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\n",
            prepared.method, prepared.path, self.endpoint.addr
        );
        let mut add = |name: &str, value: &str| {
            if name.is_empty() || unsafe_text(name) || unsafe_text(value) || name.contains(':') {
                return Err(TransportError::Envelope);
            }
            head.push_str(&format!("{name}: {value}\r\n"));
            Ok(())
        };
        for (name, value) in &prepared.static_headers {
            add(name, value)?;
        }
        let credential = if prepared
            .credential_header_name
            .eq_ignore_ascii_case("authorization")
        {
            format!("Bearer {}", self.credential.0)
        } else {
            self.credential.0.clone()
        };
        add(&prepared.credential_header_name, &credential)?;
        add("Accept", "text/event-stream")?;
        add("Accept-Encoding", "identity")?;
        add(
            "Content-Length",
            &prepared.body.as_bytes().len().to_string(),
        )?;
        add("Connection", "close")?;
        head.push_str("\r\n");
        let body_at = head.len();
        let mut wire = head.into_bytes();
        wire.extend_from_slice(prepared.body.as_bytes());
        Ok((wire, body_at))
    }
}

/// Maps a non-200 status to its normalized error.
fn status_error(head: &http::Head) -> TransportError {
    match head.status {
        401 | 403 => TransportError::Unauthorized,
        429 => TransportError::RateLimited {
            retry_after_secs: head.retry_after(),
        },
        status @ 500..=599 => TransportError::ServerError { status },
        status @ 400..=499 => TransportError::Rejected { status },
        _ => TransportError::Malformed,
    }
}
