use std::collections::{BTreeMap, BTreeSet};

use router_catalog::Catalog;
use router_policy::Router;
use router_types::{
    Capability, CapabilitySupport, EffortLevel, HarnessId, HostSnapshot, ModelId, RiskLevel,
    RouteDecision, RouteRequest, SkillCandidate, SkillId, StageKind, TaskContext, TaskKind,
    VerificationIndependence,
};

#[test]
fn codex_implementation_uses_sol_plan_terra_execute_and_terra_verify() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-implementation.json");

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();

    assert_stage(
        &plan.stages[0],
        StageKind::Plan,
        "gpt-5.6-sol",
        EffortLevel::High,
    );
    assert_stage(
        &plan.stages[1],
        StageKind::Execute,
        "gpt-5.6-terra",
        EffortLevel::Medium,
    );
    assert_stage(
        &plan.stages[2],
        StageKind::Verify,
        "gpt-5.6-terra",
        EffortLevel::High,
    );
}

#[test]
fn effort_fallback_uses_fixed_nearest_available_order_without_duplicates() {
    let catalog = load_builtin_catalog();
    let mut request = load_request("codex-implementation.json");
    request.host.supported_efforts.insert(
        ModelId::parse("gpt-5.6-sol").unwrap(),
        BTreeSet::from([EffortLevel::Xhigh, EffortLevel::Max]),
    );

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();

    assert_eq!(plan.stages[0].effort, EffortLevel::Xhigh);
    assert!(decision.exclusions().iter().any(|exclusion| {
        exclusion.stage == StageKind::Plan
            && exclusion.model.as_str() == "gpt-5.6-sol"
            && exclusion.code == "host.effort.unsupported"
    }));
}

#[test]
fn exact_score_and_cost_ties_use_lexicographic_stage_model_ids() {
    let catalog = equal_prior_catalog();
    let request = equal_prior_request();

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();

    assert_eq!(
        plan.stages
            .iter()
            .map(|stage| stage.model.as_str())
            .collect::<Vec<_>>(),
        vec!["provider-model-a", "provider-model-a", "provider-model-a"]
    );
}

#[test]
fn high_risk_security_uses_fresh_verifier_with_at_least_execute_quality() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-security.json");

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();
    let execute = &plan.stages[1];
    let verify = &plan.stages[2];

    assert_eq!(plan.stages[0].model.as_str(), "gpt-5.6-sol");
    assert_eq!(execute.model.as_str(), "gpt-5.6-sol");
    assert_eq!(verify.model.as_str(), "gpt-5.6-sol");
    assert_eq!(
        verify.verification_independence,
        Some(VerificationIndependence::FreshInstance)
    );
    assert!(
        catalog.model(&verify.model).unwrap().quality_micros
            >= catalog.model(&execute.model).unwrap().quality_micros
    );
    assert!(
        plan.stages
            .iter()
            .flat_map(|stage| &stage.skills)
            .any(|skill| skill.as_str() == "security-guardian")
    );
    assert!(
        decision
            .exclusions()
            .iter()
            .filter(|exclusion| {
                exclusion.model.as_str() == "gpt-5.6-luna"
                    && exclusion.code == "model.stage.unsupported"
            })
            .count()
            >= 3
    );
    assert!(decision.exclusions().iter().any(|exclusion| {
        exclusion.model.as_str() == "gpt-5.6-luna"
            && exclusion.code == "model.capability.unsupported"
    }));
    assert!(decision.exclusions().iter().any(|exclusion| {
        exclusion.model.as_str() == "gpt-5.6-terra"
            && exclusion.code == "verification.security.not_max_quality"
    }));
}

#[test]
fn security_uses_highest_quality_eligible_verifier_when_sol_is_unavailable() {
    let catalog = security_fallback_catalog();
    let request = security_fallback_request();

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let verify = &decision.selected_plan().unwrap().stages[2];

    assert_eq!(verify.model.as_str(), "provider-security-fallback-high");
    assert_eq!(
        verify.verification_independence,
        Some(VerificationIndependence::FreshInstance)
    );
    assert!(decision.exclusions().iter().any(|exclusion| {
        exclusion.model.as_str() == "provider-security-fallback-low"
            && exclusion.code == "verification.security.not_max_quality"
    }));
}

