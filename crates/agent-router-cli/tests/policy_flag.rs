// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `--policy` selection for `route` and `advise` (ADR 0054). Nothing is executed
//! and no provider is contacted.
#[path = "support/advisory.rs"]
mod advisory;
use advisory::command;
use serde_json::Value;
use std::path::Path;

fn root(path: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .display()
        .to_string()
}
fn route(extra: &[&str]) -> Value {
    let output = command()
        .args(["route", "--request"])
        .arg(root("tests/fixtures/requests/codex-implementation.json"))
        .arg("--catalog")
        .arg(root("assets/catalogs/codex-core-v1.json"))
        .args(extra)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn route_defaults_to_stable_v1_and_accepts_stable_v2() {
    let default = route(&[]);
    let v1 = route(&["--policy", "stable-v1"]);
    let v2 = route(&["--policy", "stable-v2"]);
    assert_eq!(default["policy_version"], "stable-v1");
    assert_eq!(default, v1, "the default must equal explicit stable-v1");
    assert_eq!(v2["policy_version"], "stable-v2");
    assert_eq!(v2["schema_version"], "1");
    assert_ne!(v1["decision_id"], v2["decision_id"]);
}

#[test]
fn route_rejects_an_unknown_policy() {
    command()
        .args(["route", "--policy", "stable-v3", "--json"])
        .assert()
        .failure();
}

#[test]
fn advise_rejects_an_unknown_policy() {
    command()
        .args(["advise", "--policy", "stable-v3", "--json"])
        .assert()
        .failure();
}
