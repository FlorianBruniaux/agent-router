// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `feedback --store` and `policy --store` accept a path relative to the working directory.
use assert_cmd::Command;
use std::fs;

fn command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("agent-router"))
}

#[test]
fn feedback_store_resolves_a_relative_path_against_the_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    command()
        .current_dir(&dir)
        .args([
            "feedback", "--store", "store", "consent", "--now", "100", "--json",
        ])
        .assert()
        .success();
    assert!(dir.join("store/feedback.sqlite3").is_file());
}

#[test]
fn policy_store_resolves_a_relative_path_against_the_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let dir = fs::canonicalize(dir.path()).unwrap();
    command()
        .current_dir(&dir)
        .args([
            "policy", "--store", "store", "active", "--now", "100", "--json",
        ])
        .assert()
        .success();
    assert!(dir.join("store/feedback.sqlite3").is_file());
}
