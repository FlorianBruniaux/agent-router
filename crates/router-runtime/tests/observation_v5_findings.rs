// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Opt-in v5 advisory findings over synthetic stores. No provider, credential or
//! subprocess. Each scenario rebuilds the journal chain, so a missing finding
//! comes from a rule's evidence requirement and not from a broken store.
use router_runtime::{Runtime, RuntimeError, RuntimeLimits, observe_run_v4, observe_run_v5};
use router_types::{OutputRetentionPolicy, RunState, RunSummary};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const PRIVATE: &str = "PRIVATE_PROMPT_PATH_CREDENTIAL_CANARY";
const MODEL: &str = "vendor/model-v5:2026_10.29";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("runtime");
        drop(Runtime::open(&root, RuntimeLimits::default()).unwrap());
        Self { _temp: temp, root }
    }
    fn db(&self) -> Connection {
        let db = Connection::open(self.root.join("runtime.sqlite3")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(3)).unwrap();
        db
    }
    fn findings(&self) -> Value {
        serde_json::to_value(observe_run_v5(&self.root, PRIVATE).unwrap()).unwrap()["findings"]
            .clone()
    }
}
/// Rule names that produced a finding, in output order.
fn fired(findings: &Value) -> Vec<String> {
    findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap().to_owned())
        .collect()
}
fn reason<'a>(findings: &'a Value, rule: &str) -> &'a str {
    findings["suppressed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["rule"] == rule)
        .unwrap_or_else(|| panic!("{rule} neither fired nor suppressed: {findings}"))["reason"]
        .as_str()
        .unwrap()
}

