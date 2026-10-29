//! Opt-in v4 timeline over synthetic stores. No provider, credential or subprocess.
//! Every scenario rebuilds the journal hash chain, so refusals and projections
//! come from semantics rather than from a broken chain.
use router_runtime::{
    Runtime, RuntimeError, RuntimeLimits, observe_run, observe_run_v2, observe_run_v3,
    observe_run_v4,
};
use router_types::{OutputRetentionPolicy, RunState, RunSummary};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

const PRIVATE: &str = "PRIVATE_PROMPT_PATH_CREDENTIAL_CANARY";
const MODEL: &str = "vendor/model-v4:2026_10.28";

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
    fn timeline(&self) -> Value {
        serde_json::to_value(observe_run_v4(&self.root, PRIVATE).unwrap()).unwrap()["timeline"]
            .clone()
    }
}

fn summary(state: RunState, attempts: u32) -> RunSummary {
    RunSummary {
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
    }
}
fn insert_run(db: &Connection, state: RunState, attempts: u32) {
    db.execute(
        "INSERT INTO runs VALUES(?1,?1,?1,?2)",
        params![
            PRIVATE,
            serde_json::to_string(&summary(state, attempts)).unwrap()
        ],
    )
    .unwrap();
    for ordinal in 1..=attempts {
        db.execute(
            "INSERT INTO run_attempts VALUES(?1,?2,'observed')",
            params![PRIVATE, ordinal],
        )
        .unwrap();
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
fn evidence(
    ordinal: u32,
    state: RunState,
    start: &str,
    finish: &str,
    transition: &str,
    tokens: Option<u64>,
    cost: Option<u64>,
) -> String {
    json!({
        "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":ordinal},
        "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
        "decision_origin":"deterministic", "sandbox":"read_only",
        "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
        "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
        "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
        "state":state, "failure":null,
        "transport":{"termination":"exited","exit_code":0,"elapsed_ms":987654321},
        "cleanup":"unknown", "observed_cost_microusd":cost,
        "observed_tokens":tokens, "started_event_hash":start,
        "finished_event_hash":finish, "transition_event_hash":transition
    })
    .to_string()
}
/// planned, awaiting_approval, queued, preparing: the path to a first start.
fn approved_prefix(db: &Connection) {
    append(db, RunState::Planned, "planned", "");
    append(db, RunState::AwaitingApproval, "transition", "reviewed");
    append(db, RunState::Queued, "transition", "approved");
    append(db, RunState::Preparing, "transition", "preparing");
}
/// One complete attempt ending in `end`, returning the evidence event hash.
fn attempt(
    db: &Connection,
    ordinal: u32,
    end: RunState,
    tokens: Option<u64>,
    cost: Option<u64>,
) -> String {
    let start = append(db, RunState::Preparing, "attempt.started", "");
    append(db, RunState::Running, "transition", "prepared");
    let finish = append(db, RunState::Running, "attempt.finished", "observed");
    let transition = append(db, end, "transition", "transport_observed");
    append(
        db,
        end,
        "attempt.evidence.v1",
        &evidence(ordinal, end, &start, &finish, &transition, tokens, cost),
    )
}

fn success_store() -> Fixture {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Verified, 1);
    approved_prefix(&tx);
    attempt(&tx, 1, RunState::SucceededTransport, Some(1200), Some(340));
    append(
        &tx,
        RunState::SucceededTransport,
        "protocol.observed",
        "decoded_not_verified",
    );
    append(
        &tx,
        RunState::Verifying,
        "transition",
        "independent_verification_required",
    );
    append(&tx, RunState::Verifying, "verification.recorded", PRIVATE);
    append(&tx, RunState::Verified, "transition", "verified");
    tx.commit().unwrap();
    f
}

