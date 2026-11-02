// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `advise --compare-policies`: both policies decide the same request, each
//! decision is saved under its own name, and `feedback advisory-record` binds an
//! outcome to the decision the operator followed. Nothing is executed.
#[path = "support/advisory.rs"]
mod advisory;
use advisory::{command, error_line, os, root, write};
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const NOW: &str = "1000";

fn parsed(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| panic!("expected one JSON: {error}"))
}
fn read(path: &Path) -> Value {
    parsed(&fs::read(path).unwrap())
}

struct Fixture {
    task: PathBuf,
    host: PathBuf,
    risk: PathBuf,
}

fn fixture(dir: &Path) -> Fixture {
    let models = ["gpt-5.6-sol", "gpt-5.6-terra"];
    let efforts: serde_json::Map<String, Value> = models
        .iter()
        .map(|model| ((*model).to_owned(), json!(["medium", "high", "xhigh"])))
        .collect();
    Fixture {
        task: write(
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
        ),
        host: write(
            &dir.join("host.json"),
            &json!({"id": "owner-codex", "harness": "codex", "supported_models": models,
                "supported_efforts": efforts, "capabilities": ["tools"]}),
        ),
        risk: write(
            &dir.join("risk.json"),
            &json!({"schema_version": 1, "environment": "local",
                "reversibility": "reversible", "operations": ["workspace_change"]}),
        ),
    }
}

fn advise(fixture: &Fixture, extra: &[&std::ffi::OsStr]) -> Command {
    let mut cmd = command();
    cmd.args(["advise", "--task"])
        .arg(&fixture.task)
        .arg("--risk-context")
        .arg(&fixture.risk)
        .arg("--catalog")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .arg("--host")
        .arg(&fixture.host)
        .args(["--now", NOW])
        .args(extra);
    cmd
}

fn compare_to(dir: &Path, name: &str) -> (PathBuf, Value) {
    let fixture = fixture(dir);
    let out = dir.join(name);
    let stdout = advise(
        &fixture,
        &[
            os("--compare-policies"),
            os("--json"),
            os("--output-directory"),
            out.as_os_str(),
        ],
    )
    .assert()
    .code(0)
    .get_output()
    .stdout
    .clone();
    (out, parsed(&stdout))
}

#[test]
fn one_task_saves_two_decisions_bound_to_one_request_hash() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let (out, printed) = compare_to(&dir, "both");

    let v1 = read(&out.join("stable-v1/advice.json"));
    let v2 = read(&out.join("stable-v2/advice.json"));
    let id1 = v1["decision"]["decision_id"].as_str().unwrap();
    let id2 = v2["decision"]["decision_id"].as_str().unwrap();
    assert_ne!(id1, id2);
    assert_eq!(v1["decision"]["policy_version"], "stable-v1");
    assert_eq!(v2["decision"]["policy_version"], "stable-v2");
    // Every saved file of one decision lives next to its own advice.json.
    for arm in ["stable-v1", "stable-v2"] {
        for file in [
            "task.json",
            "route-request.json",
            "decision.json",
            "advice.json",
        ] {
            assert!(out.join(arm).join(file).is_file(), "{arm}/{file}");
        }
    }
    // Both decisions came from the same route request bytes.
    assert_eq!(
        fs::read(out.join("stable-v1/route-request.json")).unwrap(),
        fs::read(out.join("stable-v2/route-request.json")).unwrap()
    );

    let saved = read(&out.join("compare.json"));
    assert_eq!(saved["request_hash"], printed["request_hash"]);
    let hash = saved["request_hash"].as_str().unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(saved["mode"], "advisory_comparison");
    assert_eq!(saved["executed"], false);
    let arms = saved["arms"].as_array().unwrap();
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0]["policy"], "stable-v1");
    assert_eq!(arms[0]["decision_id"], id1);
    assert_eq!(arms[0]["directory"], "stable-v1");
    assert_eq!(arms[1]["policy"], "stable-v2");
    assert_eq!(arms[1]["decision_id"], id2);
    // Per stage: model and effort of each arm, and whether they differ.
    let stages = saved["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 3);
    for stage in stages {
        let rows = stage["arms"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let same = rows[0]["model"] == rows[1]["model"] && rows[0]["effort"] == rows[1]["effort"];
        assert_eq!(stage["differs"], !same);
    }
}

#[test]
fn each_saved_arm_equals_the_single_policy_advice() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let (out, _) = compare_to(&dir, "both");
    let fixture = fixture(&dir);
    for policy in ["stable-v1", "stable-v2"] {
        let single = dir.join(format!("single-{policy}"));
        advise(
            &fixture,
            &[
                os("--policy"),
                os(policy),
                os("--json"),
                os("--output-directory"),
                single.as_os_str(),
            ],
        )
        .assert()
        .code(0);
        assert_eq!(
            fs::read(single.join("decision.json")).unwrap(),
            fs::read(out.join(policy).join("decision.json")).unwrap(),
            "{policy}"
        );
    }
}

