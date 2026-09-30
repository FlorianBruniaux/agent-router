//! `schema`: every embedded output schema is structurally valid, and real
//! outputs of its contract conform to it. The checker is a small local one (see
//! `support/json_schema.rs`), not a full JSON Schema validator.
#[path = "support/json_schema.rs"]
mod json_schema;

use assert_cmd::Command;
use router_eval::{PricingTable, TokenPrice};
use router_feedback::{FeedbackStore, redacted_digest};
use router_types::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const NAMES: [&str; 5] = [
    "error",
    "feedback-report",
    "feedback-summary",
    "route-decision",
    "version",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("agent-router"))
}
fn stdout(mut command: Command) -> Vec<u8> {
    command.assert().success().get_output().stdout.clone()
}
fn schema(name: &str) -> Value {
    let mut cmd = command();
    cmd.args(["schema", name]);
    serde_json::from_slice(&stdout(cmd)).unwrap()
}
fn assert_conforms(name: &str, value: &Value) {
    let found = json_schema::violations(&schema(name), value);
    assert!(found.is_empty(), "{name}: {found:#?}\n{value}");
}

#[test]
fn list_names_every_contract_with_its_supported_versions() {
    let mut cmd = command();
    cmd.args(["schema", "--list"]);
    let out = stdout(cmd);
    let listing: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(listing["schema_version"], 1);
    let entries = listing["schemas"].as_array().unwrap();
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, NAMES);
    // The decision envelope carries a string version, every other output a number.
    let versions =
        |name: &str| entries.iter().find(|e| e["name"] == name).unwrap()["schema_versions"].clone();
    assert_eq!(versions("route-decision"), json!(["1"]));
    assert_eq!(versions("feedback-report"), json!([1]));
    let mut again = command();
    again.args(["schema", "--list"]);
    assert_eq!(out, stdout(again));
}

#[test]
fn every_listed_schema_is_structurally_valid_and_named_by_its_id() {
    for name in NAMES {
        let document = schema(name);
        json_schema::check_schema(&document);
        let id = document["$id"].as_str().unwrap();
        assert!(
            id.starts_with(&format!("urn:agent-router:schema:{name}:")),
            "{name}: {id}"
        );
    }
}

#[test]
fn schema_output_is_the_embedded_file_and_byte_identical() {
    for name in NAMES {
        let mut first = command();
        first.args(["schema", name]);
        let mut second = command();
        second.args(["schema", name]);
        let bytes = stdout(first);
        assert_eq!(bytes, stdout(second));
        let file = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/schemas")
                .join(format!("{name}.schema.json")),
        )
        .unwrap();
        assert_eq!(bytes, file, "{name}");
    }
}

#[test]
fn unknown_or_missing_names_are_refused_with_stable_errors() {
    command()
        .args(["schema", "nope"])
        .assert()
        .code(2)
        .stderr("error: schema.unknown\n");
    command()
        .args(["schema", "nope", "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"schema.unknown\"}\n");
    command()
        .arg("schema")
        .assert()
        .code(2)
        .stderr(predicates::str::starts_with("error: cli.invalid\n"));
    command()
        .args(["schema", "error", "--list"])
        .assert()
        .code(2)
        .stderr(predicates::str::starts_with("error: cli.invalid\n"));
}

fn route(request: &str) -> (Value, i32) {
    let output = command()
        .args(["route", "--request"])
        .arg(root().join("tests/fixtures/requests").join(request))
        .arg("--catalog")
        .arg(root().join("assets/catalogs/codex-core-v1.json"))
        .arg("--json")
        .output()
        .unwrap();
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        output.status.code().unwrap(),
    )
}

#[test]
fn route_decisions_conform_to_the_route_decision_schema() {
    let (selected, code) = route("codex-implementation.json");
    assert_eq!(code, 0);
    assert!(selected["decision"]["selected"].is_object());
    assert_conforms("route-decision", &selected);
    let (no_route, code) = route("no-compatible-plan.json");
    assert_eq!(code, 3);
    assert!(no_route["decision"]["no_route"].is_object());
    assert_conforms("route-decision", &no_route);
    let stored: Value = serde_json::from_slice(
        &fs::read(root().join("tests/fixtures/decisions/codex-implementation.json")).unwrap(),
    )
    .unwrap();
    assert_conforms("route-decision", &stored);
}