#[test]
fn high_risk_architecture_uses_the_highest_quality_eligible_verifier() {
    let catalog = load_builtin_catalog();
    let request: RouteRequest = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/requests/codex-architecture.json"
    ))
    .unwrap();

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();

    assert_stage(
        &plan.stages[0],
        StageKind::Plan,
        "gpt-5.6-sol",
        EffortLevel::High,
    );
    assert_stage(
        &plan.stages[1],
        StageKind::Execute,
        "gpt-5.6-terra",
        EffortLevel::Medium,
    );
    assert_stage(
        &plan.stages[2],
        StageKind::Verify,
        "gpt-5.6-sol",
        EffortLevel::High,
    );
    assert!(decision.exclusions().iter().any(|exclusion| {
        exclusion.stage == StageKind::Verify
            && exclusion.model.as_str() == "gpt-5.6-terra"
            && exclusion.code == "verification.architecture.not_max_quality"
    }));
}

#[test]
fn missing_plan_stage_returns_no_route_instead_of_a_partial_plan() {
    let catalog = load_builtin_catalog();
    let request = load_request("no-compatible-plan.json");

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    assert!(matches!(decision.decision, RouteDecision::NoRoute { .. }));
    assert!(decision.selected_plan().is_none());
    assert!(!decision.exclusions().is_empty());
}

#[test]
fn identical_inputs_serialize_to_identical_decision_bytes() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-implementation.json");
    let router = Router::stable_v1();

    let first = serde_json::to_vec(&router.route(&request, &catalog).unwrap()).unwrap();
    let second = serde_json::to_vec(&router.route(&request, &catalog).unwrap()).unwrap();

    assert_eq!(first, second);
}

#[test]
fn selected_route_keeps_at_most_two_sorted_alternatives() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-implementation.json");
    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    let RouteDecision::Selected { alternatives, .. } = decision.decision else {
        panic!("expected selected route");
    };
    assert!(alternatives.len() <= 2);
    assert!(
        alternatives
            .windows(2)
            .all(|plans| plans[0].utility_micros >= plans[1].utility_micros)
    );
}

#[test]
fn exclusions_are_sorted_by_stage_model_and_code() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-security.json");
    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    assert!(decision.exclusions().windows(2).all(|pair| {
        (pair[0].stage, &pair[0].model, &pair[0].code)
            <= (pair[1].stage, &pair[1].model, &pair[1].code)
    }));
}

#[test]
fn cost_sum_uses_wide_arithmetic_without_panicking() {
    let catalog = extreme_catalog(i64::MAX, 0, &[]);
    let request = extreme_request(&[]);

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    assert_eq!(
        decision.selected_plan().unwrap().utility_micros,
        -6_917_529_027_640_081_855
    );
}

#[test]
fn latency_sum_uses_wide_arithmetic_without_panicking() {
    let catalog = extreme_catalog(0, i64::MAX, &[]);
    let request = extreme_request(&[]);

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    assert_eq!(
        decision.selected_plan().unwrap().utility_micros,
        -2_767_011_611_055_432_742
    );
}

#[test]
fn retrieved_skill_sum_uses_wide_arithmetic_without_panicking() {
    let catalog = extreme_catalog(0, 0, &["limit-skill-a", "limit-skill-b"]);
    let request = extreme_request(&["limit-skill-a", "limit-skill-b"]);

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    assert_eq!(
        decision.selected_plan().unwrap().utility_micros,
        922_337_203_686_477_580
    );
}

