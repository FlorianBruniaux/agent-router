//! `observe --schema-version 4` over synthetic stores: human and JSON agree,
//! output is bounded and redacted, and observation never changes the store.
use assert_cmd::Command;
use router_runtime::{Runtime, RuntimeLimits};
use router_types::{OutputRetentionPolicy, RunState, RunSummary};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const PRIVATE: &str = "TIMELINE_PRIVATE_PROMPT_PATH_PROVIDER_CANARY";
const MODEL: &str = "vendor/model-v4:2026_10.28";

struct Store {
    _temp: tempfile::TempDir,
    root: PathBuf,
}
impl Store {
    fn new(state: RunState, attempts: u32) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("runtime");
        drop(Runtime::open(&root, RuntimeLimits::default()).unwrap());
        let db = Connection::open(root.join("runtime.sqlite3")).unwrap();
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
        }
        Self { _temp: temp, root }
    }
    fn append(&self, state: RunState, kind: &str, detail: &str) -> String {
        let db = Connection::open(self.root.join("runtime.sqlite3")).unwrap();
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
    fn approve(&self) {
        self.append(RunState::Planned, "planned", "");
        self.append(RunState::AwaitingApproval, "transition", "reviewed");
        self.append(RunState::Queued, "transition", "approved");
        self.append(RunState::Preparing, "transition", "preparing");
    }
    fn attempt(&self, end: RunState, cancel_between: bool, tokens: Option<u64>, cost: Option<u64>) {
        let start = self.append(RunState::Preparing, "attempt.started", "");
        self.append(RunState::Running, "transition", "prepared");
        if cancel_between {
            self.append(RunState::Running, "control.cancel_requested", "");
        }
        let finish = self.append(RunState::Running, "attempt.finished", "observed");
        let transition = self.append(end, "transition", "transport_observed");
        let evidence = json!({
            "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":1},
            "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
            "decision_origin":"deterministic", "sandbox":"read_only",
            "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
            "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
            "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
            "state":end, "failure":null,
            "transport":{"termination":"exited","exit_code":0,"elapsed_ms":987654321},
            "cleanup":"unknown", "observed_cost_microusd":cost,
            "observed_tokens":tokens, "started_event_hash":start,
            "finished_event_hash":finish, "transition_event_hash":transition
        });
        self.append(end, "attempt.evidence.v1", &evidence.to_string());
    }
    fn observe(&self, json: bool) -> Command {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
        cmd.args(["observe", "--store"]).arg(&self.root).args([
            "--run-id",
            PRIVATE,
            "--schema-version",
            "4",
        ]);
        if json {
            cmd.arg("--json");
        }
        cmd
    }
    /// Returns (json, human) after checking redaction of both outputs.
    fn both(&self) -> (Value, String) {
        let json = self
            .observe(true)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let human = self
            .observe(false)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let (json_text, human) = (
            String::from_utf8(json).unwrap(),
            String::from_utf8(human).unwrap(),
        );
        for text in [&json_text, &human] {
            for needle in [
                PRIVATE,
                self.root.to_str().unwrap(),
                "987654321",
                "reviewed",
                "transport_observed",
            ] {
                assert!(!text.contains(needle), "leaked {needle}:\n{text}");
            }
        }
        (serde_json::from_str(&json_text).unwrap(), human)
    }
}

