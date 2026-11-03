// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Checks on `claude-core-v2` (ADR 0057): owner-declared qualities, costs from
//! the published output price, and `advise` plans that no longer fall back to
//! the identifier tie-break. No native process, provider or credential.
#![cfg(feature = "claude")]
#[path = "support/advisory.rs"]
mod advisory;
use advisory::{command, root, write};
use serde_json::{Value, json};
use std::{fs, path::Path};

const V2: &str = "assets/catalogs/claude-core-v2.json";

/// (id, quality, cost, stages) as declared in ADR 0057.
const RATINGS: [(&str, u64, u64, &[&str]); 5] = [
    (
        "claude-fable-5-1",
        990_000,
        1_000_000,
        &["plan", "execute", "verify"],
    ),
    ("claude-haiku-4-5-20251001", 650_000, 100_000, &["execute"]),
    ("claude-haiku-5-5", 690_000, 10_000, &["execute"]),
    (
        "claude-opus-5-5",
        970_000,
        400_000,
        &["plan", "execute", "verify"],
    ),
    (
        "claude-sonnet-5-5",
        860_000,
        200_000,
        &["execute", "verify"],
    ),
];

const HOST_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-haiku-5-5",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
];

fn read(path: &str) -> Value {
    serde_json::from_slice(&fs::read(root(path)).unwrap()).unwrap()
}

fn host(dir: &Path, models: &[&str], capabilities: Value) -> std::path::PathBuf {
    let efforts: serde_json::Map<String, Value> = models
        .iter()
        .map(|model| {
            (
                (*model).to_owned(),
                json!(["low", "medium", "high", "xhigh", "max"]),
            )
        })
        .collect();
    write(
        &dir.join("claude-host.json"),
        &json!({"id": "owner-claude-code", "harness": "claude",
            "supported_models": models, "supported_efforts": efforts,
            "capabilities": capabilities}),
    )
}

/// Runs `advise --json` on an inline task; returns the exit code and stdout.
fn advise(dir: &Path, catalog: &Path, host: &Path, kind: &str, risk: &str) -> (i32, Value) {
    advise_with(
        dir,
        catalog,
        host,
        kind,
        risk,
        &["--declare-capability", "tools"],
    )
}

fn advise_with(
    _dir: &Path,
    catalog: &Path,
    host: &Path,
    kind: &str,
    risk: &str,
    extra: &[&str],
) -> (i32, Value) {
    let mut cmd = command();
    cmd.args(["advise", "--text", "Bounded task", "--kind", kind])
        .args(["--risk", risk, "--scope", "crates"])
        .args(["--budget", "2000000:1800000:400000"])
        .arg("--catalog")
        .arg(catalog)
        .arg("--host")
        .arg(host)
        .args(extra)
        .args(["--now", "10", "--json"]);
    if kind == "migration" {
        cmd.args(["--environment", "local", "--reversibility", "irreversible"]);
    }
    let output = cmd.output().unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (output.status.code().unwrap(), value)
}

