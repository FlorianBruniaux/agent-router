use std::fs;
use std::path::Path;
use xtask::{hook, projection, transaction};

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../harness"),
        &dir.path().join("harness"),
    );
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &dir.path().join("xtask/src"),
    );
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        dir.path().join("xtask/Cargo.toml"),
    )
    .unwrap();
    dir
}

#[test]
fn stale_binary_cannot_attest_new_source() {
    let dir = fixture();
    install(dir.path());
    fs::write(dir.path().join("xtask/src/hook.rs"), "new source").unwrap();
    assert!(projection::check(dir.path()).is_err());
    let output = hook::run(
        dir.path(),
        "codex",
        br#"{"hook_event_name":"SessionStart","source":"startup"}"#,
    )
    .unwrap();
    assert!(
        serde_json::to_string(&output)
            .unwrap()
            .contains("CHECK-REQUIRED")
    );
}

fn install(root: &Path) {
    transaction::apply(root, projection::render(root).unwrap()).unwrap();
}

#[test]
fn generation_is_deterministic_idempotent_and_dual_host() {
    let dir = fixture();
    let expected = projection::render(dir.path()).unwrap();
    assert_eq!(expected, projection::render(dir.path()).unwrap());
    assert_eq!(expected["AGENTS.md"], expected["CLAUDE.md"]);
    for (path, content) in &expected {
        if let Some(suffix) = path.strip_prefix(".agents/skills/") {
            assert_eq!(content, &expected[&format!(".claude/skills/{suffix}")]);
        }
    }
    assert!(transaction::apply(dir.path(), expected.clone()).unwrap() > 0);
    assert_eq!(transaction::apply(dir.path(), expected).unwrap(), 0);
    projection::check(dir.path()).unwrap();
}

