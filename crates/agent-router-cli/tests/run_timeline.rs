// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `observe --schema-version 4` over synthetic stores: human and JSON agree,
//! output is bounded and redacted, and observation never changes the store.
#[path = "support/run_store.rs"]
mod run_store;

use assert_cmd::Command;
use router_types::RunState;
use run_store::{PRIVATE, Store};
use rusqlite::Connection;
use serde_json::Value;

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
        "6",
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