fn plan(report: &Value) -> Vec<String> {
    report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .map(|stages| {
            stages
                .iter()
                .map(|stage| format!("{}/{}", stage["model"].as_str().unwrap(), stage["effort"]))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn shipped_v2_validates_and_carries_the_declared_ratings() {
    command()
        .args(["catalog", "validate", "--catalog"])
        .arg(root(V2))
        .args(["--now", "10", "--json"])
        .assert()
        .success();
    let catalog = read(V2);
    assert_eq!(catalog["version"], "claude-core-v2");
    let models = catalog["models"].as_array().unwrap();
    assert_eq!(models.len(), RATINGS.len());
    for (model, (id, quality, cost, stages)) in models.iter().zip(RATINGS) {
        assert_eq!(model["id"], id);
        assert_eq!(model["quality_micros"], quality, "{id}");
        assert_eq!(model["cost_micros"], cost, "{id}");
        assert_eq!(model["stages"], json!(stages), "{id}");
        // Latency is not timed: it keeps the conservative unrated bound.
        assert_eq!(model["latency_micros"], 1_000_000, "{id}");
        assert_eq!(model["source"], "operator-declared-policy-prior", "{id}");
    }
}

#[test]
fn v2_changes_only_rating_fields_of_v1() {
    let (v1, v2) = (read("assets/catalogs/claude-core-v1.json"), read(V2));
    assert_eq!(v1["skills"], v2["skills"]);
    assert_eq!(v1["schema_version"], v2["schema_version"]);
    let rating = ["quality_micros", "cost_micros", "stages", "source"];
    for (old, new) in v1["models"]
        .as_array()
        .unwrap()
        .iter()
        .zip(v2["models"].as_array().unwrap())
    {
        let (mut old, mut new) = (old.clone(), new.clone());
        for field in rating {
            old.as_object_mut().unwrap().remove(field);
            new.as_object_mut().unwrap().remove(field);
        }
        assert_eq!(old, new);
    }
}

#[test]
fn v2_plans_follow_risk_and_are_never_a_tie_break() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path(), &HOST_MODELS, json!(["tools"]));
    let cases = [
        (
            "implement",
            "medium",
            [
                "claude-opus-5-5/\"high\"",
                "claude-sonnet-5-5/\"medium\"",
                "claude-sonnet-5-5/\"medium\"",
            ],
        ),
        (
            "review",
            "medium",
            [
                "claude-opus-5-5/\"high\"",
                "claude-sonnet-5-5/\"medium\"",
                "claude-opus-5-5/\"high\"",
            ],
        ),
        (
            "migration",
            "critical",
            [
                "claude-fable-5-1/\"max\"",
                "claude-opus-5-5/\"high\"",
                "claude-fable-5-1/\"xhigh\"",
            ],
        ),
    ];
    for (kind, risk, expected) in cases {
        let (code, report) = advise(dir.path(), &root(V2), &host, kind, risk);
        assert_eq!(code, 0, "{kind}/{risk}: {report}");
        assert_eq!(plan(&report), expected, "{kind}/{risk}");
        assert_eq!(
            report["next_manual_action"]["selection_basis"], "catalog_policy_prior",
            "{kind}/{risk}"
        );
    }
}

#[test]
fn unrated_entry_never_wins_a_stage_over_a_rated_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = read(V2);
    let fable = &mut catalog["models"][0];
    assert_eq!(fable["id"], "claude-fable-5-1");
    fable["quality_micros"] = 0.into();
    fable["cost_micros"] = 1_000_000.into();
    fable["source"] = "test-only-unrated-prior".into();
    let path = write(&dir.path().join("catalog.json"), &catalog);
    let host = host(dir.path(), &HOST_MODELS, json!(["tools"]));
    let (code, report) = advise(dir.path(), &path, &host, "migration", "critical");
    assert_eq!(code, 0, "{report}");
    let plan = plan(&report);
    assert_eq!(plan.len(), 3);
    assert!(
        plan.iter()
            .all(|stage| !stage.starts_with("claude-fable-5-1/")),
        "{plan:?}"
    );
}

#[test]
fn model_absent_from_the_catalog_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path(), &["claude-unlisted-model"], json!(["tools"]));
    let (code, _) = advise(dir.path(), &root(V2), &host, "implement", "medium");
    assert_ne!(code, 0);
}

#[test]
fn tools_stay_unknown_without_host_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let require = ["--require-capability", "tools"];
    // The catalog declares tools, but neither the host nor the operator does: no plan.
    let bare = host(dir.path(), &HOST_MODELS, json!([]));
    let (_, report) = advise_with(
        dir.path(),
        &root(V2),
        &bare,
        "implement",
        "medium",
        &require,
    );
    assert!(plan(&report).is_empty(), "{report}");
    // The same task routes once the host lists tools and the operator declares them.
    let listed = host(dir.path(), &HOST_MODELS, json!(["tools"]));
    let declared = [
        "--require-capability",
        "tools",
        "--declare-capability",
        "tools",
    ];
    let (code, report) = advise_with(
        dir.path(),
        &root(V2),
        &listed,
        "implement",
        "medium",
        &declared,
    );
    assert_eq!(code, 0, "{report}");
    assert_eq!(plan(&report).len(), 3);
}
