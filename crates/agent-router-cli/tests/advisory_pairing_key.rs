// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `advisory-record` stores the `request_hash` of the saved advice (ADR 0056):
//! every arm of one comparison gets the hash of `compare.json`, a single advice
//! gets the hash it saved, and an export without one records it as UNKNOWN.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NOW: &str = "1000";

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
fn parsed(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| panic!("expected one JSON: {error}"))
}
fn read(path: &Path) -> Value {
    parsed(&fs::read(path).unwrap())
}
fn os(value: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(value)
}

/// Writes the task, host and risk files and returns the `advise` arguments.
fn advise(dir: &Path, extra: &[&std::ffi::OsStr]) -> Command {
    let models = ["gpt-5.6-sol", "gpt-5.6-terra"];
    let efforts: serde_json::Map<String, Value> = models
        .iter()
        .map(|model| ((*model).to_owned(), json!(["medium", "high", "xhigh"])))
        .collect();
    let task = write(
        &dir.join("task.json"),
        &json!({
            "objective": "Add a dry-run flag to the backup script and document it",
            "kind": "implement",
            "deliverables": ["Patch with the flag and its usage text"],
            "constraints": ["Keep the current default behavior"],
            "scope": ["scripts", "docs"],
            "risk": "medium",
            "sensitivity": "internal",
            "budget": {"max_cost_microusd": 0, "max_elapsed_ms": 600000, "max_tokens": 200000},
            "allowed_mutations": ["workspace_write"],
            "dependencies": [],
            "acceptance": ["The dry-run flag prints planned copies without writing"],
            "required_capabilities": ["tools"]
        }),
    );
    let host = write(
        &dir.join("host.json"),
        &json!({"id": "owner-codex", "harness": "codex", "supported_models": models,
            "supported_efforts": efforts, "capabilities": ["tools"]}),
    );
    let risk = write(
        &dir.join("risk.json"),
        &json!({"schema_version": 1, "environment": "local",
            "reversibility": "reversible", "operations": ["workspace_change"]}),
    );
    let mut cmd = command();
    cmd.args(["advise", "--task"])
        .arg(task)
        .arg("--risk-context")
        .arg(risk)
        .arg("--catalog")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .arg("--host")
        .arg(host)
        .args(["--now", NOW, "--json"])
        .args(extra);
    cmd
}

fn saved(dir: &Path, name: &str, compare: bool) -> PathBuf {
    let out = dir.join(name);
    let mut args = vec![os("--output-directory"), out.as_os_str()];
    if compare {
        args.push(os("--compare-policies"));
    }
    advise(dir, &args).assert().code(0);
    out
}

fn feedback(dir: &Path, subcommand: &str, extra: &[&std::ffi::OsStr]) -> Command {
    let mut cmd = command();
    cmd.args(["feedback", "--store"])
        .arg(dir.join("feedback"))
        .arg(subcommand)
        .args(extra)
        .args(["--now", NOW, "--json"]);
    cmd
}
fn consent(dir: &Path) {
    let preferences = write(
        &dir.join("preferences.json"),
        &json!({"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":86400}),
    );
    command()
        .args(["feedback", "--store"])
        .arg(dir.join("feedback"))
        .args(["configure", "--preferences"])
        .arg(preferences)
        .args(["--now", "900", "--json"])
        .assert()
        .success();
}
fn error_line(code: &str) -> String {
    format!("{{\"schema_version\":1,\"error\":\"{code}\"}}\n")
}
fn record(dir: &Path, advice: &Path, extra: &[&str]) -> Value {
    let mut args = vec![os("--advice"), advice.as_os_str()];
    args.extend(extra.iter().map(|value| os(value)));
    parsed(
        &feedback(dir, "advisory-record", &args)
            .assert()
            .success()
            .get_output()
            .stdout,
    )
}

fn temp() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(temp.path()).unwrap();
    (temp, dir)
}

