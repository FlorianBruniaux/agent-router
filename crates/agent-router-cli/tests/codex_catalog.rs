// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Sanitized discovery-shape fixtures. No native process, provider or attestation.
#![cfg(feature = "codex")]

use assert_cmd::Command;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn root(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn cache() -> Value {
    // Only public identifiers and effort sets from the October discovery shape.
    let standard = ["low", "medium", "high", "xhigh", "max"];
    let extended = ["low", "medium", "high", "xhigh", "max", "ultra"];
    let models: Vec<_> = [
        ("codex-auto-review", standard.as_slice()),
        ("gpt-5.6-luna", standard.as_slice()),
        ("gpt-5.6-sol", extended.as_slice()),
        ("gpt-5.6-terra", extended.as_slice()),
        ("gpt-6-astra", extended.as_slice()),
        ("gpt-6-luna", standard.as_slice()),
        ("gpt-6-sol", extended.as_slice()),
        ("gpt-6.1-sol", extended.as_slice()),
        ("gpt-6.1-terra", extended.as_slice()),
        ("gpt-reserve", standard.as_slice()),
    ].into_iter().map(|(id, efforts)| json!({"slug":id,
        "supported_reasoning_levels":efforts.iter().map(|effort| json!({"effort":effort})).collect::<Vec<_>>()
    })).collect();
    json!({"fetched_at":"1970-01-01T00:00:00Z","client_version":"0.163.0","models":models})
}

#[test]
fn current_codex_discovery_refreshes_without_promoting_unknown_tools_or_scoring_unrated_models() {
    let dir = tempfile::tempdir().unwrap();
    let cache_path = dir.path().join("cache.json");
    let original = serde_json::to_vec(&cache()).unwrap();
    fs::write(&cache_path, &original).unwrap();
    let observed = Command::cargo_bin("agent-router")
        .unwrap()
        .args(["inspect", "--host", "codex", "--model-cache"])
        .arg(&cache_path)
        .args([
            "--observed-version",
            "0.163.0-alpha.10.2",
            "--now",
            "10",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let observation: Value = serde_json::from_slice(&observed).unwrap();
    assert_eq!(observation["capabilities"]["tools"]["support"], "unknown");
    let snapshot = dir.path().join("snapshot.json");
    fs::write(&snapshot, &observed).unwrap();
    let generated = Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "refresh", "--host-snapshot"])
        .arg(&snapshot)
        .arg("--prior")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .args(["--now", "10", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let catalog: Value = serde_json::from_slice(&generated).unwrap();
    assert_eq!(catalog["catalog"]["models"].as_array().unwrap().len(), 10);
    for model in catalog["catalog"]["models"].as_array().unwrap() {
        assert!(model["capabilities"].as_array().unwrap().is_empty());
        if model["id"].as_str().unwrap().starts_with("gpt-6")
            || ["codex-auto-review", "gpt-reserve"].contains(&model["id"].as_str().unwrap())
        {
            assert_eq!(model["quality_micros"], 0);
            assert_eq!(model["cost_micros"], 1_000_000);
            assert_eq!(model["latency_micros"], 1_000_000);
            assert_eq!(model["source"], "builtin-unrated-policy-prior");
        }
    }
    let generated_path = dir.path().join("generated.json");
    fs::write(&generated_path, &generated).unwrap();
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "validate", "--catalog"])
        .arg(&generated_path)
        .args(["--now", "10", "--json"])
        .assert()
        .success();
    let decision = Command::cargo_bin("agent-router")
        .unwrap()
        .arg("route")
        .arg("--request")
        .arg(root("tests/fixtures/requests/codex-implementation.json"))
        .arg("--catalog")
        .arg(&generated_path)
        .arg("--host-snapshot")
        .arg(&snapshot)
        .args(["--now", "10", "--json"])
        .assert()
        .code(3)
        .get_output()
        .stdout
        .clone();
    let decision: Value = serde_json::from_slice(&decision).unwrap();
    assert!(decision["decision"].get("no_route").is_some());
    assert_eq!(fs::read(cache_path).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
}

#[test]
fn shared_cache_from_another_release_is_still_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");
    fs::write(&path, serde_json::to_vec(&cache()).unwrap()).unwrap();
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["inspect", "--host", "codex", "--model-cache"])
        .arg(path)
        .args(["--observed-version", "0.159.2", "--now", "10", "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"probe.version.mismatch\"}\n");
}

#[test]
fn extended_prior_does_not_guess_a_newly_discovered_model() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");
    let mut cache = cache();
    cache["models"][0]["slug"] = "fixture-future-model".into();
    fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    let observed = Command::cargo_bin("agent-router")
        .unwrap()
        .args(["inspect", "--host", "codex", "--model-cache"])
        .arg(&path)
        .args(["--now", "10", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let snapshot = dir.path().join("snapshot.json");
    fs::write(&snapshot, observed).unwrap();
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "refresh", "--host-snapshot"])
        .arg(&snapshot)
        .arg("--prior")
        .arg(root("assets/catalogs/codex-core-v2.json"))
        .args(["--now", "10", "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"catalog.invalid\"}\n");
}

#[test]
fn v3_validates_and_differs_from_v2_only_by_the_declared_ratings() {
    Command::cargo_bin("agent-router")
        .unwrap()
        .args(["catalog", "validate", "--catalog"])
        .arg(root("assets/catalogs/codex-core-v3.json"))
        .args(["--now", "10", "--json"])
        .assert()
        .success();
    let load = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(root(&format!("assets/catalogs/{name}"))).unwrap())
            .unwrap()
    };
    let (v2, v3) = (load("codex-core-v2.json"), load("codex-core-v3.json"));
    assert_eq!(v3["version"], "codex-core-v3");
    assert_eq!(v3["skills"], v2["skills"]);
    let models = |v: &Value| -> std::collections::BTreeMap<String, Value> {
        v["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["id"].as_str().unwrap().to_owned(), m.clone()))
            .collect()
    };
    let (m2, m3) = (models(&v2), models(&v3));
    assert_eq!(m3.len(), m2.len());
    let rated = ["gpt-6.1-sol", "gpt-6-sol", "gpt-6.1-terra", "gpt-6-luna"];
    for (id, model) in &m2 {
        if !rated.contains(&id.as_str()) {
            assert_eq!(&m3[id], model, "{id} must be unchanged");
        }
    }
    // Declared ordering: newer before older within a tier, Sol > Terra > Luna.
    let quality = |id: &str| m3[id]["quality_micros"].as_u64().unwrap();
    let order = [
        "gpt-6.1-sol",
        "gpt-6-sol",
        "gpt-5.6-sol",
        "gpt-6.1-terra",
        "gpt-5.6-terra",
        "gpt-6-luna",
        "gpt-5.6-luna",
    ];
    for pair in order.windows(2) {
        assert!(quality(pair[0]) > quality(pair[1]), "{pair:?}");
    }
    for (new, old) in [
        ("gpt-6.1-sol", "gpt-5.6-sol"),
        ("gpt-6-sol", "gpt-5.6-sol"),
        ("gpt-6.1-terra", "gpt-5.6-terra"),
        ("gpt-6-luna", "gpt-5.6-luna"),
    ] {
        for field in ["cost_micros", "latency_micros", "capabilities"] {
            assert_eq!(m3[new][field], m3[old][field], "{new} {field}");
        }
    }
    for id in ["gpt-6-astra", "codex-auto-review", "gpt-reserve"] {
        assert_eq!(m3[id]["quality_micros"], 0, "{id} stays unrated");
    }
    for id in rated.iter().chain(&["gpt-6.1-terra"]) {
        assert_eq!(m3[*id]["source"], "operator-declared-policy-prior");
        assert!(m3[*id]["quality_micros"].as_u64().unwrap() > 0);
    }
}