#[test]
fn error_lines_conform_to_the_error_schema() {
    for args in [
        vec!["schema", "nope", "--json"],
        vec!["bogus-command", "--json"],
        vec!["route", "--json"],
    ] {
        let output = command().args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?}");
        let line: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_conforms("error", &line);
    }
}

#[test]
fn version_output_conforms_to_the_version_schema() {
    let mut cmd = command();
    cmd.args(["version", "--json"]);
    assert_conforms("version", &serde_json::from_slice(&stdout(cmd)).unwrap());
}

fn key(c: char) -> String {
    c.to_string().repeat(64)
}
fn automated(run: &str, run_key: char, model: &str) -> FeedbackInput {
    FeedbackInput {
        schema_version: 1,
        observed_at_unix_seconds: 130,
        started_at_unix_seconds: Some(120),
        run_key_sha256: Some(key(run_key)),
        binding: FeedbackBinding {
            task_hash: "a".repeat(64),
            run_hash: redacted_digest(run).unwrap(),
            receipt_hash: "b".repeat(64),
            policy_hash: "c".repeat(64),
            catalog_hash: "d".repeat(64),
            snapshot_hash: "e".repeat(64),
            producer_version: "synthetic-v1".into(),
            provenance_sha256: "f".repeat(64),
        },
        context: FeedbackContext {
            task_kind: TaskKind::Explore,
            risk: RiskLevel::Low,
            model: ModelId::parse(model).unwrap(),
            effort: EffortLevel::Medium,
            harness: HarnessId::parse("fixture-host").unwrap(),
            transport: "subprocess-jsonl".into(),
            agent: None,
            skills: BTreeSet::new(),
            plan: "single-pass".into(),
            propensity_micros: None,
            route: None,
        },
        origin: FeedbackOrigin::Automated,
        transport: VerificationOutcome::Passed,
        validation: VerificationOutcome::Unknown,
        objective: VerificationOutcome::Unknown,
        verification_evidence_sha256: None,
        human: None,
        failure: None,
        measurements: FeedbackMeasurements {
            cost_microusd: None,
            latency_ms: Some(100),
            tokens: Some(1500),
            attempts: 1,
            escalations: 0,
            usage: Some(HostUsage {
                input_tokens: UsageField::host_reported(1000),
                cached_input_tokens: UsageField::host_reported(0),
                cache_write_input_tokens: UsageField::UNKNOWN,
                output_tokens: UsageField::host_reported(500),
                reasoning_output_tokens: UsageField::UNKNOWN,
            }),
        },
        comment_sha256: None,
    }
}
fn fact(run_key: char, kind: FeedbackFactKind, at: u64) -> FeedbackFact {
    FeedbackFact {
        schema_version: 1,
        run_key_sha256: key(run_key),
        observed_at_unix_seconds: at,
        kind,
    }
}

/// A store holding accepted, rejected and unknown runs over two models, built
/// with the library because the CLI records no late fact.
fn populated_store(directory: &Path) -> PathBuf {
    let path = directory.join("feedback");
    let store = FeedbackStore::open(&path).unwrap();
    store
        .configure(
            FeedbackPreferences {
                collection_enabled: true,
                learning_enabled: true,
                ..Default::default()
            },
            100,
        )
        .unwrap();
    for (run, run_key, model) in [
        ("r1", '1', "model-a"),
        ("r2", '2', "model-a"),
        ("r3", '3', "model-b"),
    ] {
        store.record(automated(run, run_key, model), 200).unwrap();
    }
    let passed = |c| {
        fact(
            c,
            FeedbackFactKind::Verification {
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
                evidence_sha256: "9".repeat(64),
            },
            250,
        )
    };
    let applied = |c, at| {
        fact(
            c,
            FeedbackFactKind::Integration {
                outcome: IntegrationOutcome::Applied,
            },
            at,
        )
    };
    let discarded = fact(
        '2',
        FeedbackFactKind::Integration {
            outcome: IntegrationOutcome::Discarded,
        },
        260,
    );
    for f in [passed('1'), applied('1', 260), discarded, passed('3')] {
        store.record_fact(f, 270).unwrap();
    }
    path
}