#[test]
fn every_arm_of_a_comparison_records_the_same_request_hash() {
    let (_t, dir) = temp();
    consent(&dir);
    let out = saved(&dir, "both", true);
    let expected = read(&out.join("compare.json"))["request_hash"].clone();
    assert!(expected.is_string());
    // Each saved decision carries the same hash as the comparison.
    for policy in ["stable-v1", "stable-v2"] {
        assert_eq!(
            read(&out.join(policy).join("advice.json"))["request_hash"],
            expected
        );
    }
    let v2 = record(
        &dir,
        &out,
        &[
            "--policy",
            "stable-v2",
            "--followed",
            "--result",
            "accepted-as-is",
        ],
    );
    let v1 = record(
        &dir,
        &out,
        &[
            "--policy",
            "stable-v1",
            "--overridden",
            "--used-model",
            "gpt-5.6-terra",
            "--result",
            "accepted-after-rework",
        ],
    );
    assert_eq!(v1["input"]["binding"]["request_hash"], expected);
    assert_eq!(v2["input"]["binding"]["request_hash"], expected);
    assert_ne!(
        v1["input"]["binding"]["decision_id"],
        v2["input"]["binding"]["decision_id"]
    );

    // The report counts them as one trial, one arm per policy.
    let report = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(report["schema_version"], 4);
    let paired = &report["paired_trials"];
    assert_eq!(paired["trials"], 1);
    assert_eq!(paired["unknown_request_hash"], 0);
    let arms = paired["arms"].as_array().unwrap();
    let policies: Vec<_> = arms.iter().map(|arm| &arm["policy_version"]).collect();
    assert_eq!(policies, [&json!("stable-v1"), &json!("stable-v2")]);
    assert!(arms.iter().all(|arm| arm["trials"] == 1));
    assert_eq!(arms[0]["overridden"]["sample_size"], 1);
    assert_eq!(arms[1]["followed"]["sample_size"], 1);
}

#[test]
fn an_arm_whose_advice_names_another_request_is_refused() {
    let (_t, dir) = temp();
    consent(&dir);
    let out = saved(&dir, "both", true);
    let advice = out.join("stable-v1/advice.json");
    let mut document = read(&advice);
    document["request_hash"] = json!("f".repeat(64));
    write(&advice, &document);
    feedback(
        &dir,
        "advisory-record",
        &[
            os("--advice"),
            out.as_os_str(),
            os("--policy"),
            os("stable-v1"),
            os("--followed"),
            os("--result"),
            os("accepted-as-is"),
        ],
    )
    .assert()
    .failure()
    .stderr(error_line("advisory.outcome.advice_invalid"));
    // The untouched arm still records, with the comparison's hash.
    let v2 = record(
        &dir,
        &out,
        &[
            "--policy",
            "stable-v2",
            "--followed",
            "--result",
            "accepted-as-is",
        ],
    );
    assert_eq!(
        v2["input"]["binding"]["request_hash"],
        read(&out.join("compare.json"))["request_hash"]
    );
}

#[test]
fn a_single_advice_records_the_request_hash_it_saved() {
    let (_t, dir) = temp();
    consent(&dir);
    let out = saved(&dir, "single", false);
    let advice = read(&out.join("advice.json"));
    let hash = advice["request_hash"].as_str().unwrap().to_owned();
    assert_eq!(hash.len(), 64);
    let recorded = record(&dir, &out, &["--followed", "--result", "accepted-as-is"]);
    assert_eq!(recorded["input"]["binding"]["request_hash"], hash);

    // The same request compared gives the same hash: a single advice and a
    // comparison of one request pair up.
    let both = saved(&dir, "both", true);
    assert_eq!(read(&both.join("compare.json"))["request_hash"], hash);
}

#[test]
fn an_export_without_a_request_hash_records_it_as_unknown() {
    let (_t, dir) = temp();
    consent(&dir);
    let out = saved(&dir, "single", false);
    let path = out.join("advice.json");
    let mut advice = read(&path);
    advice.as_object_mut().unwrap().remove("request_hash");
    write(&path, &advice);
    let recorded = record(&dir, &out, &["--followed", "--result", "accepted-as-is"]);
    assert!(recorded["input"]["binding"].get("request_hash").is_none());
    let report = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(report["paired_trials"]["unknown_request_hash"], 1);
    // A malformed hash is not UNKNOWN: it is refused.
    advice["request_hash"] = json!("not-a-digest");
    write(&path, &advice);
    feedback(
        &dir,
        "advisory-record",
        &[
            os("--advice"),
            out.as_os_str(),
            os("--followed"),
            os("--result"),
            os("abandoned"),
        ],
    )
    .assert()
    .failure()
    .stderr(error_line("advisory.outcome.advice_invalid"));
}