#[test]
fn human_view_lists_each_stage_for_both_policies() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let fixture = fixture(&dir);
    let stdout = advise(&fixture, &[os("--compare-policies")])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(stdout).unwrap();
    assert!(text.starts_with("Advisory route comparison: nothing was executed\n"));
    for stage in ["plan", "execute", "verify"] {
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{stage}:")))
            .unwrap_or_else(|| panic!("no {stage} line in:\n{text}"));
        assert!(line.contains("stable-v1 "), "{line}");
        assert!(line.contains("stable-v2 "), "{line}");
        assert!(
            line.ends_with("same") || line.ends_with("differs"),
            "{line}"
        );
    }
}

#[test]
fn output_is_byte_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let fixture = fixture(&dir);
    for flags in [
        &["--compare-policies"][..],
        &["--compare-policies", "--json"],
    ] {
        let args: Vec<_> = flags.iter().map(|flag| os(flag)).collect();
        let first = advise(&fixture, &args)
            .assert()
            .code(0)
            .get_output()
            .stdout
            .clone();
        let second = advise(&fixture, &args)
            .assert()
            .code(0)
            .get_output()
            .stdout
            .clone();
        assert_eq!(first, second, "{flags:?}");
    }
    let (first, _) = compare_to(&dir, "first");
    let (second, _) = compare_to(&dir, "second");
    for file in [
        "compare.json",
        "stable-v1/decision.json",
        "stable-v2/decision.json",
        "stable-v1/route-request.json",
    ] {
        let normalize = |root: &Path| {
            String::from_utf8(fs::read(root.join(file)).unwrap())
                .unwrap()
                .replace(root.to_str().unwrap(), "<out>")
        };
        assert_eq!(normalize(&first), normalize(&second), "{file}");
    }
}

#[test]
fn compare_policies_refuses_an_explicit_policy() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let fixture = fixture(&dir);
    advise(
        &fixture,
        &[os("--compare-policies"), os("--policy"), os("stable-v1")],
    )
    .assert()
    .code(2);
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

#[test]
fn recording_followed_v2_binds_to_the_v2_decision() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir);
    let (out, _) = compare_to(&dir, "both");
    let id1 = read(&out.join("stable-v1/advice.json"))["decision"]["decision_id"].clone();
    let id2 = read(&out.join("stable-v2/advice.json"))["decision"]["decision_id"].clone();

    let recorded = parsed(
        &feedback(
            &dir,
            "advisory-record",
            &[
                os("--advice"),
                out.as_os_str(),
                os("--policy"),
                os("stable-v2"),
                os("--followed"),
                os("--result"),
                os("accepted-as-is"),
            ],
        )
        .assert()
        .success()
        .get_output()
        .stdout,
    );
    assert_eq!(recorded["input"]["binding"]["decision_id"], id2);
    assert_eq!(recorded["input"]["binding"]["policy_version"], "stable-v2");
    assert_eq!(recorded["input"]["adherence"], "followed");
    assert_ne!(recorded["input"]["binding"]["decision_id"], id1);

    // Neither plan followed: the outcome names the plan it departs from and
    // carries the used fields.
    let other = parsed(
        &feedback(
            &dir,
            "advisory-record",
            &[
                os("--advice"),
                out.as_os_str(),
                os("--policy"),
                os("stable-v1"),
                os("--overridden"),
                os("--used-model"),
                os("gpt-5.6-terra"),
                os("--result"),
                os("accepted-after-rework"),
            ],
        )
        .assert()
        .success()
        .get_output()
        .stdout,
    );
    assert_eq!(other["input"]["binding"]["decision_id"], id1);
    assert_eq!(other["input"]["adherence"], "overridden");
    assert_eq!(other["input"]["used"]["model"], "gpt-5.6-terra");
}