fn insert_run(db: &Connection, state: RunState, attempts: u32, cleaned: bool) {
    let run = RunSummary {
        run_id: PRIVATE.into(),
        definition_hash: "a".repeat(64),
        approval_digest: "b".repeat(64),
        receipt_id: PRIVATE.into(),
        plan_id: PRIVATE.into(),
        node_id: PRIVATE.into(),
        state,
        attempts,
        failure: None,
        worktree: Some(PRIVATE.into()),
        artifact_sha256: None,
        output_retention: OutputRetentionPolicy::Disabled,
        workflow: None,
        artifact_contract_sha256: None,
    };
    db.execute(
        "INSERT INTO runs VALUES(?1,?1,?1,?2)",
        params![PRIVATE, serde_json::to_string(&run).unwrap()],
    )
    .unwrap();
    for ordinal in 1..=attempts {
        db.execute(
            "INSERT INTO run_attempts VALUES(?1,?2,'observed')",
            params![PRIVATE, ordinal],
        )
        .unwrap();
        if cleaned {
            db.execute(
                "INSERT INTO attempt_leases VALUES(?1,?2,?3,1,'\"group_terminated\"')",
                params![PRIVATE, ordinal, PRIVATE],
            )
            .unwrap();
        }
    }
}
fn append(db: &Connection, state: RunState, kind: &str, detail: &str) -> String {
    let (sequence, previous): (u64, String) = db
        .query_row(
            "SELECT sequence,event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, String::new()));
    let mut hasher = Sha256::new();
    hasher.update(b"runtime-event-v1\0");
    hasher.update(
        serde_json::to_vec(&(sequence + 1, PRIVATE, kind, state, detail, &previous)).unwrap(),
    );
    let hash = hex::encode(hasher.finalize());
    db.execute(
        "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            sequence + 1,
            PRIVATE,
            kind,
            serde_json::to_string(&state).unwrap(),
            detail,
            previous,
            hash
        ],
    )
    .unwrap();
    hash
}
fn approve(db: &Connection) {
    append(db, RunState::Planned, "planned", "");
    append(db, RunState::AwaitingApproval, "transition", "reviewed");
    append(db, RunState::Queued, "transition", "approved");
    append(db, RunState::Preparing, "transition", "preparing");
    // A run that entered Preparing needs a settled preparation record before its
    // cleanup can be read as anything but unknown.
    let attempts: u32 = db
        .query_row(
            "SELECT json_extract(summary,'$.attempts') FROM runs",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let record = json!({
        "schema_version":1, "sequence":1, "run_id":PRIVATE,
        "definition_hash":"a".repeat(64), "workflow":null, "next_attempt":attempts,
        "stage":"complete", "captures":0, "probes":[]
    })
    .to_string();
    db.execute(
        "INSERT INTO preparation_cycles VALUES(?1,1,?2)",
        params![PRIVATE, record],
    )
    .unwrap();
    append(db, RunState::Preparing, "preparation.cycle.v1", &record);
}
/// One attempt with validated evidence. `transport` is (termination, exit_code).
fn attempt(db: &Connection, ordinal: u32, end: RunState, transport: (&str, Option<i32>)) {
    let start = append(db, RunState::Preparing, "attempt.started", "");
    append(db, RunState::Running, "transition", "prepared");
    let finish = append(db, RunState::Running, "attempt.finished", "observed");
    let transition = append(db, end, "transition", "transport_observed");
    let evidence = json!({
        "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":ordinal},
        "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
        "decision_origin":"deterministic", "sandbox":"read_only",
        "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
        "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
        "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
        "state":end, "failure":null,
        "transport":{"termination":transport.0,"exit_code":transport.1,"elapsed_ms":987654321},
        "cleanup":"unknown", "observed_cost_microusd":null,
        "observed_tokens":null, "started_event_hash":start,
        "finished_event_hash":finish, "transition_event_hash":transition
    });
    append(db, end, "attempt.evidence.v1", &evidence.to_string());
}
/// Like `attempt`, with a cancel request recorded while the attempt runs.
fn cancelled_attempt(db: &Connection, ordinal: u32, end: RunState, transport: (&str, Option<i32>)) {
    let start = append(db, RunState::Preparing, "attempt.started", "");
    append(db, RunState::Running, "transition", "prepared");
    append(db, RunState::Running, "control.cancel_requested", "");
    let finish = append(db, RunState::Running, "attempt.finished", "observed");
    let transition = append(db, end, "transition", "transport_observed");
    let evidence = json!({
        "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":ordinal},
        "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
        "decision_origin":"deterministic", "sandbox":"read_only",
        "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
        "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
        "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
        "state":end, "failure":null,
        "transport":{"termination":transport.0,"exit_code":transport.1,"elapsed_ms":987654321},
        "cleanup":"unknown", "observed_cost_microusd":null,
        "observed_tokens":null, "started_event_hash":start,
        "finished_event_hash":finish, "transition_event_hash":transition
    });
    append(db, end, "attempt.evidence.v1", &evidence.to_string());
}
fn retry(db: &Connection) {
    append(
        db,
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    append(db, RunState::Queued, "transition", "approved");
    append(db, RunState::Preparing, "transition", "preparing");
}

/// Builds a store with `build` run inside one transaction.
fn store(
    state: RunState,
    attempts: u32,
    cleaned: bool,
    build: impl FnOnce(&Connection),
) -> Fixture {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, state, attempts, cleaned);
    build(&tx);
    tx.commit().unwrap();
    f
}
fn healthy() -> Fixture {
    store(RunState::Verified, 1, true, |db| {
        approve(db);
        attempt(db, 1, RunState::SucceededTransport, ("exited", Some(0)));
        append(
            db,
            RunState::Verifying,
            "transition",
            "independent_verification_required",
        );
        append(db, RunState::Verified, "transition", "verified");
    })
}
fn defective_retry() -> Fixture {
    store(RunState::Failed, 2, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
        retry(db);
        attempt(db, 2, RunState::Failed, ("timed_out", None));
    })
}

