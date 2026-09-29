#![cfg(feature = "loopback-transport")]
//! Loopback transport tests. Each fake server carries one defect.
use router_adapter_api::*;
use router_bridge_contract::transport::*;
use router_bridge_contract::*;
use router_types::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SECRET: &str = "sk-te\"st\\vAlue-9x";

fn validated(host: &str, timeout_ms: u64, max_output_bytes: usize) -> ValidatedExecutionRequest {
    let model = ModelId::parse("exact-test-model").unwrap();
    let observation = HostObservation {
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
    let snapshot = ValidatedHostObservation::from_observation(
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
        tools: ToolSelection::Disabled,
        agent: None,
        skills: vec![],
        prompt: "PRIVATE_PROMPT".into(),
        mode: ExecutionMode::DryRun,
        target: ExecutionTarget::ExternalWorker,
        allow_degradation: false,
        timeout_ms,
        max_output_bytes,
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

struct Setup {
    host: &'static str,
    kind: SupplementalBridgeKind,
    adapter: &'static dyn ProviderAdapter,
    validated: ValidatedExecutionRequest,
    prepared: PreparedProviderRequest,
    key: [u8; 32],
}

const HOSTS: [&str; 2] = ["openai-api", "anthropic-api"];

fn setup(host: &'static str) -> Setup {
    setup_with(host, 5_000, 8192)
}

fn setup_with(host: &'static str, timeout_ms: u64, max_output_bytes: usize) -> Setup {
    let authority = EnvelopeAuthority::from_seed(&[7; 32]);
    let (adapter, kind): (&'static dyn ProviderAdapter, _) = if host == "openai-api" {
        (
            &OpenAiResponsesAdapter,
            SupplementalBridgeKind::OpenAiCompatibleApi,
        )
    } else {
        (
            &AnthropicMessagesAdapter,
            SupplementalBridgeKind::AnthropicCompatibleApi,
        )
    };
    let validated = validated(host, timeout_ms, max_output_bytes);
    let prepared = adapter
        .prepare(&validated, &authority, "run-1", 512)
        .unwrap();
    Setup {
        host,
        kind,
        adapter,
        validated,
        prepared,
        key: authority.verification_key(),
    }
}

impl Setup {
    fn run(
        &self,
        addr: SocketAddr,
        cancel: &CancelToken,
    ) -> Result<RuntimeEvidence, TransportError> {
        self.run_prepared(&self.prepared, addr, cancel)
    }
    fn run_prepared(
        &self,
        prepared: &PreparedProviderRequest,
        addr: SocketAddr,
        cancel: &CancelToken,
    ) -> Result<RuntimeEvidence, TransportError> {
        let endpoint = ApprovedEndpoint::new(self.kind, addr)?;
        ProviderTransport::new(endpoint, Secret::new(SECRET.into()).unwrap()).execute(
            self.adapter,
            prepared,
            &self.key,
            &self.validated,
            "run-1",
            cancel,
        )
    }
}

fn event(kind: &str, extra: serde_json::Value) -> String {
    let mut value = serde_json::json!({ "type": kind });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    format!("event: {kind}\ndata: {value}\n\n")
}

fn stream_with_text(host: &str, text: &str) -> String {
    if host == "openai-api" {
        event("response.created", serde_json::json!({}))
            + &event(
                "response.output_text.delta",
                serde_json::json!({"delta": text}),
            )
            + &event(
                "response.completed",
                serde_json::json!({"response":{"status":"completed","model":"native-api-model"}}),
            )
    } else {
        event(
            "message_start",
            serde_json::json!({"message":{"type":"message","role":"assistant","model":"native-api-model"}}),
        ) + &event(
            "content_block_delta",
            serde_json::json!({"delta":{"text": text}}),
        ) + &event("message_stop", serde_json::json!({}))
    }
}

fn head(status: &str, headers: &[String]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\n");
    for header in headers {
        out.push_str(header);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    out.into_bytes()
}

fn sse_response(body: &[u8]) -> Vec<u8> {
    let mut out = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            format!("Content-Length: {}", body.len()),
        ],
    );
    out.extend_from_slice(body);
    out
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
            let length = text
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map(|v| v.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if buf.len() >= end + 4 + length {
                return buf;
            }
        }
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0, "client closed before the request was complete");
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Blocks until the client closes or 10 seconds pass, so the server never
/// ends the connection before the client acts.
fn hold(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).map(|n| n > 0).unwrap_or(false) {}
}

fn serve<F>(handler: F) -> (SocketAddr, JoinHandle<Vec<u8>>)
where
    F: FnOnce(&mut TcpStream) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let give_up = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) if Instant::now() < give_up => thread::sleep(Duration::from_millis(5)),
                Err(e) => panic!("no client connected: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let request = read_request(&mut stream);
        handler(&mut stream);
        request
    });
    (addr, handle)
}

fn respond(body: Vec<u8>) -> impl FnOnce(&mut TcpStream) + Send + 'static {
    move |s| {
        s.write_all(&body).unwrap();
    }
}

/// A listener nobody should reach.
fn silent() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, addr)
}

