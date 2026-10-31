// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0

//! Regression scenarios from the claude-router analysis (issue #143), applied to
//! the Agent Router decision boundary. Rows: T04, T05, T10, T13.
//! See `docs/claude-router-regression-matrix.md`.
//!
//! Synthetic canaries only. No upstream harness, threshold or retry policy is
//! imported, and nothing here is native qualification.
#[path = "../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)] // Shared fixtures can coexist in one integration test.
mod fixture_proofs;
use router_adapter_api::SnapshotValidation;
use router_catalog::Catalog;
use router_decision::{
    BackendError, BackendOptions, BoundedDecisionContext, DecisionBackend, decide_shadow,
    validate_proposal,
};
use router_policy::PhaseRouter;
use router_types::{
    BackendCandidate, BackendDataPolicy, BackendFailurePolicy, BackendProposal, BackendRequest,
    RiskLevel, StageKind, TaskKind,
};
use std::time::{Duration, Instant};

// Split so the source holds no credential-shaped literal (package-core scan).
const CANARY: &str = concat!("canary-sk", "-", "test-0000-never-a-real-credential");

fn context() -> BoundedDecisionContext {
    let request = serde_json::from_str(include_str!(
        "../../../fixtures/examples/phase-request.json"
    ))
    .unwrap();
    let snapshot = fixture_proofs::from_json(
        include_bytes!("../../../fixtures/examples/phase-snapshot.json"),
        SnapshotValidation {
            now_unix_seconds: 100,
            max_age_seconds: 60,
        },
    )
    .unwrap();
    let catalog = Catalog::from_json(include_bytes!(
        "../../../fixtures/examples/phase-catalog.json"
    ))
    .unwrap();
    let router = PhaseRouter::stable_v2();
    let phase = router.context(request, &snapshot, &catalog).unwrap();
    BoundedDecisionContext::from_phase(&router, &phase).unwrap()
}

fn pool_of(count: usize) -> BackendRequest {
    BackendRequest {
        schema_version: 1,
        artifact_digest: "b".repeat(64),
        kind: TaskKind::Explore,
        risk: RiskLevel::Low,
        phase: StageKind::Plan,
        candidates: (0..count)
            .map(|index| BackendCandidate {
                id: format!("c{index:02}"),
                utility_micros: 10 - index as i64,
                cost_microusd: 1,
                tokens: 100,
                elapsed_ms: 10,
            })
            .collect(),
    }
}

fn proposal(request: &BackendRequest, masses: &[(&str, u32)]) -> BackendProposal {
    BackendProposal {
        schema_version: 1,
        artifact_digest: request.artifact_digest.clone(),
        candidate_id: Some("c00".into()),
        confidence_micros: 900_000,
        probabilities_micros: masses.iter().map(|(k, v)| ((*k).into(), *v)).collect(),
    }
}

/// T10, upstream probe 1: a distribution whose mass is 3 was accepted upstream.
/// Agent Router validates the mass before any selection can be used.
#[test]
fn t10_probability_mass_other_than_one_is_refused_before_selection() {
    let request = pool_of(3);
    let masses_and_expected: [(&[(&str, u32)], bool); 5] = [
        // Positive control: mass exactly 1 over three candidates.
        (
            &[("c00", 500_000), ("c01", 300_000), ("c02", 200_000)],
            true,
        ),
        // Upstream probe shape: three entries of 1.0, mass 3.
        (
            &[("c00", 1_000_000), ("c01", 1_000_000), ("c02", 1_000_000)],
            false,
        ),
        // Mass below 1.
        (
            &[("c00", 100_000), ("c01", 100_000), ("c02", 100_000)],
            false,
        ),
        // One entry at 1.0 plus another entry: mass 1.5.
        (&[("c00", 1_000_000), ("c01", 500_000)], false),
        // A single full-mass entry is a valid distribution.
        (&[("c00", 1_000_000)], true),
    ];
    for (masses, accepted) in masses_and_expected {
        let value = proposal(&request, masses);
        let result = validate_proposal(&request, &value, 800_000);
        if accepted {
            result.unwrap_or_else(|error| panic!("{masses:?} rejected: {error}"));
        } else {
            assert!(
                matches!(result, Err(BackendError::Schema)),
                "{masses:?} -> {result:?}"
            );
        }
    }
}

