// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
#![cfg(all(unix, feature = "codex"))]
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn command(config: &Path) -> Command {
    let mut c = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    c.args(["finite-entry", "--config"])
        .arg(config)
        .arg("--json");
    c
}
fn config(root: &Path, enabled: bool) -> Value {
    json!({"schema_version":1,"enabled":enabled,"transport":"local_scripted",
        "root":root.join("fresh"),"programs":{},"allowed_models":{"gpt-6-luna":["medium"]}})
}
fn error(c: &mut Command) -> Value {
    let a = c.assert().failure();
    serde_json::from_slice(&a.get_output().stderr).unwrap()
}
#[test]
fn disabled_entry_does_not_read_task_or_create_state() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    fs::write(&p, config(d.path(), false).to_string()).unwrap();
    let v = error(command(&p).write_stdin("not a task"));
    assert_eq!(v["error"], "finite_entry.disabled");
    assert!(!d.path().join("fresh").exists());
}
#[test]
fn unknown_or_duplicate_config_fields_do_not_create_state() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    for raw in [
        format!(
            "{{\"schema_version\":1,{}",
            &config(d.path(), false).to_string()[1..]
        ),
        {
            let mut v = config(d.path(), false);
            v["provider_url"] = json!("https://example.invalid");
            v.to_string()
        },
    ] {
        fs::write(&p, raw).unwrap();
        let v = error(&mut command(&p));
        assert_eq!(v["error"], "finite_entry.config");
        assert!(!d.path().join("fresh").exists());
    }
}
#[test]
fn symlink_config_is_refused_without_following_it() {
    let d = tempfile::tempdir().unwrap();
    let real = d.path().join("real.json");
    let p = d.path().join("config.json");
    fs::write(&real, config(d.path(), false).to_string()).unwrap();
    std::os::unix::fs::symlink(&real, &p).unwrap();
    let v = error(&mut command(&p));
    assert_eq!(v["error"], "finite_entry.config");
}

fn enabled(root: &Path) -> Value {
    let mut c = config(root, true);
    for name in [
        "client",
        "native",
        "helper",
        "guard",
        "namespace",
        "profile",
    ] {
        c["programs"][name] = json!({"path":root.join(name),"sha256":"a".repeat(64)});
    }
    c
}
fn submission() -> Value {
    let files: serde_json::Map<String, Value> = [
        ("AGENTS.md", "Use the supplied public files only."),
        ("statement.md", "Correct the score calculation."),
        ("lib.rs", "pub fn score() -> u64 { 0 }"),
    ]
    .into_iter()
    .map(|(n, s)| {
        (
            n.into(),
            json!({"content":s,"sha256":router_adapter_api::content_digest(s.as_bytes())}),
        )
    })
    .collect();
    json!({"schema_version":1,"kind":"submit","model":"gpt-6-luna","effort":"medium","output":"lib.rs","public_files":files,
        "task":{"objective":"Correct the score calculation","kind":"implement","deliverables":["lib.rs"],"scope":["lib.rs"],"risk":"low","sensitivity":"public","budget":{"max_cost_microusd":10000,"max_elapsed_ms":60000,"max_tokens":6000},"allowed_mutations":["workspace_write","network"],"acceptance":["The score matches the public statement"],"required_capabilities":["network","workspace_write","tools"]},
        "risk_context":{"schema_version":1,"environment":"local","reversibility":"reversible","operations":["workspace_change"]}})
}
fn refused_submission(change: impl FnOnce(&mut Value), expected: &str) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    fs::write(&p, enabled(d.path()).to_string()).unwrap();
    let mut s = submission();
    change(&mut s);
    let v = error(command(&p).write_stdin(format!("{s}\n")));
    assert_eq!(v["error"], expected);
    assert!(!d.path().join("fresh").exists());
}
#[test]
fn missing_risk_context_refuses_instead_of_inventing_local_authority() {
    refused_submission(
        |v| {
            v["risk_context"]["environment"] = json!("unknown");
        },
        "finite_entry.task",
    );
}
#[test]
fn higher_risk_wording_cannot_be_lowered_by_the_supplied_risk() {
    refused_submission(
        |v| {
            v["task"]["objective"] = json!("Deploy the result to production");
        },
        "finite_entry.task",
    );
}
#[test]
fn unknown_model_or_effort_refuses_before_native_preparation() {
    refused_submission(
        |v| v["model"] = json!("unapproved"),
        "finite_entry.selection",
    );
    refused_submission(|v| v["effort"] = json!("high"), "finite_entry.selection");
}
#[test]
fn public_inventory_refuses_traversal_wrong_hash_and_output_outside_scope() {
    refused_submission(
        |v| {
            v["public_files"]["../private"] = v["public_files"]["lib.rs"].clone();
        },
        "finite_entry.task",
    );
    refused_submission(
        |v| v["public_files"]["lib.rs"]["sha256"] = json!("0".repeat(64)),
        "finite_entry.task",
    );
    refused_submission(|v| v["output"] = json!("statement.md"), "finite_entry.task");
}
#[test]
fn existing_root_is_not_reopened_or_modified() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    let existing = d.path().join("fresh");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("sentinel"), b"keep").unwrap();
    fs::write(&p, enabled(d.path()).to_string()).unwrap();
    let v = error(command(&p).write_stdin(format!("{}\n", submission())));
    assert_eq!(v["error"], "finite_entry.fresh_required");
    assert_eq!(fs::read(existing.join("sentinel")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(existing).unwrap().count(), 1);
}
#[test]
fn absent_submission_is_refused_before_creating_state() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    fs::write(&p, enabled(d.path()).to_string()).unwrap();
    let v = error(command(&p).write_stdin(""));
    assert_eq!(v["error"], "finite_entry.input");
    assert!(!d.path().join("fresh").exists());
}

#[test]
#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
fn valid_submission_preserves_risk_floor_before_platform_refusal() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    fs::write(&p, enabled(d.path()).to_string()).unwrap();
    let a = command(&p)
        .write_stdin(format!("{}\n", submission()))
        .assert()
        .failure();
    let rows: Vec<Value> = a
        .get_output()
        .stdout
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "entry.accepted");
    assert_eq!(rows[0]["task"]["risk"], "medium");
    assert_eq!(rows[0]["model"], "gpt-6-luna");
    assert_eq!(rows[0]["effort"], "medium");
    assert_eq!(rows[0]["worker_attempts"], 0);
    assert!(!d.path().join("fresh").exists());
}

#[test]
fn qualification_mode_is_explicit_and_refuses_other_public_tasks() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config.json");
    let mut c = enabled(d.path());
    c["qualification"] = json!("fresh_t02_v1");
    fs::write(&p, c.to_string()).unwrap();
    let v = error(command(&p).write_stdin(format!("{}\n", submission())));
    #[cfg(feature = "qualification-fresh-t02")]
    assert_eq!(v["error"], "finite_entry.qualification_scope");
    #[cfg(not(feature = "qualification-fresh-t02"))]
    assert_eq!(v["error"], "finite_entry.config");
    assert!(!d.path().join("fresh").exists());
}