fn assert_no_connection(listener: &TcpListener) {
    thread::sleep(Duration::from_millis(50));
    assert!(
        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "the transport connected although it must not"
    );
}

fn no_leak(error: &TransportError) {
    let printed = format!("{error} {error:?}");
    assert!(!printed.contains(SECRET), "secret in {printed}");
    assert!(!printed.contains("PRIVATE"), "body data in {printed}");
}

#[test]
fn nominal_stream_is_decoded_and_the_verified_body_is_sent() {
    for host in HOSTS {
        let s = setup(host);
        let body = stream_with_text(host, "PRIVATE_TEXT model=forged");
        let (addr, server) = serve(respond(sse_response(body.as_bytes())));
        let evidence = s.run(addr, &CancelToken::new()).unwrap();
        let request = server.join().unwrap();
        assert!(evidence.stream_complete && evidence.transport_success);
        assert_eq!(
            evidence.executed_models[0].model.as_str(),
            "native-api-model"
        );
        assert!(
            !serde_json::to_string(&evidence)
                .unwrap()
                .contains("PRIVATE_TEXT")
        );
        let text = String::from_utf8_lossy(&request).into_owned();
        let (path, credential) = if host == "openai-api" {
            (
                "POST /v1/responses HTTP/1.1\r\n",
                format!("Authorization: Bearer {SECRET}\r\n"),
            )
        } else {
            (
                "POST /v1/messages HTTP/1.1\r\n",
                format!("x-api-key: {SECRET}\r\n"),
            )
        };
        assert!(text.starts_with(path), "{text}");
        assert!(text.contains(&credential));
        assert!(text.contains("Connection: close\r\n"));
        assert!(request.ends_with(s.prepared.body.as_bytes()));
        assert!(!s.host.is_empty());
    }
}

#[test]
fn chunked_stream_is_decoded_and_a_cut_chunk_is_truncated() {
    let s = setup("openai-api");
    let body = stream_with_text("openai-api", "hello");
    let (a, b) = body.as_bytes().split_at(40);
    let mut wire = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            "Transfer-Encoding: chunked".into(),
        ],
    );
    for part in [a, b] {
        wire.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
        wire.extend_from_slice(part);
        wire.extend_from_slice(b"\r\n");
    }
    wire.extend_from_slice(b"0\r\n\r\n");
    let (addr, server) = serve(respond(wire.clone()));
    assert!(s.run(addr, &CancelToken::new()).unwrap().stream_complete);
    server.join().unwrap();

    wire.truncate(wire.len() - 30);
    let (addr, server) = serve(respond(wire));
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Truncated)
    );
    server.join().unwrap();
}

#[test]
fn http_errors_are_normalized_without_response_data() {
    let cases: Vec<(&str, Vec<String>, TransportError)> = vec![
        ("401 Unauthorized", vec![], TransportError::Unauthorized),
        ("403 Forbidden", vec![], TransportError::Unauthorized),
        (
            "429 Too Many Requests",
            vec!["Retry-After: 7".into()],
            TransportError::RateLimited {
                retry_after_secs: Some(7),
            },
        ),
        (
            "429 Too Many Requests",
            vec![],
            TransportError::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            "429 Too Many Requests",
            vec!["Retry-After: soon".into()],
            TransportError::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            "429 Too Many Requests",
            vec!["Retry-After: Wed, 21 Oct 2015 07:28:00 GMT".into()],
            TransportError::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            "429 Too Many Requests",
            vec!["Retry-After: -5".into()],
            TransportError::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            "429 Too Many Requests",
            vec!["Retry-After: 99999999999999999999999".into()],
            TransportError::RateLimited {
                retry_after_secs: Some(3600),
            },
        ),
        (
            "429 Too Many Requests",
            vec!["Retry-After: 5".into(), "Retry-After: 6".into()],
            TransportError::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            "500 Internal Server Error",
            vec![],
            TransportError::ServerError { status: 500 },
        ),
        (
            "503 Service Unavailable",
            vec![],
            TransportError::ServerError { status: 503 },
        ),
        (
            "404 Not Found",
            vec![],
            TransportError::Rejected { status: 404 },
        ),
        ("302 Found", vec![], TransportError::Malformed),
    ];
    for host in HOSTS {
        let s = setup(host);
        for (status, mut headers, expected) in cases.clone() {
            let echo = format!("PRIVATE echo {SECRET}");
            headers.push(format!("Content-Length: {}", echo.len()));
            let mut wire = head(status, &headers);
            wire.extend_from_slice(echo.as_bytes());
            let (addr, server) = serve(respond(wire));
            let got = s.run(addr, &CancelToken::new());
            server.join().unwrap();
            assert_eq!(got.as_ref().err(), Some(&expected), "{status} {headers:?}");
            no_leak(&expected);
        }
    }
}

#[test]
fn a_body_shorter_than_content_length_is_truncated() {
    let s = setup("openai-api");
    let body = stream_with_text("openai-api", "x");
    let mut wire = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            format!("Content-Length: {}", body.len() + 100),
        ],
    );
    wire.extend_from_slice(body.as_bytes());
    let (addr, server) = serve(respond(wire));
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Truncated)
    );
    server.join().unwrap();
}