/// Every fact the JSON carries for the timeline must be readable in the human view.
fn assert_agrees(report: &Value, human: &str) {
    let t = &report["timeline"];
    assert_eq!(report["schema_version"], 4);
    assert!(
        human.contains(report["snapshot_sha256"].as_str().unwrap()),
        "{human}"
    );
    for entry in t["entries"].as_array().unwrap() {
        let mut line = format!(
            "  {}: {} {} [{}]",
            entry["sequence"],
            entry["phase"].as_str().unwrap(),
            entry["kind"].as_str().unwrap(),
            entry["state"].as_str().unwrap()
        );
        if let Some(attempt) = entry["attempt"].as_u64() {
            line.push_str(&format!(" attempt {attempt}"));
        }
        if entry["after_cancel_request"] == true {
            line.push_str(" after cancel request");
        }
        assert!(
            human.lines().any(|l| l == line),
            "missing `{line}` in\n{human}"
        );
    }
    for needle in [
        format!("Approval: {}", t["approval"]["outcome"].as_str().unwrap()),
        format!(
            "Verification: {}",
            t["verification"]["state"].as_str().unwrap()
        ),
        format!("Showing {} of {} events", t["shown"], t["run_events"]),
        format!("State: {}", report["known"]["state"].as_str().unwrap()),
    ] {
        assert!(human.contains(&needle), "missing `{needle}` in\n{human}");
    }
    for phase in t["phases"].as_array().unwrap() {
        let summary = format!("{} {}", phase["phase"].as_str().unwrap(), phase["events"]);
        assert!(human.contains(&summary), "missing phase `{summary}`");
    }
    for attempt in t["attempts"].as_array().unwrap() {
        assert!(human.contains(&format!("Attempt {}", attempt["ordinal"])));
        if let Some(model) = attempt["requested"]["model"].as_str() {
            assert!(human.contains(&format!("requested {model}")), "{human}");
        }
        assert!(human.contains("applied unknown"), "{human}");
        assert!(human.contains(&format!(
            "usage {}",
            attempt["usage"]["status"].as_str().unwrap()
        )));
    }
    for field in [
        "exact_revision_acceptance",
        "notification_delivery",
        "lost_notifications",
        "event_timestamps",
        "physical_cleanup",
    ] {
        assert_eq!(t["unknown"][field], "unknown");
        assert!(
            human.contains(&field.replace('_', " ")),
            "missing unknown {field}"
        );
    }
}

#[test]
fn success_failure_denied_interrupted_and_missing_evidence_agree() {
    // success
    let s = Store::new(RunState::Verified, 1);
    s.approve();
    s.attempt(RunState::SucceededTransport, false, Some(10), Some(20));
    s.append(
        RunState::Verifying,
        "transition",
        "independent_verification_required",
    );
    s.append(RunState::Verifying, "verification.recorded", PRIVATE);
    s.append(RunState::Verified, "transition", "verified");
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["verification"]["state"], "verified");
    assert!(human.contains("tokens 10") && human.contains("cost 20 microusd"));

    // failure
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt(RunState::Failed, false, None, None);
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["verification"]["state"], "not_reached");

    // denied approval
    let s = Store::new(RunState::Cancelled, 0);
    s.append(RunState::Planned, "planned", "");
    s.append(RunState::AwaitingApproval, "transition", "reviewed");
    s.append(RunState::AwaitingApproval, "control.cancel_requested", "");
    s.append(RunState::Cancelled, "control.cancelled_before_start", "");
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["approval"]["outcome"], "not_granted");

    // interruption: started, never finished, store reopened later
    let s = Store::new(RunState::Running, 1);
    s.approve();
    s.append(RunState::Preparing, "attempt.started", "");
    s.append(RunState::Running, "transition", "prepared");
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["attempts"][0]["requested"], Value::Null);
    assert!(human.contains("requested unknown"));
    assert!(human.contains("result after cancel request unknown"));

    // missing evidence: fewer events than attempts would need
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.append(RunState::Preparing, "attempt.started", "");
    s.append(RunState::Failed, "transition", "crashed");
    let (report, human) = s.both();
    assert_agrees(&report, &human);
}

#[test]
fn late_result_after_cancel_and_partial_usage_are_visible_in_both_views() {
    let s = Store::new(RunState::Cancelled, 1);
    s.approve();
    s.attempt(RunState::Cancelled, true, Some(5), None);
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(
        report["timeline"]["attempts"][0]["usage"]["status"],
        "partial"
    );
    assert!(human.contains("tokens 5") && human.contains("cost unknown"));
    assert!(human.contains("result after cancel request yes"));
    assert!(human.contains("Cancellation: requested 7"));
    assert!(human.contains("logical only"));
    assert!(human.contains("physical shutdown unknown"));
}