#[test]
fn healthy_run_has_no_finding_and_every_rule_explains_its_suppression() {
    let findings = healthy().findings();
    assert_eq!(findings["findings"], json!([]));
    assert_eq!(findings["automatic_action"], "none");
    assert_eq!(findings["rules_version"], 1);
    let suppressed: Vec<&str> = findings["suppressed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["rule"].as_str().unwrap())
        .collect();
    assert_eq!(
        suppressed,
        [
            "repeated_failed_attempts",
            "cleanup_unconfirmed",
            "late_result_after_cancel",
            "attempt_evidence_missing",
            "attempt_duration"
        ]
    );
    assert_eq!(
        reason(&findings, "repeated_failed_attempts"),
        "below_threshold"
    );
    assert_eq!(reason(&findings, "cleanup_unconfirmed"), "cleanup_recorded");
    assert_eq!(
        reason(&findings, "late_result_after_cancel"),
        "no_cancel_request"
    );
    assert_eq!(reason(&findings, "attempt_evidence_missing"), "no_gap");
    assert_eq!(
        reason(&findings, "attempt_duration"),
        "timing_not_qualified"
    );
}

#[test]
fn defective_retry_links_both_failed_attempts_to_their_events() {
    let f = defective_retry();
    let findings = f.findings();
    assert_eq!(fired(&findings), ["repeated_failed_attempts"]);
    let finding = &findings["findings"][0];
    assert_eq!(finding["severity"], "warning");
    assert_eq!(finding["basis"], "recorded_journal");
    assert_eq!(finding["affected_attempts"], json!([1, 2]));
    let evidence = finding["evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 2);
    // Each reference resolves to a finished marker of the same snapshot.
    let report = serde_json::to_value(observe_run_v4(&f.root, PRIVATE).unwrap()).unwrap();
    for reference in evidence {
        let event = report["known"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["sequence"] == reference["sequence"])
            .unwrap();
        assert_eq!(event["event_sha256"], reference["event_sha256"]);
    }
    for key in ["fact", "hypothesis", "check"] {
        assert!(!finding[key].as_str().unwrap().is_empty(), "{key}");
    }
    assert!(!finding["preserve"].as_array().unwrap().is_empty());
}

#[test]
fn a_single_failure_or_a_cancellation_is_not_a_repeated_failure() {
    let one = store(RunState::Failed, 1, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(2)));
    });
    assert_eq!(
        reason(&one.findings(), "repeated_failed_attempts"),
        "below_threshold"
    );
    // An attempt that ended because it was cancelled is not counted as a defect.
    let cancelled = store(RunState::Failed, 2, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("cancelled", None));
        retry(db);
        attempt(db, 2, RunState::Failed, ("exited", Some(1)));
    });
    assert_eq!(
        reason(&cancelled.findings(), "repeated_failed_attempts"),
        "below_threshold"
    );
    // An exit code that was not recorded is unknown, not a failure.
    let unknown = store(RunState::Failed, 2, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", None));
        retry(db);
        attempt(db, 2, RunState::Failed, ("exited", None));
    });
    assert_eq!(
        reason(&unknown.findings(), "repeated_failed_attempts"),
        "below_threshold"
    );
}

#[test]
fn missing_attempt_evidence_suppresses_the_retry_rule_and_proposes_a_check() {
    let f = store(RunState::Failed, 2, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
        retry(db);
        append(db, RunState::Preparing, "attempt.started", "");
        append(db, RunState::Failed, "transition", "crashed");
    });
    let findings = f.findings();
    assert_eq!(
        reason(&findings, "repeated_failed_attempts"),
        "attempt_evidence_incomplete"
    );
    assert_eq!(fired(&findings), ["attempt_evidence_missing"]);
    let finding = &findings["findings"][0];
    assert_eq!(finding["severity"], "info");
    assert_eq!(finding["affected_attempts"], json!([2]));
    assert!(finding["check"].as_str().unwrap().contains("observe"));
}

#[test]
fn an_attempt_still_running_is_not_reported_as_missing_evidence() {
    let f = store(RunState::Running, 1, false, |db| {
        approve(db);
        append(db, RunState::Preparing, "attempt.started", "");
        append(db, RunState::Running, "transition", "prepared");
    });
    let findings = f.findings();
    assert_eq!(findings["findings"], json!([]));
    assert_eq!(
        reason(&findings, "attempt_evidence_missing"),
        "run_in_progress"
    );
    assert_eq!(reason(&findings, "cleanup_unconfirmed"), "run_not_terminal");
}

