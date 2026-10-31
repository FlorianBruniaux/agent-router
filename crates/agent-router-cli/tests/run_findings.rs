//! `observe --schema-version 5`: advisory findings agree between the human and
//! JSON views, are stable, redacted and never change the store.
#[path = "support/run_store.rs"]
mod run_store;

use assert_cmd::Command;
use router_types::RunState;
use run_store::{PRIVATE, Store};
use rusqlite::Connection;
use serde_json::Value;

/// Everything the JSON says about findings must be readable in the human view.
fn assert_agrees(report: &Value, human: &str) {
    let findings = &report["findings"];
    assert_eq!(report["schema_version"], 5);
    assert!(human.contains(report["snapshot_sha256"].as_str().unwrap()));
    assert!(
        human.contains("advisory only, no automatic action"),
        "{human}"
    );
    assert!(human.contains(findings["source_snapshot_sha256"].as_str().unwrap()));
    for finding in findings["findings"].as_array().unwrap() {
        for needle in [
            format!(
                "[{}] {} id {}",
                finding["severity"].as_str().unwrap(),
                finding["rule"].as_str().unwrap(),
                finding["id"].as_str().unwrap()
            ),
            format!("Fact: {}", finding["fact"].as_str().unwrap()),
            format!("Hypothesis: {}", finding["hypothesis"].as_str().unwrap()),
            format!("Check: {}", finding["check"].as_str().unwrap()),
            format!(
                "Preserve: {}",
                finding["preserve"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ] {
            assert!(human.contains(&needle), "missing `{needle}` in\n{human}");
        }
        for evidence in finding["evidence"].as_array().unwrap() {
            assert!(
                human.contains(&format!("event {}", evidence["sequence"])),
                "{human}"
            );
        }
    }
    if findings["findings"].as_array().unwrap().is_empty() {
        assert!(human.contains("Findings: none"), "{human}");
    }
    for suppressed in findings["suppressed"].as_array().unwrap() {
        let line = format!(
            "  {}: {}",
            suppressed["rule"].as_str().unwrap(),
            suppressed["reason"].as_str().unwrap()
        );
        assert!(
            human.lines().any(|l| l == line),
            "missing `{line}` in\n{human}"
        );
    }
}

fn rules(report: &Value) -> Vec<&str> {
    report["findings"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect()
}

#[test]
fn defective_retry_and_unconfirmed_cleanup_agree_in_both_views() {
    let s = Store::new(RunState::Failed, 2);
    s.approve();
    s.attempt_transport(RunState::Failed, false, None, None, ("exited", Some(1)));
    s.append(
        RunState::AwaitingApproval,
        "retry.awaiting_fresh_approval",
        "",
    );
    s.append(RunState::Queued, "transition", "approved");
    s.append(RunState::Preparing, "transition", "preparing");
    s.attempt_transport(RunState::Failed, false, None, None, ("timed_out", None));
    let (report, human) = s.both_version("5");
    assert_agrees(&report, &human);
    assert_eq!(
        rules(&report),
        ["repeated_failed_attempts", "cleanup_unconfirmed"]
    );
    assert!(human.contains("Attempts 1, 2"), "{human}");
}

#[test]
fn healthy_failure_free_and_interrupted_runs_explain_what_they_do_not_say() {
    let ok = Store::new(RunState::SucceededTransport, 1);
    ok.approve();
    ok.attempt(RunState::SucceededTransport, false, None, None);
    let (report, human) = ok.both_version("5");
    assert_agrees(&report, &human);
    assert!(!rules(&report).contains(&"repeated_failed_attempts"));

    // Started, never finished, run left in Failed: missing evidence, not a retry defect.
    let broken = Store::new(RunState::Failed, 1);
    broken.approve();
    broken.append(RunState::Preparing, "attempt.started", "");
    broken.append(RunState::Failed, "transition", "crashed");
    let (report, human) = broken.both_version("5");
    assert_agrees(&report, &human);
    assert!(rules(&report).contains(&"attempt_evidence_missing"));
    assert!(!rules(&report).contains(&"repeated_failed_attempts"));
}

#[test]
fn late_result_after_cancel_is_reported_in_both_views() {
    let s = Store::new(RunState::Cancelled, 1);
    s.approve();
    s.attempt(RunState::Cancelled, true, None, None);
    let (report, human) = s.both_version("5");
    assert_agrees(&report, &human);
    assert!(rules(&report).contains(&"late_result_after_cancel"));
}

#[test]
fn repeated_rendering_is_identical_and_the_store_is_untouched() {
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt_transport(RunState::Failed, false, None, None, ("exited", Some(3)));
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
    let human = s
        .observe_version(false, "5")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = s
        .observe_version(true, "5")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    for _ in 0..3 {
        assert_eq!(
            human,
            s.observe_version(false, "5")
                .assert()
                .success()
                .get_output()
                .stdout
        );
        assert_eq!(
            json,
            s.observe_version(true, "5")
                .assert()
                .success()
                .get_output()
                .stdout
        );
    }
    assert_eq!(before, snapshot());
}

#[test]
fn corrupt_journal_produces_no_findings() {
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt_transport(RunState::Failed, false, None, None, ("exited", Some(3)));
    let db = Connection::open(s.root.join("runtime.sqlite3")).unwrap();
    db.execute_batch("DROP TRIGGER events_no_update; UPDATE events SET detail='observation-secret-canary' WHERE sequence=2;").unwrap();
    for json in [true, false] {
        let result = s
            .observe_version(json, "5")
            .assert()
            .code(7)
            .get_output()
            .clone();
        assert!(result.stdout.is_empty());
        assert!(
            !String::from_utf8(result.stderr)
                .unwrap()
                .contains("observation-secret-canary")
        );
    }
}

#[test]
fn earlier_versions_carry_no_findings_and_version_six_is_refused() {
    let s = Store::new(RunState::Failed, 1);
    s.approve();
    s.attempt_transport(RunState::Failed, false, None, None, ("exited", Some(3)));
    for version in ["1", "2", "3", "4"] {
        let bytes = s
            .observe_version(true, version)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("findings").is_none(), "v{version}");
    }
    s.observe_version(true, "6").assert().code(2);
    let _ = (PRIVATE, Command::new("true"));
}