#[test]
fn a_stream_cut_without_a_terminal_event_is_truncated() {
    for host in HOSTS {
        let s = setup(host);
        let full = stream_with_text(host, "x");
        // Complete frames but no terminal event.
        let last = full.trim_end().rfind("\n\n").unwrap() + 2;
        let no_terminal = full[..last].to_string();
        // Cut in the middle of the last frame.
        let mid = full[..full.len() - 10].to_string();
        for body in [no_terminal, mid, String::new()] {
            let (addr, server) = serve(respond(sse_response(body.as_bytes())));
            assert_eq!(
                s.run(addr, &CancelToken::new()),
                Err(TransportError::Truncated),
                "{body:?}"
            );
            server.join().unwrap();
        }
    }
}

#[test]
fn forged_or_malformed_events_and_heads_are_malformed() {
    let s = setup("openai-api");
    let bodies = [
        // Declared type differs from the JSON type.
        "event: response.created\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n".to_string(),
        // Not JSON.
        "data: {not json\n\n".to_string(),
        // Unknown SSE field.
        "forged: 1\n\n".to_string(),
        // Duplicate event line.
        "event: a\nevent: b\ndata: {\"type\":\"a\"}\n\n".to_string(),
    ];
    for body in bodies {
        let (addr, server) = serve(respond(sse_response(body.as_bytes())));
        assert_eq!(
            s.run(addr, &CancelToken::new()),
            Err(TransportError::Malformed),
            "{body}"
        );
        server.join().unwrap();
    }
    let ok = stream_with_text("openai-api", "x");
    let heads: Vec<Vec<String>> = vec![
        vec![format!("Content-Length: {}", ok.len())],
        vec![
            "Content-Type: application/json".into(),
            format!("Content-Length: {}", ok.len()),
        ],
        vec![
            "Content-Type: text/event-stream".into(),
            "Content-Encoding: gzip".into(),
            format!("Content-Length: {}", ok.len()),
        ],
        vec![
            "Content-Type: text/event-stream".into(),
            "Transfer-Encoding: chunked".into(),
            format!("Content-Length: {}", ok.len()),
        ],
        vec![
            "Content-Type: text/event-stream".into(),
            format!("Content-Length: {}", ok.len()),
            format!("Content-Length: {}", ok.len()),
        ],
    ];
    for headers in heads {
        let mut wire = head("200 OK", &headers);
        wire.extend_from_slice(ok.as_bytes());
        let (addr, server) = serve(respond(wire));
        assert_eq!(
            s.run(addr, &CancelToken::new()),
            Err(TransportError::Malformed),
            "{headers:?}"
        );
        server.join().unwrap();
    }
    // Both framings announced over a body that is valid chunked data: only the
    // conflict makes it malformed.
    let mut wire = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            "Transfer-Encoding: chunked".into(),
            format!("Content-Length: {}", ok.len() + 12),
        ],
    );
    wire.extend_from_slice(format!("{:x}\r\n{ok}\r\n0\r\n\r\n", ok.len()).as_bytes());
    let (addr, server) = serve(respond(wire));
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Malformed)
    );
    server.join().unwrap();
    let (addr, server) = serve(respond(b"garbage\r\n\r\n".to_vec()));
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Malformed)
    );
    server.join().unwrap();
}