#[test]
fn success_run_separates_plan_approval_attempt_transport_and_verification() {
    let f = success_store();
    let t = f.timeline();
    assert_eq!(t["ordering"], "journal_sequence_ascending");
    assert_eq!(t["run_events"], 13);
    assert_eq!(t["shown"], 13);
    assert_eq!(t["omitted_earlier"], 0);
    assert_eq!(t["approval"]["outcome"], "granted");
    assert_eq!(t["approval"]["awaiting_sequence"], 2);
    assert_eq!(t["approval"]["granted_sequence"], 3);
    assert_eq!(t["verification"]["state"], "verified");
    assert_eq!(t["verification"]["record_sequence"], 12);
    let phases: Vec<(&str, u64)> = t["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["phase"].as_str().unwrap(), p["events"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        phases,
        [
            ("plan", 1),
            ("approval", 2),
            ("preparation", 1),
            ("attempt", 3),
            ("transport", 3),
            ("verification", 3),
            ("cancellation", 0),
            ("cleanup", 0),
            ("terminal", 0),
            ("other", 0),
        ]
    );
    let first = &t["entries"][0];
    assert_eq!(first["sequence"], 1);
    assert_eq!(first["kind"], "planned");
    assert_eq!(first["phase"], "plan");
    assert_eq!(first["attempt"], Value::Null);
    let started = &t["entries"][4];
    assert_eq!(started["kind"], "attempt_started");
    assert_eq!(started["attempt"], 1);
}

#[test]
fn requested_is_known_applied_and_acceptance_stay_unknown() {
    let t = success_store().timeline();
    let attempt = &t["attempts"][0];
    assert_eq!(attempt["ordinal"], 1);
    assert_eq!(attempt["requested"]["model"], MODEL);
    assert_eq!(attempt["requested"]["effort"], "high");
    assert_eq!(attempt["applied"]["model"], "unknown");
    assert_eq!(attempt["applied"]["effort"], "unknown");
    assert_eq!(t["unknown"]["exact_revision_acceptance"], "unknown");
    assert_eq!(t["unknown"]["notification_delivery"], "unknown");
    assert_eq!(t["unknown"]["lost_notifications"], "unknown");
    assert_eq!(t["unknown"]["event_timestamps"], "unknown");
    assert_eq!(t["unknown"]["physical_cleanup"], "unknown");
    assert_eq!(t["unknown"]["objective"], "unknown");
}

#[test]
fn partial_usage_is_not_completed_with_zero() {
    let partial = {
        let f = Fixture::new();
        let mut db = f.db();
        let tx = db.transaction().unwrap();
        insert_run(&tx, RunState::SucceededTransport, 1);
        approved_prefix(&tx);
        attempt(&tx, 1, RunState::SucceededTransport, Some(77), None);
        tx.commit().unwrap();
        f.timeline()
    };
    assert_eq!(partial["attempts"][0]["usage"]["tokens"], 77);
    assert_eq!(
        partial["attempts"][0]["usage"]["cost_microusd"],
        Value::Null
    );
    assert_eq!(partial["attempts"][0]["usage"]["status"], "partial");
    let none = {
        let f = Fixture::new();
        let mut db = f.db();
        let tx = db.transaction().unwrap();
        insert_run(&tx, RunState::SucceededTransport, 1);
        approved_prefix(&tx);
        attempt(&tx, 1, RunState::SucceededTransport, None, None);
        tx.commit().unwrap();
        f.timeline()
    };
    assert_eq!(none["attempts"][0]["usage"]["status"], "unknown");
    let both = success_store().timeline();
    assert_eq!(
        both["attempts"][0]["usage"]["status"],
        "recorded_unattested"
    );
}

#[test]
fn denied_approval_is_not_granted_and_has_no_attempt() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Cancelled, 0);
    append(&tx, RunState::Planned, "planned", "");
    append(&tx, RunState::AwaitingApproval, "transition", "reviewed");
    append(
        &tx,
        RunState::AwaitingApproval,
        "control.cancel_requested",
        "",
    );
    append(
        &tx,
        RunState::Cancelled,
        "control.cancelled_before_start",
        "",
    );
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["approval"]["outcome"], "not_granted");
    assert_eq!(t["approval"]["granted_sequence"], Value::Null);
    assert_eq!(t["cancellation"]["requested_sequence"], 3);
    assert_eq!(t["cancellation"]["before_start_sequence"], 4);
    assert_eq!(t["attempts"], json!([]));
    assert_eq!(t["verification"]["state"], "not_reached");
}