#[test]
fn long_runs_are_truncated_with_an_explicit_omission_notice() {
    let s = Store::new(RunState::Preparing, 0);
    s.approve();
    for _ in 0..100 {
        s.append(RunState::Preparing, "preparation.git_returned", "");
    }
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["shown"], 64);
    assert_eq!(report["timeline"]["omitted_earlier"], 40);
    assert!(human.contains("40 earlier events omitted"));
    assert!(
        human.lines().count() < 120,
        "{} lines",
        human.lines().count()
    );
}

#[test]
fn repeated_rendering_is_identical_and_the_store_is_untouched() {
    let s = Store::new(RunState::Verified, 1);
    s.approve();
    s.attempt(RunState::SucceededTransport, false, Some(1), Some(2));
    s.append(
        RunState::Verifying,
        "transition",
        "independent_verification_required",
    );
    s.append(RunState::Verified, "transition", "verified");
    let snapshot = || {
        Connection::open(s.root.join("runtime.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT (SELECT count(*) FROM events),(SELECT group_concat(event_hash) FROM events),(SELECT summary FROM runs),(SELECT count(*) FROM control_requests)",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)),
            )
            .unwrap()
    };
    let before = snapshot();
    let first = s
        .observe(false)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = s
        .observe(true)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    for _ in 0..3 {
        assert_eq!(
            first,
            s.observe(false).assert().success().get_output().stdout
        );
        assert_eq!(json, s.observe(true).assert().success().get_output().stdout);
    }
    assert_eq!(before, snapshot());
}

#[test]
fn corrupt_and_missing_stores_are_refused_without_a_partial_timeline() {
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt(RunState::Failed, false, None, None);
    let db = Connection::open(s.root.join("runtime.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER events_no_update; UPDATE events SET detail='observation-secret-canary' WHERE sequence=2;").unwrap();
    for json in [true, false] {
        let result = s.observe(json).assert().code(7).get_output().clone();
        assert!(result.stdout.is_empty());
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(stderr.contains("runtime.integrity"));
        assert!(!stderr.contains("observation-secret-canary"));
    }
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    cmd.args(["observe", "--store"])
        .arg(&missing)
        .args(["--run-id", "x", "--schema-version", "4"]);
    assert!(cmd.assert().code(2).get_output().stdout.is_empty());
    assert!(!missing.exists());
}

#[test]
fn versions_one_to_three_are_not_affected_and_five_is_refused() {
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt(RunState::Failed, false, None, None);
    let read = |version: &str| {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
        cmd.args(["observe", "--store"]).arg(&s.root).args([
            "--run-id",
            PRIVATE,
            "--json",
            "--schema-version",
            version,
        ]);
        cmd.assert().success().get_output().stdout.clone()
    };
    for version in ["1", "2", "3"] {
        let value: Value = serde_json::from_slice(&read(version)).unwrap();
        assert_eq!(value["schema_version"].to_string(), version);
        assert!(value.get("timeline").is_none());
    }
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    cmd.args(["observe", "--store"]).arg(&s.root).args([
        "--run-id",
        PRIVATE,
        "--schema-version",
        "5",
        "--json",
    ]);
    cmd.assert().code(2);
}

#[test]
fn retry_awaiting_fresh_approval_is_pending_in_both_views() {
    let s = Store::new(RunState::AwaitingApproval, 1);
    s.approve();
    s.attempt(RunState::Failed, false, None, None);
    s.append(
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    let (report, human) = s.both();
    assert_agrees(&report, &human);
    assert_eq!(report["timeline"]["approval"]["outcome"], "pending");
    assert!(human.contains("Approval: pending (awaiting 10, granted none"));
}