#[test]
fn oversized_responses_and_headers_hit_the_limits() {
    let s = setup_with("openai-api", 5_000, 1024);
    let big = "x".repeat(4096);
    // Declared length above the limit: refused before the body is read.
    let (addr, server) = serve(respond(head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            format!("Content-Length: {}", big.len()),
        ],
    )));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    // No length, body streams past the limit until close.
    let mut wire = head("200 OK", &["Content-Type: text/event-stream".into()]);
    wire.extend_from_slice(big.as_bytes());
    let (addr, server) = serve(respond(wire));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    // Chunked body past the limit.
    let mut wire = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            "Transfer-Encoding: chunked".into(),
        ],
    );
    wire.extend_from_slice(format!("{:x}\r\n{big}\r\n0\r\n\r\n", big.len()).as_bytes());
    let (addr, server) = serve(respond(wire));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    // Headers above 16 KiB, and a head that never ends.
    let mut wire = head("200 OK", &[format!("X-Pad: {}", "a".repeat(20 * 1024))]);
    wire.extend_from_slice(b"body");
    let (addr, server) = serve(respond(wire));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    let (addr, server) = serve(|s| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\n");
        let _ = s.write_all(&vec![b'a'; 20 * 1024]);
    });
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    // A head whose terminator arrives after 16 KiB.
    let mut wire = head("200 OK", &[format!("X-Pad: {}", "a".repeat(17_000))]);
    wire.extend_from_slice(b"body");
    let (addr, server) = serve(respond(wire));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
    // Endless bodies must stop at the limit, not at the deadline.
    for chunked in [false, true] {
        let s = setup_with("openai-api", 20_000, 1024);
        let (addr, server) = serve(move |st| {
            let framing = if chunked {
                "Transfer-Encoding: chunked"
            } else {
                "Connection: close"
            };
            let _ = st.write_all(&head(
                "200 OK",
                &["Content-Type: text/event-stream".into(), framing.into()],
            ));
            let piece = "x".repeat(256);
            for _ in 0..200 {
                let out = if chunked {
                    format!("100\r\n{piece}\r\n")
                } else {
                    piece.clone()
                };
                if st.write_all(out.as_bytes()).is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let started = Instant::now();
        assert_eq!(
            s.run(addr, &CancelToken::new()),
            Err(TransportError::Limit),
            "chunked={chunked}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        server.join().unwrap();
    }
    // Too many events for the decoder bound, inside the byte limit.
    let s = setup_with("openai-api", 5_000, 4 * 1024 * 1024);
    let flood = "data: {\"type\":\"x\"}\n\n".repeat(MAX_TRANSPORT_EVENTS + 1);
    let (addr, server) = serve(respond(sse_response(flood.as_bytes())));
    assert_eq!(s.run(addr, &CancelToken::new()), Err(TransportError::Limit));
    server.join().unwrap();
}

#[test]
fn a_server_that_never_answers_times_out() {
    let s = setup_with("openai-api", 300, 8192);
    let (addr, server) = serve(hold);
    let started = Instant::now();
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Timeout)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    server.join().unwrap();
}

#[test]
fn a_stalled_stream_times_out_and_cancellation_stops_a_read() {
    let s = setup_with("openai-api", 300, 8192);
    let (addr, server) = serve(|st| {
        st.write_all(&head(
            "200 OK",
            &[
                "Content-Type: text/event-stream".into(),
                "Content-Length: 5000".into(),
            ],
        ))
        .unwrap();
        hold(st);
    });
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Timeout)
    );
    server.join().unwrap();

    let s = setup_with("openai-api", 30_000, 8192);
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let (addr, server) = serve(move |st| {
        st.write_all(&head(
            "200 OK",
            &[
                "Content-Type: text/event-stream".into(),
                "Content-Length: 5000".into(),
            ],
        ))
        .unwrap();
        st.write_all(b"data: {\"type\":\"response.created\"}\n\n")
            .unwrap();
        tx.send(()).unwrap();
        hold(st);
    });
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let canceller = thread::spawn(move || {
        rx.recv().unwrap();
        thread::sleep(Duration::from_millis(50));
        trigger.cancel();
    });
    let started = Instant::now();
    assert_eq!(s.run(addr, &cancel), Err(TransportError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancel was not prompt"
    );
    canceller.join().unwrap();
    server.join().unwrap();
}

#[test]
fn a_cancelled_token_prevents_any_connection() {
    let s = setup("openai-api");
    let (listener, addr) = silent();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert_eq!(s.run(addr, &cancel), Err(TransportError::Cancelled));
    assert_no_connection(&listener);
}

#[test]
fn only_loopback_ip_endpoints_with_a_port_are_approved() {
    let kind = SupplementalBridgeKind::OpenAiCompatibleApi;
    for text in [
        "192.0.2.1:443",
        "8.8.8.8:80",
        "0.0.0.0:80",
        "[::]:80",
        "[2001:db8::1]:80",
        "[::ffff:127.0.0.1]:80",
        "127.0.0.1:0",
        "[::1]:0",
        "localhost:80",
        "example.com:80",
        "127.0.0.1",
        "",
    ] {
        assert_eq!(
            ApprovedEndpoint::parse(kind, text),
            Err(TransportError::Refused),
            "{text}"
        );
    }
    assert!(ApprovedEndpoint::parse(kind, "127.0.0.1:8080").is_ok());
    assert!(ApprovedEndpoint::parse(kind, "127.0.0.2:8080").is_ok());
    assert!(ApprovedEndpoint::parse(kind, "[::1]:8080").is_ok());
}

#[test]
fn an_endpoint_approved_for_another_provider_is_refused_without_connecting() {
    let s = setup("openai-api");
    let (listener, addr) = silent();
    let other =
        ApprovedEndpoint::new(SupplementalBridgeKind::AnthropicCompatibleApi, addr).unwrap();
    let transport = ProviderTransport::new(other, Secret::new(SECRET.into()).unwrap());
    let got = transport.execute(
        s.adapter,
        &s.prepared,
        &s.key,
        &s.validated,
        "run-1",
        &CancelToken::new(),
    );
    assert_eq!(got, Err(TransportError::Refused));
    // An adapter for the other provider than the signed envelope is refused too.
    let wrong = ProviderTransport::new(
        ApprovedEndpoint::new(SupplementalBridgeKind::OpenAiCompatibleApi, addr).unwrap(),
        Secret::new(SECRET.into()).unwrap(),
    );
    let got = wrong.execute(
        &AnthropicMessagesAdapter,
        &s.prepared,
        &s.key,
        &s.validated,
        "run-1",
        &CancelToken::new(),
    );
    assert_eq!(got, Err(TransportError::Envelope));
    assert_no_connection(&listener);
}

#[test]
fn an_envelope_changed_after_signing_is_refused_before_any_connection() {
    for host in HOSTS {
        let s = setup(host);
        let (listener, addr) = silent();
        let mut body = s.prepared.clone();
        body.body = PreparedInput::new(b"{\"model\":\"other\"}".to_vec());
        let mut path = s.prepared.clone();
        path.path = "/injected".into();
        let mut crlf_path = s.prepared.clone();
        crlf_path.path = format!("{}\r\nX-Injected: 1", s.prepared.path);
        let mut crlf_method = s.prepared.clone();
        crlf_method.method = "POST\r\nX-Injected: 1".into();
        let mut header = s.prepared.clone();
        header.static_headers.insert("x-extra".into(), "1".into());
        let mut credential_name = s.prepared.clone();
        credential_name.credential_header_name = "x-forward".into();
        let mut mode = s.prepared.clone();
        mode.signed_route.envelope.mode = ExecutionMode::Execute;
        let mut signature = s.prepared.clone();
        signature.signed_route.signature_hex = "00".repeat(64);
        let mut timeout = s.prepared.clone();
        timeout.timeout_ms += 1;
        for tampered in [
            body,
            path,
            crlf_path,
            crlf_method,
            header,
            credential_name,
            mode,
            signature,
            timeout,
        ] {
            assert_eq!(
                s.run_prepared(&tampered, addr, &CancelToken::new()),
                Err(TransportError::Envelope)
            );
        }
        // Wrong trust key.
        let endpoint = ApprovedEndpoint::new(s.kind, addr).unwrap();
        let transport = ProviderTransport::new(endpoint, Secret::new(SECRET.into()).unwrap());
        assert_eq!(
            transport.execute(
                s.adapter,
                &s.prepared,
                &[9; 32],
                &s.validated,
                "run-1",
                &CancelToken::new()
            ),
            Err(TransportError::Envelope)
        );
        // Wrong run id.
        assert_eq!(
            transport.execute(
                s.adapter,
                &s.prepared,
                &s.key,
                &s.validated,
                "run-2",
                &CancelToken::new()
            ),
            Err(TransportError::Envelope)
        );
        assert_no_connection(&listener);
    }
}

fn b64(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(alphabet[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn pct(value: &str, all: bool) -> String {
    value
        .bytes()
        .map(|b| {
            if !all && (b.is_ascii_alphanumeric() || b"-_.~".contains(&b)) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn join(parts: impl Iterator<Item = String>) -> String {
    let mut out = String::new();
    for part in parts {
        out.push_str(&part);
    }
    out
}

fn mixed_case(text: &str) -> String {
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

fn wrap(text: &str, width: usize) -> String {
    text.as_bytes()
        .chunks(width)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap();
    quoted[1..quoted.len() - 1].to_owned()
}

fn secret_streams() -> Vec<(&'static str, Vec<u8>)> {
    let with_text = |text: &str| stream_with_text("openai-api", text).into_bytes();
    let escaped = join(SECRET.chars().map(|c| format!("\\u{:04x}", c as u32)));
    let raw_json = |body: &str| body.replace("HERE", &escaped).into_bytes();
    let hex = join(SECRET.bytes().map(|b| format!("{b:02x}")));
    let mut cases = vec![
        ("raw", with_text(SECRET)),
        (
            "raw inside a longer word",
            with_text(&format!("prefix{SECRET}suffix")),
        ),
        (
            "json escaped by hand",
            raw_json(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"HERE\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n",
            ),
        ),
        (
            "comment line",
            format!(": {SECRET}\n\n{}", stream_with_text("openai-api", "x")).into_bytes(),
        ),
        ("base64 standard", with_text(&b64(SECRET.as_bytes(), STD))),
        ("base64 url-safe", with_text(&b64(SECRET.as_bytes(), URL))),
        ("percent unreserved kept", with_text(&pct(SECRET, false))),
        ("percent everything", with_text(&pct(SECRET, true))),
        ("hex", with_text(&hex)),
        ("hex upper case", with_text(&hex.to_uppercase())),
        ("hex mixed case", with_text(&mixed_case(&hex))),
        (
            "percent lower case",
            with_text(&pct(SECRET, true).to_lowercase()),
        ),
        (
            "percent mixed case",
            with_text(&mixed_case(&pct(SECRET, true))),
        ),
        ("secret in another case", with_text(&SECRET.to_uppercase())),
        (
            "base64 wrapped by MIME line breaks",
            with_text(&wrap(&b64(SECRET.as_bytes(), STD), 6)),
        ),
        ("cut by a line break between data lines", {
            let escaped = json_escaped(SECRET);
            let (a, b) = escaped.split_at(5);
            format!(
                    "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"{a}\ndata: {b}\"}}\n\n{}",
                    stream_with_text("openai-api", "x")
                )
                .into_bytes()
        }),
    ];
    for shift in 1..=2usize {
        let mut prefixed = vec![b'Q'; shift];
        prefixed.extend_from_slice(SECRET.as_bytes());
        // A copy that sits at another alignment inside a bigger base64 text.
        let text = b64(&prefixed, STD);
        cases.push((
            if shift == 1 {
                "base64 at offset 1"
            } else {
                "base64 at offset 2"
            },
            with_text(&text),
        ));
    }
    // Split between two SSE events on the same JSON path.
    let (a, b) = SECRET.split_at(7);
    let split = event("response.created", serde_json::json!({}))
        + &event(
            "response.output_text.delta",
            serde_json::json!({"delta": a}),
        )
        + &event(
            "response.output_text.delta",
            serde_json::json!({"delta": b}),
        )
        + &event(
            "response.completed",
            serde_json::json!({"response":{"status":"completed","model":"native-api-model"}}),
        );
    cases.push(("split between two events", split.into_bytes()));
    cases
}

#[test]
fn a_response_that_carries_the_secret_in_any_covered_form_is_refused() {
    let s = setup("openai-api");
    for (name, body) in secret_streams() {
        let (addr, server) = serve(respond(sse_response(&body)));
        let got = s.run(addr, &CancelToken::new());
        server.join().unwrap();
        assert_eq!(got, Err(TransportError::SecretEcho), "{name}");
        no_leak(&got.unwrap_err());
    }
}

#[test]
fn a_secret_split_between_two_tcp_chunks_is_still_refused() {
    let s = setup("openai-api");
    let body = format!(": {SECRET}\n\n{}", stream_with_text("openai-api", "x"));
    let wire = sse_response(body.as_bytes());
    let cut = head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            format!("Content-Length: {}", body.len()),
        ],
    )
    .len()
        + 2
        + 6;
    let (addr, server) = serve(move |st| {
        st.set_nodelay(true).unwrap();
        st.write_all(&wire[..cut]).unwrap();
        st.flush().unwrap();
        thread::sleep(Duration::from_millis(80));
        st.write_all(&wire[cut..]).unwrap();
    });
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::SecretEcho)
    );
    server.join().unwrap();
}

#[test]
fn a_response_without_the_secret_is_accepted_as_a_control() {
    // Same shapes as the refusal cases, minus the secret: the refusals above
    // come from the secret and not from the shape.
    let s = setup("openai-api");
    for text in ["x", "prefix-suffix", "c2stdGVzdA==", "%73%6B", "736b"] {
        let body = stream_with_text("openai-api", text);
        let (addr, server) = serve(respond(sse_response(body.as_bytes())));
        assert!(s.run(addr, &CancelToken::new()).is_ok(), "{text}");
        server.join().unwrap();
    }
}

#[test]
fn credentials_that_are_too_short_or_not_header_safe_are_refused() {
    for bad in [
        "",
        "short",
        "1234567",
        "has space in it",
        "new\nline-secret",
        "clé-secrète-long",
    ] {
        assert!(
            matches!(Secret::new(bad.into()), Err(TransportError::Credential)),
            "{bad:?}"
        );
    }
    assert!(Secret::new("12345678".into()).is_ok());
    assert!(matches!(
        Redactor::new(&["short"]),
        Err(TransportError::Credential)
    ));
}

#[test]
fn the_redactor_covers_non_ascii_values_and_json_escapes() {
    let value = "clé-secrète-日本語-№1-🔑";
    let redactor = Redactor::new(&[value]).unwrap();
    let python = "cl\\u00e9-secr\\u00e8te-\\u65e5\\u672c\\u8a9e-\\u21161-\\ud83d\\udd11";
    let upper = "cl\\u00E9-secr\\u00E8te-\\u65E5\\u672C\\u8A9E-\\u21161-\\uD83D\\uDD11";
    let full = join(value.encode_utf16().map(|u| format!("\\u{u:04x}")));
    let serde = serde_json::to_string(value).unwrap();
    for form in [
        value.to_string(),
        python.into(),
        upper.into(),
        full,
        serde,
        pct(value, false),
        pct(value, true),
        b64(value.as_bytes(), STD),
        b64(value.as_bytes(), URL),
    ] {
        assert!(redactor.leaks(form.as_bytes()), "{form}");
    }
    let hex = join(value.bytes().map(|b| format!("{b:02x}")));
    let pct_lower = pct(value, true).to_lowercase();
    for form in [
        hex.to_uppercase(),
        mixed_case(&hex),
        pct_lower.clone(),
        mixed_case(&pct_lower),
        pct(value, false).to_lowercase(),
        value.to_ascii_uppercase(),
        wrap(&b64(value.as_bytes(), STD), 5),
    ] {
        assert!(redactor.leaks(form.as_bytes()), "{form}");
    }
    // Case folding is ASCII only: a non-ASCII case variant is not covered.
    assert!(!redactor.leaks(value.to_uppercase().as_bytes()));
    // The escaped solidus is decoded before comparison, and asserted here.
    let slashed = Redactor::new(&["ab/cd-secret/value"]).unwrap();
    assert!(slashed.leaks_in_sse(b"data: {\"type\":\"t\",\"a\":\"ab\\/cd-secret\\/value\"}\n\n"));
    assert!(!slashed.leaks(b"ab-cd-secret-value"));
    // Any JSON escaping of the value is decoded before comparison, including
    // a mix of escaped and plain characters.
    let mixed =
        "data: {\"type\":\"t\",\"a\":\"cl\\u00e9-secr\\u00e8te-日本語-\\u2116\\u0031-🔑\"}\n\n";
    assert!(redactor.leaks_in_sse(mixed.as_bytes()));
    assert!(!redactor.leaks_in_sse(b"data: {\"type\":\"t\",\"a\":\"nothing here\"}\n\n"));
    assert!(!redactor.leaks(b"unrelated text of some length"));
}

#[test]
fn debug_output_never_shows_the_credential() {
    let secret = Secret::new(SECRET.into()).unwrap();
    let endpoint =
        ApprovedEndpoint::parse(SupplementalBridgeKind::OpenAiCompatibleApi, "127.0.0.1:9")
            .unwrap();
    let transport = ProviderTransport::new(endpoint, secret);
    let printed = format!("{transport:?} {:?}", Redactor::new(&[SECRET]).unwrap());
    assert!(!printed.contains("sk-te"));
    assert!(printed.contains("REDACTED"));
}

#[test]
fn ipv6_loopback_is_usable_when_the_host_allows_binding_it() {
    let Ok(listener) = TcpListener::bind("[::1]:0") else {
        eprintln!("SKIPPED: this host cannot bind [::1]");
        return;
    };
    let addr = listener.local_addr().unwrap();
    let s = setup("openai-api");
    let body = stream_with_text("openai-api", "x");
    let server = thread::spawn(move || {
        let (mut st, _) = listener.accept().unwrap();
        let request = read_request(&mut st);
        st.write_all(&sse_response(body.as_bytes())).unwrap();
        request
    });
    assert!(s.run(addr, &CancelToken::new()).unwrap().stream_complete);
    assert!(String::from_utf8_lossy(&server.join().unwrap()).contains("Host: [::1]:"));
}

/// How a saturation attempt ended.
enum Backlog {
    Full(TcpListener, SocketAddr, Vec<TcpStream>),
    /// The host did not saturate, with the reason.
    Unavailable(String),
}

/// Fills the accept backlog of a loopback listener nobody accepts from. Only a
/// connect timeout counts as saturation, and a second probe must time out too.
/// std offers no way to set the listen backlog and this crate has no direct
/// libc or socket2 dependency, so the kernel default decides.
fn full_backlog() -> Backlog {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut held = Vec::new();
    let is_timeout = |e: &std::io::Error| {
        matches!(
            e.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )
    };
    for _ in 0..1024 {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(150)) {
            Ok(stream) => held.push(stream),
            Err(e) if is_timeout(&e) => {
                return match TcpStream::connect_timeout(&addr, Duration::from_millis(150)) {
                    Err(probe) if is_timeout(&probe) => Backlog::Full(listener, addr, held),
                    other => Backlog::Unavailable(format!(
                        "the probe after the first timeout did not time out: {:?}",
                        other.map(|_| "connected")
                    )),
                };
            }
            Err(e) => {
                return Backlog::Unavailable(format!(
                    "connect failed with {:?} after {} connections",
                    e.kind(),
                    held.len()
                ));
            }
        }
    }
    Backlog::Unavailable(format!(
        "{} connections did not fill the backlog",
        held.len()
    ))
}

#[test]
fn cancellation_and_the_deadline_interrupt_a_connection_that_cannot_complete() {
    let (_listener, addr, _held) = match full_backlog() {
        Backlog::Full(listener, addr, held) => (listener, addr, held),
        Backlog::Unavailable(reason) => {
            // Explicit skip: this host cannot build the condition, so the
            // connect guard is untested here. Not a pass.
            eprintln!("SKIPPED connect cancellation test: {reason}");
            return;
        }
    };
    let s = setup_with("openai-api", 30_000, 8192);
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        trigger.cancel();
    });
    let started = Instant::now();
    assert_eq!(s.run(addr, &cancel), Err(TransportError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "connect ignored the token"
    );
    canceller.join().unwrap();
    let s = setup_with("openai-api", 400, 8192);
    let started = Instant::now();
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Timeout)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

fn chunked_head() -> Vec<u8> {
    head(
        "200 OK",
        &[
            "Content-Type: text/event-stream".into(),
            "Transfer-Encoding: chunked".into(),
        ],
    )
}

#[test]
fn chunked_framing_defects_are_refused() {
    let s = setup("openai-api");
    let ok = stream_with_text("openai-api", "x");
    let good = |tail: &str| format!("{:x}\r\n{ok}\r\n{tail}", ok.len());
    let cases: Vec<(&str, String, TransportError)> = vec![
        (
            "chunk extension",
            format!("{:x};ext=1\r\n{ok}\r\n0\r\n\r\n", ok.len()),
            TransportError::Malformed,
        ),
        (
            "trailer",
            good("0\r\nX-Trailer: 1\r\n\r\n"),
            TransportError::Malformed,
        ),
        (
            "non-hex size",
            format!("zz\r\n{ok}\r\n0\r\n\r\n"),
            TransportError::Malformed,
        ),
        (
            "signed size",
            "+5\r\nhello\r\n0\r\n\r\n".to_string(),
            TransportError::Malformed,
        ),
        (
            "space before size",
            " 5\r\nhello\r\n0\r\n\r\n".to_string(),
            TransportError::Malformed,
        ),
        (
            "empty size line",
            format!("\r\n{ok}\r\n0\r\n\r\n"),
            TransportError::Malformed,
        ),
        (
            "nine digit size",
            "000000005\r\nhello\r\n0\r\n\r\n".into(),
            TransportError::Malformed,
        ),
        (
            "sixteen digit size",
            "ffffffffffffffff\r\n".into(),
            TransportError::Malformed,
        ),
        (
            "eight digit huge size",
            "ffffffff\r\n".into(),
            TransportError::Limit,
        ),
        (
            "missing CRLF after data",
            format!("{:x}\r\n{ok}XX0\r\n\r\n", ok.len()),
            TransportError::Malformed,
        ),
        (
            "bytes after the last chunk",
            good("0\r\n\r\nextra"),
            TransportError::Malformed,
        ),
    ];
    // A size line that never ends must be refused as soon as it is too long,
    // not at the deadline.
    let mut wire = chunked_head();
    wire.extend_from_slice("a".repeat(40).as_bytes());
    let (addr, server) = serve(move |st| {
        let _ = st.write_all(&wire);
        hold(st);
    });
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::Malformed),
        "endless size line"
    );
    server.join().unwrap();
    for (name, body, expected) in cases {
        let mut wire = chunked_head();
        wire.extend_from_slice(body.as_bytes());
        let (addr, server) = serve(move |st| {
            let _ = st.write_all(&wire);
        });
        assert_eq!(s.run(addr, &CancelToken::new()), Err(expected), "{name}");
        server.join().unwrap();
    }
}

