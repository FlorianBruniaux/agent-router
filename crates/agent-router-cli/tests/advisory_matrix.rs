// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Golden matrix of `advise` over seven representative tasks. It pins today's
//! plans so any later routing change is a reviewable diff and so a collapse of
//! differentiation cannot happen silently. Nothing is executed and no provider
//! is contacted. The helpers below are copied from `advisory_route.rs` on
//! purpose, that file is edited by parallel changes.
#![cfg(feature = "codex")]
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NOW: &str = "10";
const GOLDEN: &str = "tests/golden/advise-matrix-v1.json";

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
fn parsed(stdout: &[u8], stderr: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap_or_else(|error| {
        panic!(
            "expected one JSON result: {error}; stderr={}",
            String::from_utf8_lossy(stderr)
        )
    })
}

/// Synthetic Codex host snapshot built like the Codex probe: models and efforts
/// are discovered, capabilities stay UNKNOWN.
fn discovered_snapshot(dir: &Path) -> PathBuf {
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
    ]
    .into_iter()
    .map(|(slug, efforts)| {
        json!({"slug": slug, "supported_reasoning_levels":
            efforts.iter().map(|effort| json!({"effort": effort})).collect::<Vec<_>>()})
    })
    .collect();
    let cache = write(
        &dir.join("cache.json"),
        &json!({"fetched_at": "1970-01-01T00:00:00Z", "client_version": "0.163.0", "models": models}),
    );
    let observed = command()
        .args(["inspect", "--host", "codex", "--model-cache"])
        .arg(&cache)
        .args(["--observed-version", "0.163.0", "--now", NOW, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let path = dir.join("observation.json");
    fs::write(&path, observed).unwrap();
    path
}

/// Declared Claude host: a flat catalog with no capability evidence.
fn claude_host(dir: &Path) -> PathBuf {
    let models = [
        "claude-haiku-4-5-20251001",
        "claude-opus-5-5",
        "claude-sonnet-5-5",
    ];
    let efforts: BTreeMap<_, _> = models
        .iter()
        .map(|model| (*model, json!(["medium", "high", "xhigh"])))
        .collect();
    write(
        &dir.join("claude-host.json"),
        &json!({"id": "owner-claude-code", "harness": "claude",
            "supported_models": models, "supported_efforts": efforts,
            "capabilities": []}),
    )
}

struct Case {
    name: &'static str,
    objective: &'static str,
    kind: &'static str,
    risk: &'static str,
    operations: &'static [&'static str],
    mutations: &'static [&'static str],
    reversibility: &'static str,
}

const CASES: [Case; 7] = [
    Case {
        name: "explore-low",
        objective: "Read a module and explain its role",
        kind: "explore",
        risk: "low",
        operations: &["read_only"],
        mutations: &[],
        reversibility: "reversible",
    },
    Case {
        name: "implement-low",
        objective: "Rename a variable in one file",
        kind: "implement",
        risk: "low",
        operations: &["workspace_change"],
        mutations: &["workspace_write"],
        reversibility: "reversible",
    },
    Case {
        name: "implement-medium",
        objective: "Fix a failing Rust test",
        kind: "implement",
        risk: "medium",
        operations: &["workspace_change"],
        mutations: &["workspace_write"],
        reversibility: "reversible",
    },
    Case {
        name: "review-medium",
        objective: "Review a 300 line pull request",
        kind: "review",
        risk: "medium",
        operations: &["read_only"],
        mutations: &[],
        reversibility: "reversible",
    },
    Case {
        name: "architecture-high",
        objective: "Review the storage architecture",
        kind: "architecture",
        risk: "high",
        operations: &["workspace_change"],
        mutations: &["workspace_write"],
        reversibility: "reversible",
    },
    Case {
        name: "security-high",
        objective: "Audit how secrets are handled",
        kind: "security",
        risk: "high",
        operations: &["read_only"],
        mutations: &[],
        reversibility: "reversible",
    },
    Case {
        name: "migration-critical",
        objective: "Migrate the SQLite schema to version 4",
        kind: "migration",
        risk: "critical",
        operations: &["data_migration"],
        mutations: &["workspace_write"],
        reversibility: "irreversible",
    },
];

#[derive(Clone, Copy, PartialEq)]
enum Variant {
    CodexUnknown,
    CodexTools,
    ClaudeFlat,
}
impl Variant {
    const ALL: [Variant; 3] = [Self::CodexUnknown, Self::CodexTools, Self::ClaudeFlat];
    fn name(self) -> &'static str {
        match self {
            Self::CodexUnknown => "codex-v3-capabilities-unknown",
            Self::CodexTools => "codex-v3-declared-tools",
            Self::ClaudeFlat => "claude-v1-flat-declared-tools",
        }
    }
}

fn task(case: &Case, required_capabilities: Value) -> Value {
    json!({
        "objective": case.objective,
        "kind": case.kind,
        "deliverables": [],
        "constraints": [],
        "scope": ["crates"],
        "risk": case.risk,
        "sensitivity": "internal",
        "budget": {"max_cost_microusd": 2000000, "max_elapsed_ms": 1800000, "max_tokens": 400000},
        "allowed_mutations": case.mutations,
        "dependencies": [],
        "acceptance": ["owner accepts the result"],
        "required_capabilities": required_capabilities
    })
}