struct Scripted {
    probabilities: Vec<(&'static str, u32)>,
    pause: Duration,
}
impl DecisionBackend for Scripted {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn propose(
        &self,
        request: &BackendRequest,
        _: Instant,
    ) -> router_decision::Result<BackendProposal> {
        std::thread::sleep(self.pause);
        let mut value = proposal(request, &self.probabilities);
        value.candidate_id = Some("c01".into());
        Ok(value)
    }
}

fn permissive() -> BackendOptions {
    BackendOptions {
        data_policy: BackendDataPolicy::RedactedMetadataToLoopback,
        failure_policy: BackendFailurePolicy::DeterministicFallback,
        ..BackendOptions::default()
    }
}

/// T10 through the public decision entry point: an impossible distribution from
/// a backend must leave only the deterministic baseline active.
#[test]
fn t10_impossible_distribution_never_becomes_a_shadow_selection() {
    let context = context();
    let baseline = context
        .baseline()
        .selected
        .as_ref()
        .map(|choice| choice.candidate.id.clone());
    assert!(baseline.is_some());
    let decision = decide_shadow(
        &context,
        &Scripted {
            probabilities: vec![("c00", 1_000_000), ("c01", 1_000_000)],
            pause: Duration::ZERO,
        },
        permissive(),
    )
    .unwrap();
    assert_eq!(decision.fallback_reason.as_deref(), Some("backend.schema"));
    assert_eq!(decision.shadow_candidate_id, None);
    assert_eq!(decision.confidence_micros, None);
    assert_eq!(decision.active_candidate_id, baseline);
    assert!(!decision.semantic_execution_enabled);

    // Positive control: the same backend with a valid distribution is accepted
    // as a shadow suggestion, so the refusal above is caused by the mass.
    let accepted = decide_shadow(
        &context,
        &Scripted {
            probabilities: vec![("c00", 100_000), ("c01", 900_000)],
            pause: Duration::ZERO,
        },
        permissive(),
    )
    .unwrap();
    assert_eq!(accepted.fallback_reason, None);
    assert!(accepted.shadow_candidate_id.is_some());
}

/// T04: a backend that answers successfully but after the deadline must not
/// apply a selection, whichever failure policy is configured.
#[test]
fn t04_success_after_the_deadline_cannot_apply_a_selection() {
    let context = context();
    for failure_policy in [
        BackendFailurePolicy::Abstain,
        BackendFailurePolicy::DeterministicFallback,
    ] {
        let decision = decide_shadow(
            &context,
            &Scripted {
                probabilities: vec![("c00", 100_000), ("c01", 900_000)],
                pause: Duration::from_millis(150),
            },
            BackendOptions {
                timeout: Duration::from_millis(40),
                failure_policy,
                ..permissive()
            },
        )
        .unwrap();
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("backend.timeout"),
            "{failure_policy:?}"
        );
        assert_eq!(decision.shadow_candidate_id, None);
        assert_eq!(decision.confidence_micros, None);
        assert_eq!(
            decision.active_candidate_id.is_some(),
            failure_policy == BackendFailurePolicy::DeterministicFallback
        );
    }
}