#[test]
fn unexpected_statuses_and_http_1_0_are_handled_explicitly() {
    let s = setup("openai-api");
    let ok = stream_with_text("openai-api", "x");
    // HTTP/1.0 with no length: read until close, accepted.
    let mut wire = b"HTTP/1.0 200 OK\r\nContent-Type: text/event-stream\r\n\r\n".to_vec();
    wire.extend_from_slice(ok.as_bytes());
    let (addr, server) = serve(move |st| {
        let _ = st.write_all(&wire);
    });
    assert!(s.run(addr, &CancelToken::new()).unwrap().stream_complete);
    server.join().unwrap();
    // Other versions are refused, even with a valid stream behind them.
    for version in ["HTTP/2", "HTTP/1.2", "HTTP/0.9", "http/1.1", "HTTP/1.11"] {
        let mut wire = format!(
            "{version} 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n",
            ok.len()
        )
        .into_bytes();
        wire.extend_from_slice(ok.as_bytes());
        let (addr, server) = serve(respond(wire));
        assert_eq!(
            s.run(addr, &CancelToken::new()),
            Err(TransportError::Malformed),
            "{version}"
        );
        server.join().unwrap();
    }
    // 1xx, other 2xx and every 3xx are Malformed. A redirect is never followed:
    // its target listener sees no connection.
    let (target, target_addr) = silent();
    for status in [
        "100 Continue",
        "101 Switching Protocols",
        "103 Early Hints",
        "201 Created",
        "202 Accepted",
        "204 No Content",
        "206 Partial Content",
        "301 Moved Permanently",
        "302 Found",
        "303 See Other",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        // A complete, valid event stream under each status: only the status
        // makes the response unacceptable.
        let mut wire = head(
            status,
            &[
                format!("Location: http://{target_addr}/v1/responses"),
                "Content-Type: text/event-stream".into(),
                format!("Content-Length: {}", ok.len()),
            ],
        );
        wire.extend_from_slice(ok.as_bytes());
        let (addr, server) = serve(respond(wire));
        assert_eq!(
            s.run(addr, &CancelToken::new()),
            Err(TransportError::Malformed),
            "{status}"
        );
        server.join().unwrap();
    }
    assert_no_connection(&target);
}