/// Runs one case and returns the full `advise` report.
fn advise(dir: &Path, variant: Variant, case: &Case) -> Value {
    let work = dir.join(format!("{}-{}", variant.name(), case.name));
    fs::create_dir_all(&work).unwrap();
    let required = match variant {
        Variant::CodexUnknown => json!([]),
        _ => json!(["tools"]),
    };
    let task = write(&work.join("task.json"), &task(case, required));
    let context = write(
        &work.join("risk-context.json"),
        &json!({"schema_version": 1, "environment": "local",
            "reversibility": case.reversibility, "operations": case.operations}),
    );
    let mut cmd = command();
    cmd.args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(&context);
    match variant {
        Variant::CodexUnknown | Variant::CodexTools => {
            cmd.arg("--catalog")
                .arg(root("assets/catalogs/codex-core-v3.json"))
                .arg("--host-snapshot")
                .arg(discovered_snapshot(&work));
        }
        Variant::ClaudeFlat => {
            cmd.arg("--catalog")
                .arg(root("assets/catalogs/claude-core-v1.json"))
                .arg("--host")
                .arg(claude_host(&work));
        }
    }
    if variant != Variant::CodexUnknown {
        cmd.args(["--declare-capability", "tools"]);
    }
    let output = cmd.args(["--now", NOW, "--json"]).output().unwrap();
    parsed(&output.stdout, &output.stderr)
}

