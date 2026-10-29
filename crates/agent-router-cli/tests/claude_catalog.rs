// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Route checks on `claude-core-v1`. No native process, provider or credential.
#![cfg(feature = "claude")]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};

const IDS: [&str; 3] = [
    "claude-haiku-4-5-20251001",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
];

fn root(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn catalog() -> Value {
    serde_json::from_slice(&fs::read(root("assets/catalogs/claude-core-v1.json")).unwrap()).unwrap()
}

fn request(models: &[&str], required: &[&str], knowledge: Option<Value>) -> Value {
    let efforts: serde_json::Map<String, Value> = models
        .iter()
        .map(|model| ((*model).to_owned(), json!(["medium", "high"])))
        .collect();
    let mut value = json!({
        "task": {
            "objective": "Implement a bounded parser change",
            "kind": "implement",
            "risk": "low",
            "required_capabilities": required,
            "acceptance": ["workspace tests pass"]
        },
        "host": {
            "id": "claude-local",
            "harness": "claude",
            "supported_models": models,
            "supported_efforts": efforts,
            "capabilities": []
        },
        "skill_candidates": []
    });
    if let Some(knowledge) = knowledge {
        value["capability_knowledge"] = knowledge;
    }
    value
}

/// Returns the exit code and the decision document.
fn route(catalog: &Value, request: &Value) -> (i32, Value) {
    let dir = tempfile::tempdir().unwrap();
    let catalog_path = dir.path().join("catalog.json");
    let request_path = dir.path().join("request.json");
    fs::write(&catalog_path, serde_json::to_vec(catalog).unwrap()).unwrap();
    fs::write(&request_path, serde_json::to_vec(request).unwrap()).unwrap();
    let output = Command::cargo_bin("agent-router")
        .unwrap()
        .arg("route")
        .arg("--request")
        .arg(&request_path)
        .arg("--catalog")
        .arg(&catalog_path)
        .args(["--now", "10", "--json"])
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

fn selected_models(decision: &Value) -> Vec<String> {
    decision["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["model"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn shipped_catalog_validates_and_declares_every_entry_unrated_without_capabilities() {
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "validate", "--catalog"])
        .arg(root("assets/catalogs/claude-core-v1.json"))
        .args(["--now", "10", "--json"])
        .assert()
        .success();
    let catalog = catalog();
    let models = catalog["models"].as_array().unwrap();
    let ids: Vec<_> = models
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, IDS);
    for model in models {
        assert_eq!(model["quality_micros"], 0);
        assert_eq!(model["cost_micros"], 1_000_000);
        assert_eq!(model["latency_micros"], 1_000_000);
        assert_eq!(model["source"], "operator-declared-unrated-prior");
        assert!(model["capabilities"].as_array().unwrap().is_empty());
        assert!(
            !model["efforts"]
                .as_array()
                .unwrap()
                .contains(&json!("ultra"))
        );
    }
}

#[test]
fn nominal_request_routes_only_to_models_the_host_lists() {
    let host = ["claude-sonnet-5-5", "claude-opus-5-5"];
    let (code, decision) = route(&catalog(), &request(&host, &[], None));
    assert_eq!(code, 0);
    assert_eq!(decision["catalog_version"], "claude-core-v1");
    let models = selected_models(&decision);
    assert_eq!(models.len(), 3);
    assert!(models.iter().all(|model| host.contains(&model.as_str())));
    for stage in decision["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
    {
        assert!(["medium", "high"].contains(&stage["effort"].as_str().unwrap()));
    }
}

#[test]
fn model_absent_from_the_catalog_is_refused_even_when_the_host_lists_it() {
    let (code, decision) = route(&catalog(), &request(&["claude-unlisted-model"], &[], None));
    assert_eq!(code, 3);
    assert!(decision["decision"].get("no_route").is_some());
}

#[test]
fn catalog_model_absent_from_the_host_is_never_selected() {
    let (code, decision) = route(
        &catalog(),
        &request(&["claude-haiku-4-5-20251001"], &[], None),
    );
    assert_eq!(code, 0);
    assert_eq!(selected_models(&decision), ["claude-haiku-4-5-20251001"; 3]);
}

#[test]
fn unrated_model_is_never_preferred_over_a_rated_one() {
    // Test-only rating: the shipped catalog rates nothing.
    let mut rated = catalog();
    let entry = &mut rated["models"][2];
    assert_eq!(entry["id"], "claude-sonnet-5-5");
    entry["quality_micros"] = json!(500_000);
    entry["cost_micros"] = json!(500_000);
    entry["latency_micros"] = json!(500_000);
    entry["source"] = "test-only-rated-prior".into();
    let host: Vec<&str> = IDS.to_vec();
    let (code, decision) = route(&rated, &request(&host, &[], None));
    assert_eq!(code, 0);
    assert_eq!(selected_models(&decision), ["claude-sonnet-5-5"; 3]);
    for alternative in decision["decision"]["selected"]["alternatives"]
        .as_array()
        .unwrap()
    {
        let stages = alternative["stages"].as_array().unwrap();
        assert!(
            stages
                .iter()
                .any(|stage| stage["model"] == "claude-sonnet-5-5")
        );
    }
}

#[test]
fn tools_stay_unknown_so_a_tools_task_does_not_route() {
    let host = ["claude-sonnet-5-5"];
    let unknown = request(&host, &["tools"], Some(json!({"tools": "unknown"})));
    let (code, decision) = route(&catalog(), &unknown);
    assert_eq!(code, 3);
    assert!(decision["decision"].get("no_route").is_some());
    // Declared host support alone does not promote the catalog's empty capabilities.
    let mut declared = request(&host, &["tools"], None);
    declared["host"]["capabilities"] = json!(["tools"]);
    let (code, _) = route(&catalog(), &declared);
    assert_eq!(code, 3);
}

#[test]
fn claude_native_stream_snapshot_cannot_refresh_the_catalog_without_effort_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let stream = dir.path().join("stream.jsonl");
    fs::write(
        &stream,
        concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-haiku-4-5\",\"claude_code_version\":\"2.1.285\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"type\":\"message\",\"model\":\"claude-haiku-4-5-20251001\"}}\n",
        ),
    )
    .unwrap();
    let observed = Command::cargo_bin("agent-router")
        .unwrap()
        .args(["inspect", "--host", "claude", "--native-stream"])
        .arg(&stream)
        .args(["--observed-at", "5", "--now", "10", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let observation: Value = serde_json::from_slice(&observed).unwrap();
    assert_eq!(observation["capabilities"]["tools"]["support"], "unknown");
    assert!(
        observation["host"]["supported_efforts"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let snapshot = dir.path().join("snapshot.json");
    fs::write(&snapshot, &observed).unwrap();
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "refresh", "--host-snapshot"])
        .arg(&snapshot)
        .arg("--prior")
        .arg(root("assets/catalogs/claude-core-v1.json"))
        .args(["--now", "10", "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"catalog.invalid\"}\n");
}