fn write_pricing(directory: &Path) -> PathBuf {
    let pricing = directory.join("pricing.json");
    let table = PricingTable {
        schema_version: 1,
        version: "schema-test-v1".into(),
        source: "synthetic".into(),
        observed_at_unix_seconds: 90,
        models: [(
            ModelId::parse("model-a").unwrap(),
            TokenPrice {
                input_microusd_per_million: 3_000_000,
                cached_input_microusd_per_million: 300_000,
                output_microusd_per_million: 15_000_000,
            },
        )]
        .into(),
    };
    fs::write(&pricing, serde_json::to_vec(&table).unwrap()).unwrap();
    pricing
}

fn feedback_json(store: &Path, subcommand: &str, pricing: Option<&Path>) -> Value {
    let mut cmd = command();
    cmd.args(["feedback", "--store"])
        .arg(store)
        .arg(subcommand)
        .args(["--now", "300", "--json"]);
    if let Some(pricing) = pricing {
        cmd.arg("--pricing").arg(pricing);
    }
    serde_json::from_slice(&stdout(cmd)).unwrap()
}

#[test]
fn feedback_report_and_summary_conform_to_their_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let directory = fs::canonicalize(dir.path()).unwrap();
    let store = populated_store(&directory);
    let pricing = write_pricing(&directory);
    for priced in [false, true] {
        let pricing = priced.then_some(pricing.as_path());
        let report = feedback_json(&store, "report", pricing);
        assert_eq!(report["groups"].as_array().unwrap().len(), 2);
        assert_conforms("feedback-report", &report);
        let summary = feedback_json(&store, "summary", pricing);
        assert_eq!(summary["runs"], 3);
        assert_eq!(summary["accepted"], 1);
        assert_eq!(summary["rejected"], 1);
        assert_eq!(summary["unknown"], 1);
        assert!(summary["accepted_rate"].is_object());
        assert_conforms("feedback-summary", &summary);
    }
    // The priced summary has a total and, model-b being unpriced, no per-accepted figure.
    let summary = feedback_json(&store, "summary", Some(&pricing));
    assert!(summary["cost"]["total_microusd"].is_u64());
    assert!(summary["cost"]["per_accepted_microusd"].is_null());
    assert_eq!(summary["cost"]["reason"], "cost_unknown_runs");
}

/// Learning disabled, a human event, and a group whose runs have no verdict.
/// With `with_accepted` a second group holds one accepted run.
fn undecided_store(directory: &Path, with_accepted: bool) -> PathBuf {
    let path = directory.join("feedback");
    let store = FeedbackStore::open(&path).unwrap();
    store
        .configure(
            FeedbackPreferences {
                collection_enabled: true,
                learning_enabled: false,
                ..Default::default()
            },
            100,
        )
        .unwrap();
    let pending = automated("pending", '4', "model-c");
    store.record(pending.clone(), 200).unwrap();
    let mut human = pending;
    human.origin = FeedbackOrigin::Human;
    human.human = Some(HumanVerdict {
        success: VerificationOutcome::Passed,
        quality: Some(80),
        perceived_cost: PerceivedCost::Low,
        correction_required: None,
        reviewer_sha256: "2".repeat(64),
    });
    store.record(human, 200).unwrap();
    if with_accepted {
        store
            .record(automated("done", '1', "model-a"), 200)
            .unwrap();
        for f in [
            fact(
                '1',
                FeedbackFactKind::Verification {
                    validation: VerificationOutcome::Passed,
                    objective: VerificationOutcome::Passed,
                    evidence_sha256: "9".repeat(64),
                },
                250,
            ),
            fact(
                '1',
                FeedbackFactKind::Integration {
                    outcome: IntegrationOutcome::Applied,
                },
                260,
            ),
        ] {
            store.record_fact(f, 270).unwrap();
        }
    }
    path
}

