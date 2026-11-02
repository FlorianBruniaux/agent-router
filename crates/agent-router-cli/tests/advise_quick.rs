// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Inline `advise` entry and one-line `advisory-record`, through the binary.
//! Nothing is executed and no provider is contacted.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NOW: &str = "1000";
const OBJECTIVE: &str = "Add a dry-run flag to the backup script";
const ACCEPTANCE: &str = "The operator judges the result by hand.";

fn command() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.timeout(Duration::from_secs(20));
    command
}
fn root(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}
fn write(path: &Path, value: &Value) -> PathBuf {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_owned()
}
fn parsed(output: &[u8]) -> Value {
    serde_json::from_slice(output).unwrap_or_else(|error| panic!("one JSON result: {error}"))
}
fn host(dir: &Path) -> PathBuf {
    write(
        &dir.join("host.json"),
        &json!({"id": "owner-codex", "harness": "codex",
            "supported_models": ["gpt-5.6-sol", "gpt-5.6-terra"],
            "supported_efforts": {"gpt-5.6-sol": ["medium", "high", "xhigh"],
                "gpt-5.6-terra": ["medium", "high", "xhigh"]},
            "capabilities": []}),
    )
}

/// `advise` with the shared host and catalog; `extra` carries the task.
fn advise(dir: &Path, extra: &[&str]) -> Command {
    let mut cmd = command();
    cmd.arg("advise")
        .args(extra)
        .arg("--catalog")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .arg("--host")
        .arg(host(dir))
        .args(["--now", NOW, "--json"]);
    cmd
}
const MINIMAL: &[&str] = &[
    "--text",
    OBJECTIVE,
    "--kind",
    "implement",
    "--risk",
    "medium",
    "--scope",
    "scripts",
    "--budget",
    "0:600000:200000",
];