#[test]
fn fragments_of_the_secret_alone_do_not_trigger_a_refusal() {
    let s = setup("openai-api");
    // SECRET is sk-te"st\vAlue-9x. None of these carries it whole, in any
    // covered form, once whitespace is removed and case is folded.
    let created = event("response.created", serde_json::json!({"note": "SK-TE"}));
    let delta_a = event(
        "response.output_text.delta",
        serde_json::json!({"delta": "st\\VALUE"}),
    );
    let delta_b = event(
        "response.output_text.delta",
        serde_json::json!({"delta": "-9X and sk-tE|"}),
    );
    let done = event(
        "response.completed",
        serde_json::json!({"response":{"status":"completed","model":"native-api-model"}}),
    );
    let body = format!("{created}{delta_a}{delta_b}{done}");
    let (addr, server) = serve(respond(sse_response(body.as_bytes())));
    assert!(s.run(addr, &CancelToken::new()).is_ok());
    server.join().unwrap();
}

#[test]
fn a_secret_that_only_appears_once_whitespace_is_removed_is_refused_by_design() {
    // Documented false positive: fail closed. A legitimate text that spells the
    // secret with a space inside is refused like a leak.
    let s = setup("openai-api");
    let body = stream_with_text("openai-api", "sk-te\" st\\vAlue-9x");
    let (addr, server) = serve(respond(sse_response(body.as_bytes())));
    assert_eq!(
        s.run(addr, &CancelToken::new()),
        Err(TransportError::SecretEcho)
    );
    server.join().unwrap();
}