#[test]
fn final_utility_out_of_i64_range_returns_typed_error() {
    let catalog = extreme_catalog(i64::MAX, i64::MAX, &[]);
    let request = extreme_request(&[]);

    let error = Router::stable_v1().route(&request, &catalog).unwrap_err();

    assert_eq!(error.code(), "route.arithmetic.out_of_range");
    assert_eq!(error.component(), Some("utility_micros"));
}

#[test]
fn explicit_unknown_capability_fails_closed_and_deduplicates_missing_evidence() {
    let catalog = load_builtin_catalog();
    let mut request = load_request("codex-implementation.json");
    request.task.risk = RiskLevel::High;
    request
        .capability_knowledge
        .insert(Capability::Tools, CapabilitySupport::Unknown);

    let decision = Router::stable_v1().route(&request, &catalog).unwrap();

    let RouteDecision::NoRoute {
        missing_evidence, ..
    } = decision.decision
    else {
        panic!("explicit unknown high-risk capability must fail closed");
    };
    assert_eq!(
        missing_evidence,
        vec!["host 'codex-local' has unknown support for capability 'tools'"]
    );
}

fn assert_stage(
    stage: &router_types::PlanStage,
    kind: StageKind,
    model: &str,
    effort: EffortLevel,
) {
    assert_eq!(stage.kind, kind);
    assert_eq!(stage.model.as_str(), model);
    assert_eq!(stage.effort, effort);
}

fn load_builtin_catalog() -> Catalog {
    Catalog::from_json(include_bytes!(
        "../../../assets/catalogs/codex-core-v1.json"
    ))
    .unwrap()
}

fn load_request(name: &str) -> RouteRequest {
    let bytes: &[u8] = match name {
        "codex-implementation.json" => {
            include_bytes!("../../../tests/fixtures/requests/codex-implementation.json")
        }
        "codex-security.json" => {
            include_bytes!("../../../tests/fixtures/requests/codex-security.json")
        }
        "no-compatible-plan.json" => {
            include_bytes!("../../../tests/fixtures/requests/no-compatible-plan.json")
        }
        _ => panic!("unknown request fixture {name}"),
    };
    serde_json::from_slice(bytes).unwrap()
}

fn equal_prior_catalog() -> Catalog {
    Catalog::from_json(
        br#"{
          "schema_version": 1,
          "version": "equal-priors-v1",
          "models": [
            {
              "id": "provider-model-b",
              "stages": ["plan", "execute", "verify"],
              "efforts": ["medium", "high"],
              "capabilities": ["tools"],
              "quality_micros": 800000,
              "cost_micros": 400000,
              "latency_micros": 300000,
              "source": "test-prior"
            },
            {
              "id": "provider-model-a",
              "stages": ["plan", "execute", "verify"],
              "efforts": ["medium", "high"],
              "capabilities": ["tools"],
              "quality_micros": 800000,
              "cost_micros": 400000,
              "latency_micros": 300000,
              "source": "test-prior"
            }
          ],
          "skills": []
        }"#,
    )
    .unwrap()
}

fn equal_prior_request() -> RouteRequest {
    let model_a = ModelId::parse("provider-model-a").unwrap();
    let model_b = ModelId::parse("provider-model-b").unwrap();
    RouteRequest {
        task: TaskContext {
            objective: "exercise exact tie breaking".into(),
            kind: TaskKind::Implement,
            risk: RiskLevel::Medium,
            required_capabilities: BTreeSet::from([Capability::Tools]),
            acceptance: vec!["model ids break equal-score ties".into()],
        },
        host: HostSnapshot {
            id: "tie-host".into(),
            harness: HarnessId::parse("codex").unwrap(),
            supported_models: BTreeSet::from([model_a.clone(), model_b.clone()]),
            supported_efforts: BTreeMap::from([
                (
                    model_a,
                    BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
                ),
                (
                    model_b,
                    BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
                ),
            ]),
            capabilities: BTreeSet::from([Capability::Tools]),
        },
        skill_candidates: vec![],
        capability_knowledge: BTreeMap::new(),
    }
}