fn stages(report: &Value) -> Vec<Value> {
    report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Reviewable projection of a report: what routing decided, nothing volatile.
fn record(variant: Variant, case: &Case, report: &Value) -> Value {
    let stages: Vec<Value> = stages(report)
        .iter()
        .map(|stage| {
            json!({
                "stage": stage["kind"],
                "model": stage["model"],
                "effort": stage["effort"],
                "skills": stage["skills"],
            })
        })
        .collect();
    json!({
        "variant": variant.name(),
        "case": case.name,
        "kind": case.kind,
        "risk": case.risk,
        "status": report["status"],
        "stages": stages,
        "selection_basis": report["next_manual_action"]["selection_basis"],
    })
}

fn matrix(dir: &Path) -> Vec<(Variant, &'static Case, Value)> {
    Variant::ALL
        .into_iter()
        .flat_map(|variant| {
            CASES
                .iter()
                .map(move |case| (variant, case, advise(dir, variant, case)))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn golden_value(dir: &Path) -> Value {
    Value::Array(
        matrix(dir)
            .iter()
            .map(|(variant, case, report)| record(*variant, case, report))
            .collect(),
    )
}

fn render(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text
}

#[test]
fn advise_matrix_matches_golden_v1() {
    let dir = tempfile::tempdir().unwrap();
    let actual = render(&golden_value(dir.path()));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    // A missing golden compares as empty, so the failure prints the actual output.
    let golden = fs::read_to_string(&path).unwrap_or_default();
    // Compare-only, like the other goldens of this crate. To accept an intended
    // routing change, replace the golden with the printed actual output and
    // review the diff.
    assert!(
        actual == golden,
        "advise matrix moved from {GOLDEN}; if intended, replace the file with:\n{actual}"
    );
}

/// Distinct (model, effort, skills) stage tuples among the seven Codex cases
/// with capabilities unknown.
fn distinct_stage_tuples(dir: &Path) -> BTreeSet<String> {
    CASES
        .iter()
        .flat_map(|case| stages(&advise(dir, Variant::CodexUnknown, case)))
        .map(|stage| format!("{}|{}|{}", stage["model"], stage["effort"], stage["skills"]))
        .collect()
}

#[test]
fn advise_matrix_reports_distinct_plan_count() {
    let dir = tempfile::tempdir().unwrap();
    let tuples = distinct_stage_tuples(dir.path());
    // Later routing PRs raise this number explicitly.
    assert_eq!(tuples.len(), EXPECTED_DISTINCT_STAGE_TUPLES, "{tuples:#?}");
}

#[test]
fn advise_matrix_is_byte_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let first = render(&golden_value(&dir.path().join("a")));
    let second = render(&golden_value(&dir.path().join("b")));
    assert_eq!(first, second);
}

const EXPECTED_DISTINCT_STAGE_TUPLES: usize = 5;

// ---- Stable-v2 relations, pending #235. They fail on current routing. ----

fn risk_rank(risk: &str) -> u8 {
    match risk {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        other => panic!("unknown risk {other}"),
    }
}
fn effort_rank(effort: &str) -> u8 {
    ["low", "medium", "high", "xhigh", "max", "ultra"]
        .iter()
        .position(|candidate| *candidate == effort)
        .unwrap_or_else(|| panic!("unknown effort {effort}")) as u8
}
fn model_entry(model: &str) -> Value {
    let catalog: Value =
        serde_json::from_slice(&fs::read(root("assets/catalogs/codex-core-v3.json")).unwrap())
            .unwrap();
    catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == model)
        .unwrap_or_else(|| panic!("model {model} not in catalog"))
        .clone()
}
fn micros(model: &str, field: &str) -> u64 {
    model_entry(model)[field].as_u64().unwrap()
}
/// Codex cases with capabilities unknown, ordered from lowest to highest risk.
fn by_risk(dir: &Path) -> Vec<(&'static Case, Value)> {
    let mut rows: Vec<_> = CASES
        .iter()
        .map(|case| (case, advise(dir, Variant::CodexUnknown, case)))
        .collect();
    rows.sort_by_key(|(case, _)| risk_rank(case.risk));
    rows
}
fn stage_named(report: &Value, name: &str) -> Value {
    stages(report)
        .into_iter()
        .find(|stage| stage["kind"] == name)
        .unwrap_or_else(|| panic!("no stage {name}: {report:#}"))
}

#[test]
#[ignore = "pending stable-v2, see #235"]
fn matrix_each_stage_varies_with_risk_or_states_why() {
    let dir = tempfile::tempdir().unwrap();
    let rows = by_risk(dir.path());
    let (_, lowest) = rows.first().unwrap();
    let (_, highest) = rows.last().unwrap();
    // Expected future field: `scoring.reason_codes`, a non-empty list naming the
    // data that prevents variation (for example equal catalog costs).
    let states_why = highest["scoring"]["reason_codes"]
        .as_array()
        .is_some_and(|codes| !codes.is_empty());
    let (low, high) = (stages(lowest), stages(highest));
    assert_eq!(low.len(), high.len());
    for (low, high) in low.iter().zip(&high) {
        let varies = low["model"] != high["model"] || low["effort"] != high["effort"];
        assert!(
            varies || states_why,
            "stage {} is identical for lowest and highest risk with no reason code: {low}",
            low["kind"]
        );
    }
}

#[test]
#[ignore = "pending stable-v2, see #235"]
fn matrix_low_risk_execute_is_cheaper_than_critical() {
    let dir = tempfile::tempdir().unwrap();
    let rows = by_risk(dir.path());
    let cost = |report: &Value| {
        let stage = stage_named(report, "execute");
        let model = stage["model"].as_str().unwrap().to_owned();
        (
            micros(&model, "cost_micros"),
            effort_rank(stage["effort"].as_str().unwrap()),
        )
    };
    let low = cost(&rows.first().unwrap().1);
    let critical = cost(&rows.last().unwrap().1);
    assert!(
        low < critical,
        "execute (cost, effort) low={low:?} critical={critical:?}: low risk must be strictly cheaper"
    );
}

#[test]
#[ignore = "pending stable-v2, see #235"]
fn matrix_critical_uses_strongest_eligible_verifier() {
    let dir = tempfile::tempdir().unwrap();
    let rows = by_risk(dir.path());
    let verify = stage_named(&rows.last().unwrap().1, "verify");
    let chosen = micros(verify["model"].as_str().unwrap(), "quality_micros");
    // Eligible verifiers: catalog models that allow the verify stage and appear
    // in the synthetic host pool. Unrated models have quality 0 and never win.
    let pool = [
        "gpt-5.6-luna",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-6-astra",
        "gpt-6-luna",
        "gpt-6-sol",
        "gpt-6.1-sol",
        "gpt-6.1-terra",
    ];
    let strongest = pool
        .iter()
        .filter(|id| {
            model_entry(id)["stages"]
                .as_array()
                .unwrap()
                .contains(&json!("verify"))
        })
        .map(|id| micros(id, "quality_micros"))
        .max()
        .unwrap();
    assert_eq!(
        chosen, strongest,
        "critical verifier is not the strongest eligible one"
    );
}

#[test]
#[ignore = "pending stable-v2, see #235"]
fn matrix_effort_never_decreases_with_risk() {
    let dir = tempfile::tempdir().unwrap();
    let rows = by_risk(dir.path());
    let efforts = |report: &Value| -> Vec<u8> {
        stages(report)
            .iter()
            .map(|stage| effort_rank(stage["effort"].as_str().unwrap()))
            .collect()
    };
    // Weak monotonicity over every pair of cases with strictly ordered risk.
    for (a, ra) in &rows {
        for (b, rb) in &rows {
            if risk_rank(a.risk) < risk_rank(b.risk) {
                for (stage, (x, y)) in efforts(ra).iter().zip(efforts(rb)).enumerate() {
                    assert!(
                        y >= *x,
                        "stage {stage} effort decreases from {} to {}",
                        a.name,
                        b.name
                    );
                }
            }
        }
    }
    // A constant plan satisfies weak monotonicity, so v2 also needs a rise
    // somewhere between the lowest and the highest risk.
    let lowest = efforts(&rows.first().unwrap().1);
    let highest = efforts(&rows.last().unwrap().1);
    assert!(
        lowest.iter().zip(&highest).any(|(low, high)| high > low),
        "no stage raises its effort from the lowest to the highest risk: {lowest:?} vs {highest:?}"
    );
}