fn dir() -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    fs::canonicalize(dir.keep()).unwrap()
}
fn facts(report: &Value) -> Vec<(String, String)> {
    report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fact| {
            (
                fact["subject"].as_str().unwrap().to_owned(),
                fact["provenance"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn one_advise_command_and_one_record_command_work_end_to_end() {
    let dir = dir();
    let out = dir.join("advice");
    let store = dir.join("feedback");
    let prefs = write(
        &dir.join("prefs.json"),
        &json!({"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":86400}),
    );
    command()
        .args(["feedback", "--store"])
        .arg(&store)
        .args(["configure", "--preferences"])
        .arg(&prefs)
        .args(["--now", "900", "--json"])
        .assert()
        .success();

    let stdout = advise(&dir, MINIMAL)
        .arg("--output-directory")
        .arg(&out)
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let report = parsed(&stdout);
    assert_eq!(report["status"], "recommended");

    // The one-line record takes the advice directory and a shorthand.
    let recorded = command()
        .args(["feedback", "--store"])
        .arg(&store)
        .args(["advisory-record", "--advice"])
        .arg(&out)
        .args([
            "--followed",
            "--result",
            "accepted-as-is",
            "--now",
            NOW,
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recorded = parsed(&recorded);
    assert_eq!(recorded["input"]["adherence"], "followed");
    assert_eq!(
        recorded["input"]["binding"]["decision_id"],
        report["decision"]["decision_id"]
    );
}

#[test]
fn every_defaulted_fact_is_labelled_defaulted_and_given_ones_are_not() {
    let dir = dir();
    let report = parsed(&advise(&dir, MINIMAL).assert().code(0).get_output().stdout);
    let defaulted: Vec<String> = facts(&report)
        .into_iter()
        .filter(|(_, provenance)| provenance == "defaulted")
        .map(|(subject, _)| subject)
        .collect();
    assert_eq!(
        defaulted,
        [
            "task.sensitivity",
            "task.environment",
            "task.reversibility",
            "task.operations",
            "task.allowed_mutations",
            "task.acceptance"
        ]
    );
    // Risk, kind, scope and budget were stated: never a defaulted row.
    assert!(facts(&report).iter().all(|(subject, provenance)| {
        !["task.risk", "task.kind", "task.scope", "task.budget"].contains(&subject.as_str())
            || provenance != "defaulted"
    }));
}

fn file_report(dir: &Path) -> Vec<u8> {
    let task = write(
        &dir.join("task.json"),
        &json!({"objective": OBJECTIVE, "kind": "implement", "scope": ["scripts"],
            "risk": "medium", "sensitivity": "internal",
            "budget": {"max_cost_microusd": 0, "max_elapsed_ms": 600000, "max_tokens": 200000},
            "allowed_mutations": ["workspace_write"], "acceptance": [ACCEPTANCE]}),
    );
    let risk = write(
        &dir.join("risk.json"),
        &json!({"schema_version": 1, "environment": "local",
            "reversibility": "reversible", "operations": ["workspace_change"]}),
    );
    advise(
        dir,
        &[
            "--task",
            task.to_str().unwrap(),
            "--risk-context",
            risk.to_str().unwrap(),
        ],
    )
    .assert()
    .code(0)
    .get_output()
    .stdout
    .clone()
}

#[test]
fn explicit_flags_equal_to_the_defaults_match_the_file_path_byte_for_byte() {
    let dir = dir();
    let mut args = MINIMAL.to_vec();
    args.extend([
        "--sensitivity",
        "internal",
        "--environment",
        "local",
        "--reversibility",
        "reversible",
        "--operation",
        "workspace_change",
        "--mutation",
        "workspace_write",
        "--acceptance",
        ACCEPTANCE,
    ]);
    let quick = advise(&dir, &args)
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        String::from_utf8(quick).unwrap(),
        String::from_utf8(file_report(&dir)).unwrap()
    );
}

#[test]
fn defaults_keep_the_decision_of_the_file_path() {
    let dir = dir();
    let quick = parsed(&advise(&dir, MINIMAL).assert().code(0).get_output().stdout);
    let file = parsed(&file_report(&dir));
    assert_eq!(quick["decision"], file["decision"]);
    assert_eq!(quick["mapping"], file["mapping"]);
}

#[test]
fn a_missing_stated_fact_is_asked_never_defaulted() {
    let dir = dir();
    for (flag, question) in [
        ("--risk", "task.risk.unknown"),
        ("--budget", "task.budget.unknown"),
        ("--scope", "task.scope.missing"),
    ] {
        let mut args = Vec::new();
        let mut skip = false;
        for value in MINIMAL {
            if skip {
                skip = false;
            } else if *value == flag {
                skip = true;
            } else {
                args.push(*value);
            }
        }
        let report = parsed(&advise(&dir, &args).assert().code(3).get_output().stdout);
        assert_eq!(report["status"], "needs_clarification");
        let codes: Vec<_> = report["questions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| q["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, [question]);
    }
}

#[test]
fn critical_and_irreversible_are_only_taken_when_stated() {
    let dir = dir();
    let mut critical = MINIMAL.to_vec();
    let at = critical.iter().position(|v| *v == "medium").unwrap();
    critical[at] = "critical";
    let report = parsed(&advise(&dir, &critical).assert().code(0).get_output().stdout);
    let risk = report["mapping"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["field"] == "risk")
        .unwrap();
    assert_eq!(risk["value"]["declared"], "critical");
    let mut irreversible = MINIMAL.to_vec();
    irreversible.extend(["--reversibility", "irreversible"]);
    let report = parsed(
        &advise(&dir, &irreversible)
            .assert()
            .code(0)
            .get_output()
            .stdout,
    );
    let rows = facts(&report);
    // A stated fact is not a defaulted one.
    assert!(!rows.contains(&("task.reversibility".into(), "defaulted".into())));
    // An unstated reversibility is the reversible default, never irreversible.
    let plain = parsed(&advise(&dir, MINIMAL).assert().code(0).get_output().stdout);
    let row = plain["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["subject"] == "task.reversibility")
        .unwrap();
    assert_eq!(row["value"], "reversible");
}

#[test]
fn a_migration_never_defaults_its_environment_or_reversibility() {
    let dir = dir();
    let mut migration = MINIMAL.to_vec();
    let at = migration.iter().position(|v| *v == "implement").unwrap();
    migration[at] = "migration";
    let report = parsed(
        &advise(&dir, &migration)
            .assert()
            .code(3)
            .get_output()
            .stdout,
    );
    assert_eq!(report["status"], "needs_clarification");
    let codes: Vec<_> = report["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"risk.environment.unknown"), "{codes:?}");
    assert!(codes.contains(&"risk.reversibility.unknown"), "{codes:?}");
    migration.extend(["--environment", "local", "--reversibility", "irreversible"]);
    let report = parsed(
        &advise(&dir, &migration)
            .assert()
            .code(0)
            .get_output()
            .stdout,
    );
    let rows = facts(&report);
    assert!(!rows.contains(&("task.environment".into(), "defaulted".into())));
    assert!(!rows.contains(&("task.reversibility".into(), "defaulted".into())));
}

#[test]
fn inline_flags_and_the_task_file_are_mutually_exclusive() {
    let dir = dir();
    let task = write(&dir.join("t.json"), &json!({}));
    advise(
        &dir,
        &["--task", task.to_str().unwrap(), "--text", OBJECTIVE],
    )
    .assert()
    .code(2);
    advise(&dir, &["--text", OBJECTIVE]).assert().code(2);
    advise(&dir, &[]).assert().code(2);
}

#[test]
fn the_record_shorthand_and_the_long_form_cannot_be_mixed() {
    let dir = dir();
    let out = dir.join("advice");
    advise(&dir, MINIMAL)
        .arg("--output-directory")
        .arg(&out)
        .assert()
        .code(0);
    let store = dir.join("feedback");
    for extra in [
        vec!["--followed", "--overridden"],
        vec!["--followed", "--adherence", "followed"],
        vec![],
    ] {
        command()
            .args(["feedback", "--store"])
            .arg(&store)
            .args(["advisory-record", "--advice"])
            .arg(&out)
            .args(["--result", "accepted-as-is", "--now", NOW, "--json"])
            .args(extra)
            .assert()
            .code(2);
    }
}