#[test]
fn check_detects_drift_missing_and_stale_outputs() {
    for mutation in ["changed", "missing", "stale"] {
        let dir = fixture();
        install(dir.path());
        match mutation {
            "changed" => fs::write(dir.path().join("AGENTS.md"), "local edit").unwrap(),
            "missing" => fs::remove_file(dir.path().join("CLAUDE.md")).unwrap(),
            _ => fs::write(dir.path().join(".codex/agents/obsolete.toml"), "old").unwrap(),
        }
        assert!(projection::check(dir.path()).is_err(), "{mutation}");
        assert!(
            transaction::apply(dir.path(), projection::render(dir.path()).unwrap()).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn update_remove_and_rollback_preserve_unrelated_files() {
    let dir = fixture();
    install(dir.path());
    fs::write(dir.path().join("unrelated.txt"), "keep").unwrap();
    let original = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    let source = dir.path().join("harness/instructions.md");
    fs::write(&source, "# Updated canonical instructions\n").unwrap();
    assert!(transaction::apply(dir.path(), projection::render(dir.path()).unwrap()).unwrap() > 0);
    projection::check(dir.path()).unwrap();
    transaction::rollback(dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        original
    );
    // Source rollback is separate and must be explicit; projections now drift.
    assert!(projection::check(dir.path()).is_err());
    transaction::apply(dir.path(), projection::Files::new()).unwrap();
    assert!(!dir.path().join("AGENTS.md").exists());
    transaction::rollback(dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        original
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("unrelated.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn collision_and_rollback_concurrent_edit_are_refused_before_mutation() {
    let dir = fixture();
    fs::write(dir.path().join("CLAUDE.md"), "owner content").unwrap();
    assert!(transaction::apply(dir.path(), projection::render(dir.path()).unwrap()).is_err());
    assert!(!dir.path().join("AGENTS.md").exists());
    fs::remove_file(dir.path().join("CLAUDE.md")).unwrap();
    install(dir.path());
    fs::write(dir.path().join("AGENTS.md"), "concurrent edit").unwrap();
    assert!(transaction::rollback(dir.path()).is_err());
    assert!(dir.path().join("CLAUDE.md").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        "concurrent edit"
    );
}

#[test]
fn machine_local_files_are_not_inputs_or_outputs() {
    let dir = fixture();
    let before = projection::render(dir.path()).unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(
        dir.path().join(".claude/settings.local.json"),
        "PRIVATE_SENTINEL",
    )
    .unwrap();
    fs::write(dir.path().join("harness/.env"), "PRIVATE_SENTINEL").unwrap();
    fs::write(dir.path().join(".env"), "PRIVATE_SENTINEL").unwrap();
    let after = projection::render(dir.path()).unwrap();
    assert_eq!(before, after);
    assert!(after.values().all(|v| !v.contains("PRIVATE_SENTINEL")));
    install(dir.path());
    projection::check(dir.path()).unwrap();
}

#[test]
fn malicious_names_unknown_fields_and_inventory_escapes_are_refused() {
    for mutation in ["path", "alias", "extra"] {
        let dir = fixture();
        let path = dir.path().join("harness/manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        match mutation {
            "path" => manifest["skills"][0] = "../../.env".into(),
            "alias" => manifest["agents"][0]["codex_model"] = "opus".into(),
            _ => manifest["environment"] = "PRIVATE_SENTINEL".into(),
        }
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(projection::render(dir.path()).is_err());
    }
    let dir = fixture();
    install(dir.path());
    let path = dir.path().join(projection::INVENTORY);
    let mut inv: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    inv["files"][".codex/agents/../../../victim"] = "a".repeat(64).into();
    fs::write(path, serde_json::to_vec(&inv).unwrap()).unwrap();
    assert!(transaction::apply(dir.path(), projection::Files::new()).is_err());
    assert!(dir.path().join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn source_output_ancestor_and_state_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    for target in ["harness/instructions.md", ".codex", ".harness-state"] {
        let dir = fixture();
        let outside = tempfile::tempdir().unwrap();
        let path = dir.path().join(target);
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        symlink(outside.path(), &path).unwrap();
        let outcome =
            projection::render(dir.path()).and_then(|files| transaction::apply(dir.path(), files));
        assert!(outcome.is_err(), "{target}");
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[test]
fn an_active_writer_lock_prevents_generation() {
    let dir = fixture();
    fs::create_dir(dir.path().join(".harness-state")).unwrap();
    fs::write(dir.path().join(".harness-state/lock"), "").unwrap();
    assert!(transaction::apply(dir.path(), projection::render(dir.path()).unwrap()).is_err());
    assert!(!dir.path().join("AGENTS.md").exists());
}

#[test]
fn oversized_recovery_journal_is_refused_before_projection_changes() {
    let dir = fixture();
    install(dir.path());
    let before = fs::read(dir.path().join("AGENTS.md")).unwrap();
    let journal = fs::read(dir.path().join(".harness-state/rollback.json")).unwrap();
    fs::write(
        dir.path().join("harness/instructions.md"),
        "x".repeat(600_000),
    )
    .unwrap();
    let result = transaction::apply(dir.path(), projection::render(dir.path()).unwrap());
    assert!(result.unwrap_err().to_string().contains("journal exceeds"));
    assert_eq!(fs::read(dir.path().join("AGENTS.md")).unwrap(), before);
    assert_eq!(
        fs::read(dir.path().join(".harness-state/rollback.json")).unwrap(),
        journal
    );
    transaction::rollback(dir.path()).unwrap();
    assert!(!dir.path().join("AGENTS.md").exists());
}

#[test]
fn hook_fixture_matrix_keeps_authority_with_the_host() {
    let dir = fixture();
    install(dir.path());
    for host in ["codex", "claude"] {
        for (fixture, expected) in [
            ("success", Some("parity-ok")),
            ("noop", None),
            ("invalid", Some("UNKNOWN-invalid-input")),
            ("denied", Some("parity-ok")),
        ] {
            let bytes = fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/fixtures/{host}-{fixture}.json")),
            )
            .unwrap();
            let result = hook::run(dir.path(), host, &bytes);
            if let Some(expected) = expected {
                let output = serde_json::to_string(&result.unwrap()).unwrap();
                assert!(output.contains(expected), "{output}");
                assert!(!output.contains("permissionDecision"));
                assert!(!output.contains("PRIVATE_SENTINEL"));
                assert!(!output.contains("bypassPermissions"));
            } else {
                assert!(result.is_none());
            }
        }
        let output =
            serde_json::to_string(&hook::run(dir.path(), host, &vec![b'a'; 65537]).unwrap())
                .unwrap();
        assert!(output.contains("UNKNOWN-input-limit"));
    }
    fs::write(dir.path().join("AGENTS.md"), "drift").unwrap();
    let bytes = br#"{"hook_event_name":"SessionStart","source":"startup"}"#;
    let output = serde_json::to_string(&hook::run(dir.path(), "codex", bytes).unwrap()).unwrap();
    assert!(output.contains("CHECK-REQUIRED"));
    let absent = dir.path().join("does-not-exist");
    assert!(
        serde_json::to_string(&hook::run(&absent, "claude", bytes).unwrap())
            .unwrap()
            .contains("CHECK-REQUIRED")
    );
}

#[cfg(unix)]
#[test]
fn actual_launcher_handles_missing_dependency_and_invokes_rust() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let dir = fixture();
    install(dir.path());
    for host in ["codex", "claude"] {
        let output = Command::new("sh")
            .arg(dir.path().join("harness/session-start.sh"))
            .arg(host)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("UNKNOWN-missing-binary")
        );
    }
    fs::create_dir_all(dir.path().join("target/debug")).unwrap();
    fs::copy(
        assert_cmd::cargo::cargo_bin("xtask"),
        dir.path().join("target/debug/xtask"),
    )
    .unwrap();
    for host in ["codex", "claude"] {
        let mut child = Command::new("sh")
            .arg(dir.path().join("harness/session-start.sh"))
            .arg(host)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"hook_event_name":"SessionStart","source":"startup"}"#)
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        assert!(
            String::from_utf8(result.stdout)
                .unwrap()
                .contains(&format!("{host}:parity-ok"))
        );
    }
}

#[test]
fn harness_is_not_a_product_dependency() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for name in [
        "router-types",
        "router-catalog",
        "router-policy",
        "agent-router-cli",
    ] {
        let manifest = fs::read_to_string(root.join(format!("crates/{name}/Cargo.toml"))).unwrap();
        assert!(!manifest.contains("xtask"));
    }
    let manifest = fs::read_to_string(root.join("xtask/Cargo.toml")).unwrap();
    assert!(!manifest.contains("path ="));
}
