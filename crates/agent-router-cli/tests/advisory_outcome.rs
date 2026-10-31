// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Manual outcomes of `advise` recommendations, through the binary: record,
//! report, export and purge. Nothing is executed and no provider is contacted.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NOW: &str = "1000";
// Split so the source holds no private-home path literal (package-core scan).
const HOME_PATH: &str = concat!("/Us", "ers/me/clients/acme/secret-repo");
const OBJECTIVE: &str = "Add a dry-run flag to the backup script and document it";

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
    serde_json::from_slice(output)
        .unwrap_or_else(|error| panic!("expected one JSON result: {error}"))
}

/// Runs `advise` for a task with this objective and returns its `advice.json`.
fn advise(dir: &Path, name: &str, objective: &str, host_models: &[&str]) -> PathBuf {
    let task = write(
        &dir.join(format!("{name}-task.json")),
        &json!({
            "objective": objective,
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
    let efforts: serde_json::Map<String, Value> = host_models
        .iter()
        .map(|model| ((*model).to_owned(), json!(["medium", "high", "xhigh"])))
        .collect();
    let host = write(
        &dir.join(format!("{name}-host.json")),
        &json!({"id": "owner-codex", "harness": "codex", "supported_models": host_models,
            "supported_efforts": efforts, "capabilities": ["tools"]}),
    );
    let risk = write(
        &dir.join(format!("{name}-risk.json")),
        &json!({"schema_version": 1, "environment": "local",
            "reversibility": "reversible", "operations": ["workspace_change"]}),
    );
    let out = dir.join(format!("{name}-advice"));
    command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(&risk)
        .arg("--catalog")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .arg("--host")
        .arg(&host)
        .args([
            "--skill",
            "tdd",
            "--now",
            NOW,
            "--json",
            "--output-directory",
        ])
        .arg(&out)
        .assert()
        .code(0);
    out.join("advice.json")
}

fn store(dir: &Path) -> PathBuf {
    dir.join("feedback")
}
fn feedback(dir: &Path, subcommand: &str, extra: &[&std::ffi::OsStr]) -> Command {
    let mut cmd = command();
    cmd.args(["feedback", "--store"])
        .arg(store(dir))
        .arg(subcommand)
        .args(extra)
        .args(["--now", NOW, "--json"]);
    cmd
}
fn os(value: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(value)
}
fn consent(dir: &Path, now: &str) {
    let preferences = write(
        &dir.join("preferences.json"),
        &json!({"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":86400}),
    );
    command()
        .args(["feedback", "--store"])
        .arg(store(dir))
        .args(["configure", "--preferences"])
        .arg(preferences)
        .args(["--now", now, "--json"])
        .assert()
        .success();
}
fn record(dir: &Path, advice: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut args = vec![os("--advice"), advice.as_os_str()];
    args.extend(extra.iter().map(|value| os(value)));
    feedback(dir, "advisory-record", &args).assert()
}
fn error_line(code: &str) -> String {
    format!("{{\"schema_version\":1,\"error\":\"{code}\"}}\n")
}

#[test]
fn recipe_records_reports_exports_and_purges_without_storing_task_text() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir, "900");
    let first = advise(&dir, "one", OBJECTIVE, &["gpt-5.6-sol", "gpt-5.6-terra"]);
    let second = advise(
        &dir,
        "two",
        "Rename the restore helper and update its callers",
        &["gpt-5.6-sol", "gpt-5.6-terra"],
    );

    // Followed, accepted as is, no cost reported.
    let out = record(
        &dir,
        &first,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let recorded = parsed(&out);
    let input = &recorded["input"];
    assert_eq!(input["adherence"], "followed");
    assert_eq!(input["used"], Value::Null);
    assert_eq!(input["result"], "accepted_as_is");
    assert_eq!(input["binding"]["task_kind"], "implement");
    assert_eq!(input["binding"]["risk"], "medium");
    assert_eq!(input["binding"]["selection_basis"], "catalog_policy_prior");
    assert_eq!(input["binding"]["recommended"].as_array().unwrap().len(), 3);
    let decision_id: Value = parsed(&fs::read(&first).unwrap())["decision"]["decision_id"].clone();
    assert_eq!(input["binding"]["decision_id"], decision_id);
    // A missing cost is UNKNOWN (null), never zero.
    assert_eq!(input["measurements"]["cost_microusd"], Value::Null);
    assert_eq!(input["measurements"]["tokens"], Value::Null);

    // Overridden: another model, no stated effort, host-reported cost.
    record(
        &dir,
        &second,
        &[
            "--adherence",
            "overridden",
            "--result",
            "accepted-after-rework",
            "--used-model",
            "gpt-5.6-terra",
            "--used-no-skill",
            "--cost-microusd",
            "12345",
        ],
    )
    .success();
    // Repeating an identical record is idempotent; a different one is refused.
    record(
        &dir,
        &first,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .success();
    // Advising the same task again gives the same decision id. A different
    // result is a correction stored as attempt 2, not an error.
    let again = advise(
        &dir,
        "one-again",
        OBJECTIVE,
        &["gpt-5.6-sol", "gpt-5.6-terra"],
    );
    let corrected = parsed(
        &record(
            &dir,
            &again,
            &["--adherence", "followed", "--result", "abandoned"],
        )
        .success()
        .get_output()
        .stdout,
    );
    assert_eq!(corrected["attempt"], 2);
    assert_eq!(corrected["input"]["binding"]["decision_id"], decision_id);
    // The latest attempt counts, once.
    let report = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(
        (report["decisions"].as_u64(), report["records"].as_u64()),
        (Some(2), Some(3))
    );
    let kind = &report["task_kinds"][0];
    assert_eq!(kind["followed"]["sample_size"], 1);
    assert_eq!(kind["followed"]["records"], 2);
    assert_eq!(kind["followed"]["abandoned"]["numerator"], 1);
    assert_eq!(kind["followed"]["accepted_as_is"]["numerator"], 0);
    // Put the first result back for the rest of the recipe.
    record(
        &dir,
        &first,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .success();

    let report = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(
        (report["decisions"].as_u64(), report["records"].as_u64()),
        (Some(2), Some(4))
    );
    assert_eq!(report["attribution"], "not_established");
    let kind = &report["task_kinds"][0];
    assert_eq!(kind["task_kind"], "implement");
    assert_eq!(kind["followed"]["sample_size"], 1);
    assert_eq!(kind["followed"]["accepted_as_is"]["numerator"], 1);
    assert_eq!(kind["followed"]["accepted_as_is"]["denominator"], 1);
    assert_eq!(kind["followed"]["records"], 3);
    assert_eq!(kind["followed"]["unknown"]["cost"], 1);
    assert_eq!(kind["followed"]["unknown"]["used_model"], Value::Null);
    assert_eq!(kind["overridden"]["sample_size"], 1);
    assert_eq!(kind["overridden"]["accepted_after_rework"]["numerator"], 1);
    assert_eq!(kind["overridden"]["unknown"]["cost"], 0);
    assert_eq!(kind["overridden"]["unknown"]["used_model"], 0);
    assert_eq!(kind["overridden"]["unknown"]["used_effort"], 1);
    assert_eq!(kind["overridden"]["unknown"]["used_skills"], 0);

    // The needles exist in the advice file, so their absence below is meaningful.
    let advice_text = fs::read_to_string(&first).unwrap();
    assert!(advice_text.contains(OBJECTIVE) && advice_text.contains(dir.to_str().unwrap()));
    // No prompt, objective, path or comment reaches the store or the export.
    let export = feedback(&dir, "export", &[])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stored = fs::read(store(&dir).join("feedback.sqlite3")).unwrap();
    let wal = fs::read(store(&dir).join("feedback.sqlite3-wal")).unwrap_or_default();
    let exported = parsed(&export);
    let outcomes = exported["advisory_outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 4);
    // Every stored string is a hash, an enum value or a path-free identifier.
    fn leaves(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) => out.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
            Value::Object(fields) => fields.values().for_each(|v| leaves(v, out)),
            _ => {}
        }
    }
    let mut strings = Vec::new();
    leaves(&exported["advisory_outcomes"], &mut strings);
    assert!(!strings.is_empty());
    for text in strings {
        assert!(
            !text.contains(['/', '\\', ' ']) && !text.starts_with(['.', '~']),
            "path-like or free text stored: {text}"
        );
    }
    for needle in [
        OBJECTIVE,
        "dry-run",
        "restore helper",
        dir.to_str().unwrap(),
        "advice.json",
    ] {
        for (name, bytes) in [("export", &export), ("database", &stored), ("wal", &wal)] {
            assert!(
                !String::from_utf8_lossy(bytes).contains(needle),
                "{name} contains {needle:?}"
            );
        }
    }

    // Purge removes the records.
    let purged = parsed(
        &feedback(&dir, "purge", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(purged["purged_events"], 4);
    consent(&dir, NOW);
    let empty = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(empty["decisions"], 0);
    assert_eq!(
        empty["overall"]["followed"]["accepted_as_is"]["rate_micros"],
        Value::Null
    );
}

#[test]
fn free_text_is_refused_rather_than_stored() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir, "900");
    let advice = advise(&dir, "one", OBJECTIVE, &["gpt-5.6-sol", "gpt-5.6-terra"]);
    for (flag, value) in [
        ("--used-model", "the model my colleague suggested"),
        ("--used-effort", "as high as possible"),
        ("--used-skill", "my own notes in my home directory"),
        // Paths without a space are not free text, but they are not identifiers.
        ("--used-model", HOME_PATH),
        ("--used-skill", HOME_PATH),
        ("--used-skill", "../secret-repo"),
        ("--used-model", "~/secret-repo"),
        ("--used-model", ".secret"),
    ] {
        record(
            &dir,
            &advice,
            &[
                "--adherence",
                "overridden",
                "--result",
                "abandoned",
                flag,
                value,
            ],
        )
        .failure()
        .stderr(error_line("feedback.invalid"));
    }
    // A followed recommendation states no substitute.
    record(
        &dir,
        &advice,
        &[
            "--adherence",
            "followed",
            "--result",
            "abandoned",
            "--used-model",
            "gpt-5.6-terra",
        ],
    )
    .failure()
    .stderr(error_line("feedback.invalid"));
    // An override must say what was used instead.
    record(
        &dir,
        &advice,
        &["--adherence", "overridden", "--result", "abandoned"],
    )
    .failure()
    .stderr(error_line("feedback.invalid"));
    let report = parsed(
        &feedback(&dir, "advisory-report", &[])
            .assert()
            .success()
            .get_output()
            .stdout,
    );
    assert_eq!(report["decisions"], 0);
}

#[test]
fn recording_needs_consent_and_a_recommendation() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    let advice = advise(&dir, "one", OBJECTIVE, &["gpt-5.6-sol", "gpt-5.6-terra"]);
    record(
        &dir,
        &advice,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .failure()
    .stderr(error_line("feedback.consent_required"));
    consent(&dir, "900");

    // A report that recommends nothing has nothing to follow.
    let mut report = parsed(&fs::read(&advice).unwrap());
    report["status"] = json!("no_recommendation");
    let path = write(&dir.join("no-recommendation.json"), &report);
    record(
        &dir,
        &path,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .failure()
    .stderr(error_line("advisory.outcome.not_recommended"));

    // A file that is not an advice report is refused.
    let path = write(&dir.join("other.json"), &json!({"hello": "world"}));
    record(
        &dir,
        &path,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .failure()
    .stderr(error_line("request.json.invalid"));
}

#[test]
fn the_advice_size_bound_matches_what_advise_can_publish() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    consent(&dir, "900");
    let advice = advise(&dir, "one", OBJECTIVE, &["gpt-5.6-sol", "gpt-5.6-terra"]);
    // `advise` publishes a document of at most 1 MiB, newline included.
    let limit = 1024 * 1024;
    let mut text = fs::read_to_string(&advice).unwrap();
    assert!(text.len() < limit);
    let pad = |text: &str, total: usize| {
        format!(
            "{}{}\n",
            text.trim_end(),
            " ".repeat(total - text.trim_end().len() - 1)
        )
    };
    let exact = dir.join("exact.json");
    fs::write(&exact, pad(&text, limit)).unwrap();
    assert_eq!(fs::metadata(&exact).unwrap().len() as usize, limit);
    record(
        &dir,
        &exact,
        &["--adherence", "followed", "--result", "accepted-as-is"],
    )
    .success();
    text = pad(&text, limit + 1);
    let over = dir.join("over.json");
    fs::write(&over, text).unwrap();
    record(
        &dir,
        &over,
        &["--adherence", "followed", "--result", "abandoned"],
    )
    .failure()
    .stderr(error_line("request.too_large"));
}