#[test]
fn feedback_outputs_with_null_figures_and_disabled_learning_conform() {
    for with_accepted in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let directory = fs::canonicalize(dir.path()).unwrap();
        let store = undecided_store(&directory, with_accepted);
        let report = feedback_json(&store, "report", None);
        assert_eq!(report["human_events"], 1);
        assert!(report["advisory"]["advisory"].is_null());
        assert_eq!(report["advisory"]["reason"], "learning_disabled");
        let group = report["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["model"] == "model-c")
            .unwrap();
        assert!(group["accepted_rate"].is_null());
        assert!(group["time_to_acceptance_seconds"]["p50"].is_null());
        assert_eq!(group["human"]["passed"], 1);
        assert_conforms("feedback-report", &report);
        let summary = feedback_json(&store, "summary", None);
        assert_eq!(summary["human"]["passed"], 1);
        assert_eq!(summary["accepted_rate"].is_null(), !with_accepted);
        assert_eq!(
            summary["time_to_acceptance_seconds"]["p50"].is_null(),
            !with_accepted
        );
        assert_conforms("feedback-summary", &summary);
    }
}

#[test]
fn the_feedback_schemas_reject_outputs_that_break_their_contract() {
    let dir = tempfile::tempdir().unwrap();
    let directory = fs::canonicalize(dir.path()).unwrap();
    let store = populated_store(&directory);
    let pricing = write_pricing(&directory);
    for (name, subcommand) in [
        ("feedback-report", "report"),
        ("feedback-summary", "summary"),
    ] {
        let real = feedback_json(&store, subcommand, Some(&pricing));
        assert_conforms(name, &real);
        let broken = |mutate: &dyn Fn(&mut Value)| {
            let mut value = real.clone();
            mutate(&mut value);
            json_schema::violations(&schema(name), &value)
        };
        assert!(
            !broken(&|v| v["extra"] = json!(1)).is_empty(),
            "{name}: extra key"
        );
        assert!(
            !broken(&|v| v["schema_version"] = json!(2)).is_empty(),
            "{name}: version"
        );
        assert!(
            !broken(&|v| drop(v.as_object_mut().unwrap().remove("content_hash"))).is_empty(),
            "{name}: missing field"
        );
        assert!(
            !broken(&|v| v["content_hash"] = json!("short")).is_empty(),
            "{name}: hash length"
        );
        assert!(
            !broken(&|v| v["cost_placeholder"] = json!(null)).is_empty(),
            "{name}: unknown key"
        );
    }
    let report = feedback_json(&store, "report", Some(&pricing));
    let mut nested = report.clone();
    nested["groups"][0]["cost"]["total_microusd"] = json!("9");
    assert!(!json_schema::violations(&schema("feedback-report"), &nested).is_empty());
    let mut nested = report;
    nested["groups"][0]["accepted"] = json!(-1);
    assert!(!json_schema::violations(&schema("feedback-report"), &nested).is_empty());
    let mut summary = feedback_json(&store, "summary", Some(&pricing));
    summary["attribution"] = json!("established");
    assert!(!json_schema::violations(&schema("feedback-summary"), &summary).is_empty());
    let mut summary = feedback_json(&store, "summary", Some(&pricing));
    summary["unknown_counts"]["verdict"] = json!(null);
    assert!(!json_schema::violations(&schema("feedback-summary"), &summary).is_empty());
}

#[test]
fn the_checker_rejects_outputs_that_break_their_contract() {
    let (selected, _) = route("codex-implementation.json");
    let broken = |mutate: &dyn Fn(&mut Value)| {
        let mut value = selected.clone();
        mutate(&mut value);
        json_schema::violations(&schema("route-decision"), &value)
    };
    assert!(!broken(&|v| v["extra"] = json!(1)).is_empty());
    assert!(!broken(&|v| v["schema_version"] = json!(1)).is_empty());
    assert!(!broken(&|v| drop(v.as_object_mut().unwrap().remove("decision_id"))).is_empty());
    assert!(!broken(&|v| v["decision"] = json!({"selected": {}, "no_route": {}})).is_empty());
    let error = json_schema::violations(&schema("error"), &json!({"schema_version": 1}));
    assert!(!error.is_empty());
    let version = json_schema::violations(
        &schema("version"),
        &json!({"schema_version":1,"version":"1","target":"t","features":[],"contracts":{"x":{"command":"c","schema_versions":[]}}}),
    );
    assert!(!version.is_empty());
}
