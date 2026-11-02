// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! ADR 0054 acceptance for the compiled `stable-v2` advisory policy.
use std::{fs, path::PathBuf};

use router_catalog::Catalog;
use router_policy::{EffortProvenance, Router, StageScoring};
use router_types::{EffortLevel, RiskLevel, RouteDecision, RouteRequest, StageKind, TaskKind};
use serde_json::Value;

const CATALOGS: [&str; 4] = [
    "claude-core-v1",
    "codex-core-v1",
    "codex-core-v2",
    "codex-core-v3",
];
const RISKS: [RiskLevel; 4] = [
    RiskLevel::Low,
    RiskLevel::Medium,
    RiskLevel::High,
    RiskLevel::Critical,
];
const KINDS: [TaskKind; 6] = [
    TaskKind::Explore,
    TaskKind::Implement,
    TaskKind::Review,
    TaskKind::Architecture,
    TaskKind::Security,
    TaskKind::Migration,
];

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("router-policy crate is nested beneath the workspace root")
        .join(path)
}

fn catalog_json(name: &str) -> Value {
    serde_json::from_slice(&fs::read(root(&format!("assets/catalogs/{name}.json"))).unwrap())
        .unwrap()
}

fn catalog_from(value: &Value) -> Catalog {
    Catalog::from_json(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn requests() -> Vec<(String, RouteRequest)> {
    let mut paths = fs::read_dir(root("tests/fixtures/requests"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let request = serde_json::from_slice(&fs::read(&path).unwrap()).ok()?;
            Some((path.file_name()?.to_string_lossy().into_owned(), request))
        })
        .collect()
}

/// Highest quality of any catalog model that lists `stage`: an upper bound of
/// the eligible maximum, whatever the host and the constraints remove.
fn catalog_max_quality(catalog: &Value, stage: StageKind) -> i64 {
    let stage = serde_json::to_value(stage).unwrap();
    catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|model| model["stages"].as_array().unwrap().contains(&stage))
        .map(|model| model["quality_micros"].as_i64().unwrap())
        .max()
        .unwrap()
}

fn rescaled(name: &str, divisor: i64) -> Value {
    let mut catalog = catalog_json(name);
    for model in catalog["models"].as_array_mut().unwrap() {
        let quality = model["quality_micros"].as_i64().unwrap() / divisor;
        model["quality_micros"] = quality.into();
    }
    catalog
}

fn with_task(request: &RouteRequest, kind: TaskKind, risk: RiskLevel) -> RouteRequest {
    let mut request = request.clone();
    request.task.kind = kind;
    request.task.risk = risk;
    request
}

#[test]
fn v2_targets_never_exceed_max_eligible_quality() {
    let mut checked = 0;
    // The rescaled catalog puts every quality below the largest margin, so the
    // cap and the floor at zero both bind.
    let mut catalogs = CATALOGS
        .iter()
        .map(|name| (name.to_string(), catalog_json(name)))
        .collect::<Vec<_>>();
    catalogs.push(("codex-core-v3/20".into(), rescaled("codex-core-v3", 20)));
    for (name, value) in &catalogs {
        let catalog = catalog_from(value);
        for (file, base) in requests() {
            for kind in KINDS {
                for risk in RISKS {
                    let request = with_task(&base, kind, risk);
                    let scored = Router::stable_v2()
                        .route_scored(&request, &catalog)
                        .unwrap();
                    let Some(scoring) = scored.scoring else {
                        assert!(scored.decision.selected_plan().is_none());
                        continue;
                    };
                    assert_eq!(scoring.stages.len(), 3);
                    for stage in &scoring.stages {
                        let ceiling = catalog_max_quality(value, stage.stage);
                        let context = format!("{file}@{name} {kind:?}/{risk:?} {:?}", stage.stage);
                        assert!(stage.target_micros >= 0, "{context}");
                        assert!(
                            stage.target_micros <= ceiling,
                            "{context}: target {} above catalog maximum {ceiling}",
                            stage.target_micros
                        );
                        assert_eq!(
                            stage.shortfall_micros,
                            (stage.target_micros - stage.quality_micros).max(0),
                            "{context}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 500, "only {checked} stage targets were checked");
}

#[test]
fn v2_unrated_catalog_gets_zero_targets_and_the_unrated_code() {
    let mut value = catalog_json("codex-core-v3");
    for model in value["models"].as_array_mut().unwrap() {
        model["quality_micros"] = 0.into();
    }
    let catalog = catalog_from(&value);
    let base = requests()
        .into_iter()
        .find(|(file, _)| file == "codex-implementation.json")
        .unwrap()
        .1;
    let scored = Router::stable_v2().route_scored(&base, &catalog).unwrap();
    let scoring = scored.scoring.expect("a plan is selected");
    for stage in &scoring.stages {
        assert_eq!(stage.target_micros, 0);
        assert_eq!(stage.shortfall_micros, 0);
        assert!(has(stage, "catalog.unrated"), "{stage:?}");
    }
    assert!(scoring.reason_codes.contains(&"catalog.unrated".to_owned()));
}

fn has(stage: &StageScoring, code: &str) -> bool {
    stage.reason_codes.iter().any(|candidate| candidate == code)
}

#[test]
fn v2_decision_id_differs_from_v1() {
    let mut compared = 0;
    for name in CATALOGS {
        let catalog = catalog_from(&catalog_json(name));
        for (file, request) in requests() {
            let v1 = Router::stable_v1().route(&request, &catalog);
            let v2 = Router::stable_v2().route(&request, &catalog);
            let (Ok(v1), Ok(v2)) = (v1, v2) else { continue };
            assert_eq!(v1.policy_version, "stable-v1");
            assert_eq!(v2.policy_version, "stable-v2");
            assert_eq!(v2.schema_version, "1", "{file}@{name}");
            assert_ne!(v1.decision_id, v2.decision_id, "{file}@{name}");
            compared += 1;
        }
    }
    assert!(compared > 20, "only {compared} requests were compared");
}

#[test]
fn v2_is_deterministic_and_route_matches_route_scored() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    let router = Router::stable_v2();
    for (file, request) in requests() {
        let first = router.route_scored(&request, &catalog).unwrap();
        let second = router.route_scored(&request, &catalog).unwrap();
        assert_eq!(
            serde_json::to_vec(&first.decision).unwrap(),
            serde_json::to_vec(&second.decision).unwrap(),
            "{file}"
        );
        assert_eq!(
            serde_json::to_vec(&first.decision).unwrap(),
            serde_json::to_vec(&router.route(&request, &catalog).unwrap()).unwrap(),
            "{file}: route and route_scored disagree"
        );
        assert_eq!(first.scoring, second.scoring, "{file}");
    }
}

#[test]
fn v1_route_scored_has_no_scoring_object() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    for (_, request) in requests() {
        let scored = Router::stable_v1()
            .route_scored(&request, &catalog)
            .unwrap();
        assert!(scored.scoring.is_none());
        assert_eq!(
            serde_json::to_vec(&scored.decision).unwrap(),
            serde_json::to_vec(&Router::stable_v1().route(&request, &catalog).unwrap()).unwrap()
        );
    }
}

#[test]
fn v2_critical_requires_the_strongest_eligible_verifier_with_its_exclusion_code() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    let base = requests()
        .into_iter()
        .find(|(file, _)| file == "codex-implementation.json")
        .unwrap()
        .1;
    let request = with_task(&base, TaskKind::Implement, RiskLevel::Critical);
    let RouteDecision::Selected { exclusions, .. } = Router::stable_v2()
        .route(&request, &catalog)
        .unwrap()
        .decision
    else {
        panic!("a plan is selected");
    };
    assert!(
        exclusions
            .iter()
            .any(|exclusion| exclusion.code == "verification.critical.not_max_quality"),
        "{exclusions:?}"
    );
    // The same request under v1 never reports the code: v1 is frozen.
    let RouteDecision::Selected { exclusions, .. } = Router::stable_v1()
        .route(&request, &catalog)
        .unwrap()
        .decision
    else {
        panic!("a plan is selected");
    };
    assert!(
        exclusions
            .iter()
            .all(|exclusion| exclusion.code != "verification.critical.not_max_quality")
    );
}

#[test]
fn v2_utility_is_minus_total_shortfall() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    for (file, base) in requests() {
        for risk in RISKS {
            let request = with_task(&base, TaskKind::Implement, risk);
            let scored = Router::stable_v2()
                .route_scored(&request, &catalog)
                .unwrap();
            let (Some(plan), Some(scoring)) = (scored.decision.selected_plan(), scored.scoring)
            else {
                continue;
            };
            let total: i64 = scoring.stages.iter().map(|s| s.shortfall_micros).sum();
            assert_eq!(plan.utility_micros, -total, "{file} {risk:?}");
        }
    }
}

#[test]
fn v2_low_risk_execute_uses_the_cheap_rated_model_and_critical_does_not() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    let base = requests()
        .into_iter()
        .find(|(file, _)| file == "codex-exploration.json")
        .unwrap()
        .1;
    let execute_model = |risk| {
        let request = with_task(&base, TaskKind::Implement, risk);
        let decision = Router::stable_v2().route(&request, &catalog).unwrap();
        decision.selected_plan().unwrap().stages[1]
            .model
            .to_string()
    };
    assert_eq!(execute_model(RiskLevel::Low), "gpt-5.6-luna");
    assert_ne!(execute_model(RiskLevel::Critical), "gpt-5.6-luna");
}

fn with_efforts(request: &RouteRequest, efforts: &[&str]) -> RouteRequest {
    let mut value = serde_json::to_value(request).unwrap();
    for supported in value["host"]["supported_efforts"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        *supported = serde_json::json!(efforts);
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn effort_falls_back_to_nearest_supported_level() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    let base = requests()
        .into_iter()
        .find(|(file, _)| file == "codex-exploration.json")
        .unwrap()
        .1;
    // Low-risk implement prefers plan medium, execute low, verify medium. A host
    // that offers only `low` and `xhigh` gets `low` where the table asks for
    // medium (higher neighbour high is absent, then the lower neighbour).
    let request = with_efforts(
        &with_task(&base, TaskKind::Implement, RiskLevel::Low),
        &["low", "xhigh"],
    );
    let scored = Router::stable_v2()
        .route_scored(&request, &catalog)
        .unwrap();
    let plan = scored.decision.selected_plan().unwrap();
    let scoring = scored.scoring.unwrap();
    let efforts: Vec<_> = plan.stages.iter().map(|stage| stage.effort).collect();
    assert_eq!(
        efforts,
        [EffortLevel::Low, EffortLevel::Low, EffortLevel::Low]
    );
    let rows: Vec<_> = scoring
        .stages
        .iter()
        .map(|stage| (stage.effort_target, stage.effort_provenance))
        .collect();
    assert_eq!(
        rows,
        [
            (EffortLevel::Medium, EffortProvenance::Fallback),
            (EffortLevel::Low, EffortProvenance::PolicyTable),
            (EffortLevel::Medium, EffortProvenance::Fallback),
        ]
    );
    // The higher neighbour wins over the lower one when it exists.
    let request = with_efforts(
        &with_task(&base, TaskKind::Implement, RiskLevel::Medium),
        &["low", "xhigh"],
    );
    let plan = Router::stable_v2()
        .route(&request, &catalog)
        .unwrap()
        .selected_plan()
        .unwrap()
        .clone();
    // Medium risk plan target is high: high is absent, xhigh is the higher neighbour.
    assert_eq!(plan.stages[0].effort, EffortLevel::Xhigh);
}

#[test]
fn scoring_declares_effort_as_a_prior_and_its_cost_as_not_modelled() {
    let catalog = catalog_from(&catalog_json("codex-core-v3"));
    let base = requests()
        .into_iter()
        .find(|(file, _)| file == "codex-exploration.json")
        .unwrap()
        .1;
    // The fixture host offers a narrow effort set per model; widen it so the
    // table target is always available.
    let request = with_efforts(
        &with_task(&base, TaskKind::Implement, RiskLevel::High),
        &["low", "medium", "high", "xhigh", "max"],
    );
    let scoring = Router::stable_v2()
        .route_scored(&request, &catalog)
        .unwrap()
        .scoring
        .unwrap();
    assert_eq!(scoring.effort_basis, "declared_policy_prior");
    assert_eq!(scoring.effort_cost, "not_modelled");
    assert_eq!(scoring.table_digest.len(), 64);
    for stage in &scoring.stages {
        assert_eq!(stage.effort_provenance, EffortProvenance::PolicyTable);
        assert_eq!(stage.effort, stage.effort_target);
    }
}