fn security_fallback_catalog() -> Catalog {
    Catalog::from_json(
        br#"{
          "schema_version": 1,
          "version": "security-fallback-v1",
          "models": [
            {
              "id": "gpt-5.6-sol",
              "stages": ["plan", "execute", "verify"],
              "efforts": ["medium", "high"],
              "capabilities": ["tools"],
              "quality_micros": 950000,
              "cost_micros": 900000,
              "latency_micros": 0,
              "source": "test-prior"
            },
            {
              "id": "provider-security-fallback-high",
              "stages": ["plan", "execute", "verify"],
              "efforts": ["medium", "high"],
              "capabilities": ["tools"],
              "quality_micros": 850000,
              "cost_micros": 900000,
              "latency_micros": 0,
              "source": "test-prior"
            },
            {
              "id": "provider-security-fallback-low",
              "stages": ["plan", "execute", "verify"],
              "efforts": ["medium", "high"],
              "capabilities": ["tools"],
              "quality_micros": 700000,
              "cost_micros": 100000,
              "latency_micros": 0,
              "source": "test-prior"
            }
          ],
          "skills": []
        }"#,
    )
    .unwrap()
}

fn security_fallback_request() -> RouteRequest {
    let high = ModelId::parse("provider-security-fallback-high").unwrap();
    let low = ModelId::parse("provider-security-fallback-low").unwrap();
    RouteRequest {
        task: TaskContext {
            objective: "verify with the best available security reviewer".into(),
            kind: TaskKind::Security,
            risk: RiskLevel::High,
            required_capabilities: BTreeSet::from([Capability::Tools]),
            acceptance: vec!["highest-quality eligible verifier".into()],
        },
        host: HostSnapshot {
            id: "security-fallback-host".into(),
            harness: HarnessId::parse("codex").unwrap(),
            supported_models: BTreeSet::from([high.clone(), low.clone()]),
            supported_efforts: BTreeMap::from([
                (
                    high,
                    BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
                ),
                (
                    low,
                    BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
                ),
            ]),
            capabilities: BTreeSet::from([Capability::Tools]),
        },
        skill_candidates: vec![],
        capability_knowledge: BTreeMap::new(),
    }
}

fn extreme_catalog(cost_micros: i64, latency_micros: i64, skill_ids: &[&str]) -> Catalog {
    let skills = skill_ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"id":"{id}","description":"limit fixture","task_kinds":["implement"],"required_capabilities":[],"trust":"builtin"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{
          "schema_version": 1,
          "version": "extreme-v1",
          "models": [{{
            "id": "provider-extreme",
            "stages": ["plan", "execute", "verify"],
            "efforts": ["medium", "high"],
            "capabilities": ["tools"],
            "quality_micros": 1000000,
            "cost_micros": {cost_micros},
            "latency_micros": {latency_micros},
            "source": "test-prior"
          }}],
          "skills": [{skills}]
        }}"#
    );
    Catalog::from_json(json.as_bytes()).unwrap()
}

fn extreme_request(skill_ids: &[&str]) -> RouteRequest {
    let model = ModelId::parse("provider-extreme").unwrap();
    RouteRequest {
        task: TaskContext {
            objective: "exercise wide routing arithmetic".into(),
            kind: TaskKind::Implement,
            risk: RiskLevel::Medium,
            required_capabilities: BTreeSet::from([Capability::Tools]),
            acceptance: vec!["no panic or wrapped score".into()],
        },
        host: HostSnapshot {
            id: "extreme-host".into(),
            harness: HarnessId::parse("codex").unwrap(),
            supported_models: BTreeSet::from([model.clone()]),
            supported_efforts: BTreeMap::from([(
                model,
                BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
            )]),
            capabilities: BTreeSet::from([Capability::Tools]),
        },
        skill_candidates: skill_ids
            .iter()
            .map(|id| SkillCandidate {
                skill: SkillId::parse(*id).unwrap(),
                source: "limit-fixture".into(),
                score_micros: i64::MAX,
            })
            .collect(),
        capability_knowledge: BTreeMap::new(),
    }
}