#[cfg(feature = "local-sidecar")]
mod network {
    use super::*;
    use router_decision::JevSidecarBackend;
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        thread::{self, JoinHandle},
    };

    /// Serves one scripted reply and counts every connection attempt during a
    /// fixed observation window, so a hidden retry or second endpoint is seen.
    fn peer(reply: Vec<u8>, delay: Duration) -> (SocketAddr, JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let end = Instant::now() + Duration::from_millis(900);
            let mut connections = 0;
            while Instant::now() < end {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        connections += 1;
                        if connections > 1 {
                            continue;
                        }
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        loop {
                            let mut buffer = [0; 1024];
                            let read = socket.read(&mut buffer).unwrap();
                            if read == 0 {
                                break;
                            }
                            bytes.extend_from_slice(&buffer[..read]);
                            if let Some(offset) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                                let headers = std::str::from_utf8(&bytes[..offset]).unwrap();
                                let length: usize = headers
                                    .lines()
                                    .find_map(|line| line.strip_prefix("Content-Length: "))
                                    .unwrap()
                                    .parse()
                                    .unwrap();
                                if bytes.len() >= offset + 4 + length {
                                    break;
                                }
                            }
                        }
                        thread::sleep(delay);
                        let _ = socket.write_all(&reply);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
            connections
        });
        (address, handle)
    }

    fn http(status: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn propose(address: SocketAddr, budget: Duration) -> Result<BackendProposal, BackendError> {
        let request = pool_of(3);
        JevSidecarBackend::new(address, None)
            .unwrap()
            .propose(&request, Instant::now() + budget)
    }

    /// T05 and T13: refusals, an unavailable dependency and a timeout are
    /// reported through bounded codes, sent once, and never echo the provider body.
    #[test]
    fn t05_t13_failures_are_coded_sent_once_and_never_echo_the_provider_body() {
        let error_body = format!(r#"{{"error":{{"message":"invalid key {CANARY}"}}}}"#);
        let cases: [(Vec<u8>, Duration, &str); 5] = [
            (
                http("401 Unauthorized", &error_body),
                Duration::ZERO,
                "backend.transport",
            ),
            (
                http("403 Forbidden", &error_body),
                Duration::ZERO,
                "backend.transport",
            ),
            (
                http("429 Too Many Requests", &error_body),
                Duration::ZERO,
                "backend.transport",
            ),
            (
                http("500 Internal Server Error", &error_body),
                Duration::ZERO,
                "backend.transport",
            ),
            (
                http("200 OK", "{}"),
                Duration::from_millis(300),
                "backend.timeout",
            ),
        ];
        for (reply, delay, expected) in cases {
            let label = String::from_utf8_lossy(&reply[..20]).into_owned();
            let (address, connections) = peer(reply, delay);
            let error = propose(address, Duration::from_millis(120)).unwrap_err();
            assert_eq!(error.code(), expected, "{label}");
            let shown = format!("{error} {error:?}");
            assert!(!shown.contains(CANARY), "{label}: {shown}");
            assert_eq!(connections.join().unwrap(), 1, "{label}: hidden retry");
        }

        // Absent dependency: nothing listens. The error is the same transport
        // class and carries no endpoint detail beyond its code.
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = unused.local_addr().unwrap();
        drop(unused);
        let error = propose(address, Duration::from_millis(500)).unwrap_err();
        assert_eq!(error.code(), "backend.transport");
    }

    /// T13: malformed JSON, an unknown field inside the answer and a duplicate
    /// key are refused as `backend.schema`; the canary placed in the refused
    /// payload is neither echoed nor turned into a selection.
    #[test]
    fn t13_malformed_unknown_and_duplicate_answers_fail_at_the_schema_boundary() {
        let bodies = [
            r#"{"answers":{"route":"#.to_string(),
            format!(
                r#"{{"answers":{{"route":{{"type":"choice","choice":"c01","confidence":0.5,"answer_confidence":0.9,"{CANARY}":"x"}}}}}}"#
            ),
            format!(
                r#"{{"answers":{{"route":{{"type":"choice","choice":"c01","confidence":0.5,"answer_confidence":0.9,"action":{{"act_probability":0.9,"{CANARY}":1}}}}}}}}"#
            ),
            format!(
                r#"{{"answers":{{"route":{{"type":"choice","choice":"c01","choice":"{CANARY}","confidence":0.5,"answer_confidence":0.9}}}}}}"#
            ),
        ];
        for body in bodies {
            let (address, connections) = peer(http("200 OK", &body), Duration::ZERO);
            let error = propose(address, Duration::from_secs(1)).unwrap_err();
            assert_eq!(error.code(), "backend.schema", "{body}");
            assert!(!format!("{error} {error:?}").contains(CANARY));
            assert_eq!(connections.join().unwrap(), 1);
        }
    }

    /// T13 end to end: a 200 answer naming a canary as the candidate is refused
    /// as out of pool, and the serialized decision contains no trace of it.
    #[test]
    fn t13_foreign_candidate_text_from_the_provider_never_reaches_the_decision() {
        let context = context();
        let body = format!(
            r#"{{"answers":{{"route":{{"type":"choice","choice":"{CANARY}","confidence":0.5,"answer_confidence":0.95}}}}}}"#
        );
        let (address, connections) = peer(http("200 OK", &body), Duration::ZERO);
        let backend = JevSidecarBackend::new(address, None).unwrap();
        let decision = decide_shadow(&context, &backend, permissive()).unwrap();
        assert_eq!(connections.join().unwrap(), 1);
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("backend.out_of_pool")
        );
        assert_eq!(decision.shadow_candidate_id, None);
        let encoded = serde_json::to_string(&decision).unwrap();
        assert!(!encoded.contains(CANARY), "{encoded}");
    }
}