#[test]
fn late_result_after_cancel_is_flagged_and_cancellation_is_not_cleanup() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Cancelled, 1);
    approved_prefix(&tx);
    let start = append(&tx, RunState::Preparing, "attempt.started", "");
    append(&tx, RunState::Running, "transition", "prepared");
    append(&tx, RunState::Running, "control.cancel_requested", "");
    let finish = append(&tx, RunState::Running, "attempt.finished", "observed");
    let transition = append(&tx, RunState::Cancelled, "transition", "transport_observed");
    append(
        &tx,
        RunState::Cancelled,
        "attempt.evidence.v1",
        &evidence(
            1,
            RunState::Cancelled,
            &start,
            &finish,
            &transition,
            None,
            None,
        ),
    );
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["cancellation"]["requested_sequence"], 7);
    assert_eq!(t["cancellation"]["terminal_cancelled"], true);
    assert_eq!(t["cancellation"]["events_after_request"], 3);
    assert_eq!(t["attempts"][0]["result_after_cancel_request"], true);
    let late: Vec<&str> = t["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["after_cancel_request"] == true)
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(late, ["attempt_finished", "transition", "attempt_evidence"]);
    // A cancellation request and a cancelled state never imply physical cleanup.
    assert_eq!(t["cleanup"]["recorded"], "unknown");
    assert_eq!(t["cleanup"]["confirmed_sequence"], Value::Null);
    assert_eq!(t["unknown"]["physical_cleanup"], "unknown");
}

fn incomplete_store() -> Fixture {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Failed, 1);
    approved_prefix(&tx);
    let start = append(&tx, RunState::Preparing, "attempt.started", "");
    append(&tx, RunState::Running, "transition", "prepared");
    append(
        &tx,
        RunState::Running,
        "output.incomplete",
        "stream_reader_deadline",
    );
    let finish = append(&tx, RunState::Running, "attempt.finished", "observed");
    let transition = append(&tx, RunState::Failed, "transition", "transport_observed");
    append(
        &tx,
        RunState::Failed,
        "attempt.evidence.v1",
        &evidence(
            1,
            RunState::Failed,
            &start,
            &finish,
            &transition,
            None,
            None,
        ),
    );
    append(
        &tx,
        RunState::Failed,
        "process.cleanup_unknown",
        "authority_retained",
    );
    tx.commit().unwrap();
    f
}

#[test]
fn incomplete_capture_keeps_notification_loss_unknown() {
    let f = incomplete_store();
    let t = f.timeline();
    let kinds: Vec<&str> = t["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"output_incomplete"));
    assert_eq!(t["unknown"]["lost_notifications"], "unknown");
    assert_eq!(t["unknown"]["notification_delivery"], "unknown");
    assert_eq!(t["cleanup"]["unknown_sequence"], 11);
    assert_eq!(t["cleanup"]["confirmed_sequence"], Value::Null);
}

#[test]
fn entries_are_bounded_with_explicit_omission_and_phase_summary_survives() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Preparing, 0);
    approved_prefix(&tx);
    for _ in 0..200 {
        append(&tx, RunState::Preparing, "preparation.git_returned", "");
    }
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["entry_limit"], 64);
    assert_eq!(t["run_events"], 204);
    assert_eq!(t["shown"], 64);
    assert_eq!(t["omitted_earlier"], 140);
    let entries = t["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 64);
    assert_eq!(entries[0]["sequence"], 141);
    assert_eq!(entries[63]["sequence"], 204);
    // The plan event is outside the window but its phase summary is not lost.
    assert_eq!(t["phases"][0]["phase"], "plan");
    assert_eq!(t["phases"][0]["first_sequence"], 1);
    assert_eq!(t["approval"]["outcome"], "granted");
}

#[test]
fn later_events_of_other_runs_are_counted_not_exported() {
    let f = success_store();
    let db = f.db();
    let prev: String = db
        .query_row(
            "SELECT event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut hasher = Sha256::new();
    hasher.update(b"runtime-event-v1\0");
    hasher.update(
        serde_json::to_vec(&(
            14u64,
            "other_private_run",
            "transition",
            RunState::Planned,
            "",
            &prev,
        ))
        .unwrap(),
    );
    let hash = hex::encode(hasher.finalize());
    let mut other = summary(RunState::Planned, 0);
    other.run_id = "other_private_run".into();
    db.execute(
        "INSERT INTO runs VALUES(?1,?1,?1,?2)",
        params!["other_private_run", serde_json::to_string(&other).unwrap()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO events VALUES(14,'other_private_run','transition','\"planned\"','',?1,?2)",
        params![prev, hash],
    )
    .unwrap();
    let report = observe_run_v4(&f.root, PRIVATE).unwrap();
    let text = serde_json::to_string(&report).unwrap();
    assert!(!text.contains("other_private_run"));
    assert_eq!(report.timeline.later_global_events, 1);
}

#[test]
fn unknown_kinds_are_classified_without_publishing_the_kind() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Planned, 0);
    append(&tx, RunState::Planned, "planned", "");
    append(&tx, RunState::Planned, &format!("weird.{PRIVATE}"), PRIVATE);
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["entries"][1]["kind"], "unclassified");
    assert_eq!(t["entries"][1]["phase"], "other");
    assert_eq!(t["phases"][9]["events"], 1);
}