#[test]
fn unresolved_cleanup_after_a_terminal_run_is_surfaced_without_claiming_termination() {
    let f = store(RunState::Cancelled, 1, false, |db| {
        approve(db);
        attempt(db, 1, RunState::Cancelled, ("cancelled", None));
    });
    let findings = f.findings();
    assert_eq!(fired(&findings), ["cleanup_unconfirmed"]);
    let finding = &findings["findings"][0];
    assert_eq!(finding["severity"], "warning");
    let text = format!("{} {}", finding["fact"], finding["hypothesis"]);
    assert!(text.contains("not confirmed"), "{text}");
    assert!(!text.contains("still running"));
}

#[test]
fn a_run_that_never_started_has_no_cleanup_finding() {
    let f = store(RunState::Cancelled, 0, false, |db| {
        append(db, RunState::Planned, "planned", "");
        append(db, RunState::AwaitingApproval, "transition", "reviewed");
        append(
            db,
            RunState::AwaitingApproval,
            "control.cancel_requested",
            "",
        );
        append(
            db,
            RunState::Cancelled,
            "control.cancelled_before_start",
            "",
        );
    });
    let findings = f.findings();
    assert_eq!(findings["findings"], json!([]));
    assert_eq!(
        reason(&findings, "cleanup_unconfirmed"),
        "no_attempt_started"
    );
}

#[test]
fn a_result_recorded_after_cancel_is_reported_with_both_sequences() {
    let f = store(RunState::Cancelled, 1, true, |db| {
        approve(db);
        let start = append(db, RunState::Preparing, "attempt.started", "");
        append(db, RunState::Running, "transition", "prepared");
        append(db, RunState::Running, "control.cancel_requested", "");
        let finish = append(db, RunState::Running, "attempt.finished", "observed");
        let transition = append(db, RunState::Cancelled, "transition", "transport_observed");
        let evidence = json!({
            "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":1},
            "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
            "decision_origin":"deterministic", "sandbox":"read_only",
            "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
            "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
            "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
            "state":"cancelled", "failure":null,
            "transport":{"termination":"exited","exit_code":0,"elapsed_ms":1},
            "cleanup":"unknown", "observed_cost_microusd":null,
            "observed_tokens":null, "started_event_hash":start,
            "finished_event_hash":finish, "transition_event_hash":transition
        });
        append(
            db,
            RunState::Cancelled,
            "attempt.evidence.v1",
            &evidence.to_string(),
        );
    });
    let findings = f.findings();
    assert_eq!(fired(&findings), ["late_result_after_cancel"]);
    let finding = &findings["findings"][0];
    assert_eq!(finding["severity"], "info");
    let sequences: Vec<u64> = finding["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["sequence"].as_u64().unwrap())
        .collect();
    assert_eq!(sequences, [8, 9]);
    assert!(
        finding["hypothesis"]
            .as_str()
            .unwrap()
            .contains("not applied")
    );
}