#[test]
fn a_comparison_directory_needs_a_named_policy() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir);
    let (out, _) = compare_to(&dir, "both");
    let record = |extra: &[&str]| {
        let mut args = vec![os("--advice"), out.as_os_str()];
        args.extend(extra.iter().map(|value| os(value)));
        args.extend([os("--followed"), os("--result"), os("accepted-as-is")]);
        feedback(&dir, "advisory-record", &args).assert()
    };
    record(&[])
        .failure()
        .stderr(error_line("advisory.outcome.policy_required"));
    record(&["--policy", "stable-v9"])
        .failure()
        .stderr(error_line("advisory.outcome.policy_unknown"));
    record(&["--policy", "../stable-v2"])
        .failure()
        .stderr(error_line("advisory.outcome.policy_unknown"));
    // A plain advice has no arms to choose from.
    feedback(
        &dir,
        "advisory-record",
        &[
            os("--advice"),
            out.join("stable-v2").as_os_str(),
            os("--policy"),
            os("stable-v2"),
            os("--followed"),
            os("--result"),
            os("accepted-as-is"),
        ],
    )
    .assert()
    .failure()
    .stderr(error_line("advisory.outcome.policy_unexpected"));
}

#[test]
fn a_tampered_comparison_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir);
    let (out, _) = compare_to(&dir, "both");
    let marker = out.join("compare.json");
    let original = read(&marker);
    let attempt = |edit: &dyn Fn(&mut Value)| {
        let mut document = original.clone();
        edit(&mut document);
        fs::write(&marker, serde_json::to_vec(&document).unwrap()).unwrap();
        feedback(
            &dir,
            "advisory-record",
            &[
                os("--advice"),
                out.as_os_str(),
                os("--policy"),
                os("stable-v2"),
                os("--followed"),
                os("--result"),
                os("accepted-as-is"),
            ],
        )
        .assert()
        .failure()
        .stderr(error_line("advisory.outcome.advice_invalid"));
    };
    attempt(&|doc| doc["arms"][1]["directory"] = json!("../stable-v1"));
    attempt(&|doc| doc["arms"][1]["decision_id"] = json!("0".repeat(64)));
    attempt(&|doc| doc["mode"] = json!("advisory"));
}

#[test]
fn a_high_risk_task_differs_only_where_the_policies_pick_differently() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let fixture = fixture(&dir);
    let stdout = command()
        .args([
            "advise",
            "--text",
            "Add a dry-run flag",
            "--kind",
            "implement",
            "--risk",
            "high",
            "--budget",
            "0:600000:200000",
            "--scope",
            "scripts",
            "--now",
            NOW,
            "--compare-policies",
            "--json",
            "--catalog",
        ])
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .arg("--host")
        .arg(&fixture.host)
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let comparison = parsed(&stdout);
    let flags: Vec<_> = comparison["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| {
            (
                stage["stage"].as_str().unwrap(),
                stage["differs"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        flags,
        [("plan", false), ("execute", false), ("verify", true)]
    );
    let verify = &comparison["stages"][2]["arms"];
    assert_eq!(verify[0]["model"], "gpt-5.6-terra");
    assert_eq!(verify[1]["model"], "gpt-5.6-sol");
}