#[test]
fn v4_extends_v3_without_changing_any_earlier_projection() {
    let f = success_store();
    let v1 = observe_run(&f.root, PRIVATE).unwrap();
    let v2 = observe_run_v2(&f.root, PRIVATE).unwrap();
    let v3 = observe_run_v3(&f.root, PRIVATE).unwrap();
    let mut v4 = observe_run_v4(&f.root, PRIVATE).unwrap();
    assert_eq!(v4.observation.observation.summary.identity, v1.identity);
    assert_eq!(v4.observation.observation.summary.known, v1.known);
    assert_eq!(v4.observation.observation.attempts, v2.attempts);
    assert_eq!(v4.observation.timings, v3.timings);
    assert_eq!(v4.observation.observation.summary.schema_version, 4);
    let digest = v4.observation.observation.summary.snapshot_sha256.clone();
    assert_ne!(digest, v3.observation.summary.snapshot_sha256);
    v4.observation.observation.summary.snapshot_sha256.clear();
    let mut hasher = Sha256::new();
    hasher.update(b"run-observation-v4\0");
    hasher.update(serde_json::to_vec(&v4).unwrap());
    assert_eq!(digest, hex::encode(hasher.finalize()));
    // Earlier versions are unchanged by the presence of the new one.
    assert_eq!(v1.schema_version, 1);
    assert_eq!(v3.observation.summary.schema_version, 3);
}

#[test]
fn repeated_reads_are_byte_identical_and_do_not_mutate_the_store() {
    let f = success_store();
    let before: (i64, String, String) = f
        .db()
        .query_row(
            "SELECT (SELECT count(*) FROM events),(SELECT event_hash FROM events ORDER BY sequence DESC LIMIT 1),(SELECT summary FROM runs)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let first = serde_json::to_vec(&observe_run_v4(&f.root, PRIVATE).unwrap()).unwrap();
    let second = serde_json::to_vec(&observe_run_v4(&f.root, PRIVATE).unwrap()).unwrap();
    assert_eq!(first, second);
    let after: (i64, String, String) = f
        .db()
        .query_row(
            "SELECT (SELECT count(*) FROM events),(SELECT event_hash FROM events ORDER BY sequence DESC LIMIT 1),(SELECT summary FROM runs)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn observing_while_an_owner_is_open_neither_recovers_nor_releases_it() {
    let f = success_store();
    let owner = Runtime::open(&f.root, RuntimeLimits::default()).unwrap();
    let names = |root: &PathBuf| {
        let mut names: Vec<_> = fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| {
                let n = n.to_string_lossy();
                !n.ends_with("-wal") && !n.ends_with("-shm")
            })
            .collect();
        names.sort();
        names
    };
    let before = names(&f.root);
    let sequence = observe_run_v4(&f.root, PRIVATE)
        .unwrap()
        .observation
        .observation
        .summary
        .horizon
        .sequence;
    assert_eq!(names(&f.root), before);
    assert_eq!(
        observe_run_v4(&f.root, PRIVATE)
            .unwrap()
            .observation
            .observation
            .summary
            .horizon
            .sequence,
        sequence
    );
    drop(owner);
}

#[test]
fn corrupt_journal_missing_store_and_missing_run_are_refused_without_leaks() {
    let f = success_store();
    let db = f.db();
    db.execute_batch("DROP TRIGGER events_no_update").unwrap();
    db.execute("UPDATE events SET detail='tampered' WHERE sequence=3", [])
        .unwrap();
    let error = observe_run_v4(&f.root, PRIVATE).unwrap_err();
    assert!(matches!(error, RuntimeError::Integrity(_)), "{error:?}");
    assert!(!format!("{error} {error:?}").contains(PRIVATE));

    let missing = tempfile::tempdir().unwrap();
    let root = missing.path().canonicalize().unwrap().join("absent");
    assert!(observe_run_v4(&root, PRIVATE).is_err());
    assert!(!root.exists());

    let g = success_store();
    assert!(matches!(
        observe_run_v4(&g.root, "no-such-run"),
        Err(RuntimeError::NotFound)
    ));
    assert!(matches!(
        observe_run_v4(&g.root, ""),
        Err(RuntimeError::Invalid(_))
    ));
}

#[test]
fn missing_attempt_evidence_leaves_requested_and_usage_unknown() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Failed, 1);
    approved_prefix(&tx);
    append(&tx, RunState::Preparing, "attempt.started", "");
    append(&tx, RunState::Failed, "transition", "crashed");
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["attempts"][0]["requested"], Value::Null);
    assert_eq!(t["attempts"][0]["usage"]["status"], "unknown");
    assert_eq!(t["attempts"][0]["result_after_cancel_request"], Value::Null);
}