#[test]
fn identities_are_stable_across_reads_and_follow_the_evidence_not_the_horizon() {
    let f = defective_retry();
    let first = serde_json::to_vec(&observe_run_v5(&f.root, PRIVATE).unwrap()).unwrap();
    assert_eq!(
        first,
        serde_json::to_vec(&observe_run_v5(&f.root, PRIVATE).unwrap()).unwrap()
    );
    let before = f.findings();
    let id = before["findings"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(id.len(), 64);

    // Another run's later event moves the horizon and the source snapshot, but
    // does not change what this run's finding is about.
    let db = f.db();
    let prev: String = db
        .query_row(
            "SELECT event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let last: u64 = db
        .query_row("SELECT max(sequence) FROM events", [], |r| r.get(0))
        .unwrap();
    let mut hasher = Sha256::new();
    hasher.update(b"runtime-event-v1\0");
    hasher.update(
        serde_json::to_vec(&(
            last + 1,
            "other",
            "transition",
            RunState::Planned,
            "",
            &prev,
        ))
        .unwrap(),
    );
    let hash = hex::encode(hasher.finalize());
    let mut other = serde_json::to_value(RunSummary {
        run_id: "other".into(),
        definition_hash: "a".repeat(64),
        approval_digest: "b".repeat(64),
        receipt_id: "other".into(),
        plan_id: "other".into(),
        node_id: "other".into(),
        state: RunState::Planned,
        attempts: 0,
        failure: None,
        worktree: None,
        artifact_sha256: None,
        output_retention: OutputRetentionPolicy::Disabled,
        workflow: None,
        artifact_contract_sha256: None,
    })
    .unwrap();
    other["run_id"] = json!("other");
    db.execute(
        "INSERT INTO runs VALUES('other','other','other',?1)",
        [other.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO events VALUES(?1,'other','transition','\"planned\"','',?2,?3)",
        params![last + 1, prev, hash],
    )
    .unwrap();
    let after = f.findings();
    assert_eq!(after["findings"][0]["id"], id);
    assert_ne!(
        after["source_snapshot_sha256"],
        before["source_snapshot_sha256"]
    );
}

#[test]
fn a_new_failed_attempt_changes_the_identity_and_never_reuses_the_old_one() {
    let two = defective_retry().findings();
    let three = store(RunState::Failed, 3, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
        retry(db);
        attempt(db, 2, RunState::Failed, ("timed_out", None));
        retry(db);
        attempt(db, 3, RunState::Failed, ("killed", None));
    })
    .findings();
    assert_eq!(three["findings"][0]["affected_attempts"], json!([1, 2, 3]));
    assert_ne!(two["findings"][0]["id"], three["findings"][0]["id"]);
}

#[test]
fn corrupt_journal_gives_a_refusal_and_no_finding() {
    let f = defective_retry();
    let db = f.db();
    db.execute_batch("DROP TRIGGER events_no_update").unwrap();
    db.execute("UPDATE events SET detail='tampered' WHERE sequence=3", [])
        .unwrap();
    let error = observe_run_v5(&f.root, PRIVATE).unwrap_err();
    assert!(matches!(error, RuntimeError::Integrity(_)), "{error:?}");
    assert!(!format!("{error} {error:?}").contains(PRIVATE));
}

#[test]
fn legacy_missing_timing_creates_no_finding_and_output_is_redacted_and_read_only() {
    let f = defective_retry();
    let snapshot = |db: &Connection| -> (i64, String) {
        db.query_row(
            "SELECT (SELECT count(*) FROM events),(SELECT group_concat(event_hash) FROM events)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    let before = snapshot(&f.db());
    let text = serde_json::to_string(&observe_run_v5(&f.root, PRIVATE).unwrap()).unwrap();
    assert_eq!(before, snapshot(&f.db()));
    for needle in [
        PRIVATE,
        "987654321",
        "transport_observed",
        "reviewed",
        "crashed",
    ] {
        assert!(!text.contains(needle), "leaked {needle}");
    }
    let findings = f.findings();
    assert!(!fired(&findings).contains(&"attempt_duration".to_owned()));
    assert_eq!(
        reason(&findings, "attempt_duration"),
        "timing_not_qualified"
    );
}

#[test]
fn v5_extends_v4_and_has_its_own_digest_domain() {
    let f = defective_retry();
    let v4 = observe_run_v4(&f.root, PRIVATE).unwrap();
    let mut v5 = observe_run_v5(&f.root, PRIVATE).unwrap();
    assert_eq!(v5.observation.timeline, v4.timeline);
    assert_eq!(v5.observation.observation.timings, v4.observation.timings);
    assert_eq!(
        v5.findings.source_snapshot_sha256,
        v4.observation.observation.summary.snapshot_sha256
    );
    assert_eq!(
        v5.observation
            .observation
            .observation
            .summary
            .schema_version,
        5
    );
    let digest = v5
        .observation
        .observation
        .observation
        .summary
        .snapshot_sha256
        .clone();
    v5.observation
        .observation
        .observation
        .summary
        .snapshot_sha256
        .clear();
    let mut hasher = Sha256::new();
    hasher.update(b"run-observation-v5\0");
    hasher.update(serde_json::to_vec(&v5).unwrap());
    assert_eq!(digest, hex::encode(hasher.finalize()));
}

#[test]
fn an_ordinary_cancellation_is_not_a_late_result() {
    // The supervisor itself reports `cancelled`: the request was honoured.
    let f = store(RunState::Cancelled, 1, true, |db| {
        approve(db);
        cancelled_attempt(db, 1, RunState::Cancelled, ("cancelled", None));
    });
    let findings = f.findings();
    assert!(!fired(&findings).contains(&"late_result_after_cancel".to_owned()));
    assert_eq!(
        reason(&findings, "late_result_after_cancel"),
        "ordinary_cancellation"
    );
}

#[test]
fn a_late_result_survives_a_later_retry_with_its_own_cancel_request() {
    let f = store(RunState::SucceededTransport, 2, true, |db| {
        approve(db);
        cancelled_attempt(db, 1, RunState::Failed, ("exited", Some(0)));
        retry(db);
        attempt(db, 2, RunState::SucceededTransport, ("exited", Some(0)));
    });
    let findings = f.findings();
    assert_eq!(fired(&findings), ["late_result_after_cancel"]);
    let finding = &findings["findings"][0];
    assert_eq!(finding["affected_attempts"], json!([1]));
    let sequences: Vec<u64> = finding["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["sequence"].as_u64().unwrap())
        .collect();
    // Cancel request (8) and attempt 1's finished marker (9); attempt 2 is normal.
    assert_eq!(sequences, [8, 9]);
}

#[test]
fn cleanup_confirmed_before_a_later_unknown_marker_does_not_hide_the_finding() {
    let confirmed_then_unknown = store(RunState::Failed, 1, false, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
        append(
            db,
            RunState::Failed,
            "process.cleanup_confirmed",
            "\"group_terminated\"",
        );
        append(
            db,
            RunState::Failed,
            "process.cleanup_unknown",
            "authority_retained",
        );
    });
    let findings = confirmed_then_unknown.findings();
    assert!(fired(&findings).contains(&"cleanup_unconfirmed".to_owned()));
    let unknown_then_confirmed = store(RunState::Failed, 1, false, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
        append(
            db,
            RunState::Failed,
            "process.cleanup_unknown",
            "authority_retained",
        );
        append(
            db,
            RunState::Failed,
            "process.cleanup_confirmed",
            "\"group_terminated\"",
        );
    });
    assert_eq!(
        reason(&unknown_then_confirmed.findings(), "cleanup_unconfirmed"),
        "cleanup_marker_confirmed"
    );
}

#[test]
fn source_snapshot_digest_is_recomputable_from_the_report() {
    let f = defective_retry();
    let v5 = observe_run_v5(&f.root, PRIVATE).unwrap();
    // Documented recipe: take the embedded v4 report, restore schema_version 4,
    // empty its own digest and hash it under the `run-observation-v4` domain.
    let mut v4 = v5.observation.clone();
    v4.observation.observation.summary.schema_version = 4;
    v4.observation.observation.summary.snapshot_sha256.clear();
    let mut hasher = Sha256::new();
    hasher.update(b"run-observation-v4\0");
    hasher.update(serde_json::to_vec(&v4).unwrap());
    assert_eq!(
        v5.findings.source_snapshot_sha256,
        hex::encode(hasher.finalize())
    );
    // A different journal generation cannot carry the same source digest.
    let other = store(RunState::Failed, 1, true, |db| {
        approve(db);
        attempt(db, 1, RunState::Failed, ("exited", Some(1)));
    });
    assert_ne!(
        observe_run_v5(&other.root, PRIVATE)
            .unwrap()
            .findings
            .source_snapshot_sha256,
        v5.findings.source_snapshot_sha256
    );
}
