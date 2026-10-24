#![cfg(all(unix, feature = "codex"))]
use assert_cmd::Command;
use serde_json::json;
use std::fs;
#[test]
fn resume_requires_existing_runtime_and_never_creates_a_registry() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap().join("runtime");
    let registry = root.with_file_name("admission");
    let config = d.path().join("resume.json");
    fs::write(&config,json!({"schema_version":1,"root":root,"admission":{"schema_version":1,"registry":registry,"expected_policy_sha256":"e".repeat(64),"runtime_id":"entry","session_budget":null}}).to_string()).unwrap();
    for existing in [false, true] {
        if existing {
            fs::create_dir(&root).unwrap();
        }
        Command::new(assert_cmd::cargo::cargo_bin!("agent-router"))
            .args(["finite-resume", "--config"])
            .arg(&config)
            .args(["--run-id", "entry", "--json", "--approve"])
            .write_stdin("unapproved\n")
            .assert()
            .failure();
        assert!(!root.join("runtime.sqlite3").exists());
        assert!(!registry.exists());
    }
}
#[test]
fn resume_json_protocol_is_required() {
    Command::new(assert_cmd::cargo::cargo_bin!("agent-router"))
        .args(["finite-resume", "--config", "/missing", "--run-id", "entry"])
        .assert()
        .failure();
}