#[test]
fn retry_returns_the_run_to_pending_approval_not_granted() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::AwaitingApproval, 1);
    approved_prefix(&tx);
    attempt(&tx, 1, RunState::Failed, None, None);
    append(
        &tx,
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["approval"]["outcome"], "pending");
    assert_eq!(t["approval"]["awaiting_sequence"], 10);
    assert_eq!(t["approval"]["granted_sequence"], Value::Null);
}

#[test]
fn a_retry_that_is_approved_again_reports_the_latest_grant() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::Preparing, 1);
    approved_prefix(&tx);
    attempt(&tx, 1, RunState::Failed, None, None);
    append(
        &tx,
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    append(&tx, RunState::Queued, "transition", "approved");
    append(&tx, RunState::Preparing, "transition", "preparing");
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["approval"]["outcome"], "granted");
    assert_eq!(t["approval"]["awaiting_sequence"], 10);
    assert_eq!(t["approval"]["granted_sequence"], 11);
}

#[test]
fn redaction_holds_for_the_whole_serialized_report() {
    let needles = [
        PRIVATE,
        "decoded_not_verified",
        "independent_verification_required",
        "987654321",
        "stream_reader_deadline",
        "authority_retained",
        "transport_observed",
        "prepared",
        "reviewed",
    ];
    for f in [success_store(), incomplete_store()] {
        let text = serde_json::to_string(&observe_run_v4(&f.root, PRIVATE).unwrap()).unwrap();
        for needle in needles {
            assert!(!text.contains(needle), "leaked {needle}");
        }
    }
}

#[test]
fn a_cancel_request_does_not_leak_into_the_next_approval_cycle() {
    let f = Fixture::new();
    let mut db = f.db();
    let tx = db.transaction().unwrap();
    insert_run(&tx, RunState::SucceededTransport, 2);
    approved_prefix(&tx);
    // Attempt 1: cancelled while running, result recorded after the request.
    let start = append(&tx, RunState::Preparing, "attempt.started", "");
    append(&tx, RunState::Running, "transition", "prepared");
    append(&tx, RunState::Running, "control.cancel_requested", "");
    let finish = append(&tx, RunState::Running, "attempt.finished", "observed");
    let transition = append(&tx, RunState::Failed, "transition", "transport_observed");
    append(
        &tx,
        RunState::Failed,
        "attempt.evidence.v1",
        &evidence(
            1,
            RunState::Failed,
            &start,
            &finish,
            &transition,
            None,
            None,
        ),
    );
    // Retry clears the control request; attempt 2 runs normally.
    append(
        &tx,
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    append(&tx, RunState::Queued, "transition", "approved");
    append(&tx, RunState::Preparing, "transition", "preparing");
    attempt(&tx, 2, RunState::SucceededTransport, None, None);
    tx.commit().unwrap();
    let t = f.timeline();
    assert_eq!(t["attempts"][0]["result_after_cancel_request"], true);
    assert_eq!(t["attempts"][1]["result_after_cancel_request"], false);
    assert_eq!(t["cancellation"]["requested_sequence"], Value::Null);
    assert_eq!(t["cancellation"]["events_after_request"], 0);
    let flagged: Vec<u64> = t["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["after_cancel_request"] == true)
        .map(|e| e["sequence"].as_u64().unwrap())
        .collect();
    // Only attempt 1's own post-request events (finish, transition, evidence).
    assert_eq!(flagged, [8, 9, 10]);
}
