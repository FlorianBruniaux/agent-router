use std::collections::{BTreeMap, BTreeSet};

use router_catalog::Catalog;
use router_policy::{
    CapabilityKnowledge, ConstraintEngine, EligibilityContext, Support, is_eligible,
};
use router_types::{
    Capability, EffortLevel, HarnessId, HostSnapshot, ModelId, ModelSpec, RiskLevel, StageKind,
    TaskContext, TaskKind,
};

#[test]
fn unavailable_high_quality_model_is_excluded() {
    let context = fixture_context(
        "gpt-5.6-sol",
        StageKind::Plan,
        EffortLevel::High,
        RiskLevel::Medium,
    );

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert!(!is_eligible(&verdicts));
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.code() == "host.model.unavailable")
    );
}

#[test]
fn unsupported_effort_is_excluded() {
    let context = fixture_context(
        "gpt-5.6-terra",
        StageKind::Execute,
        EffortLevel::Max,
        RiskLevel::Medium,
    );

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert!(!is_eligible(&verdicts));
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.code() == "host.effort.unsupported")
    );
}

#[test]
fn missing_required_capability_is_excluded() {
    let mut context = fixture_context(
        "gpt-5.6-terra",
        StageKind::Execute,
        EffortLevel::High,
        RiskLevel::Medium,
    );
    context.task.required_capabilities = BTreeSet::from([Capability::Vision]);
    context.capabilities =
        CapabilityKnowledge::from(BTreeMap::from([(Capability::Vision, Support::Unsupported)]));

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert!(!is_eligible(&verdicts));
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.code() == "host.capability.missing")
    );
}

#[test]
fn model_that_cannot_serve_stage_is_excluded() {
    let context = fixture_context(
        "gpt-5.6-luna",
        StageKind::Verify,
        EffortLevel::High,
        RiskLevel::Medium,
    );

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert!(!is_eligible(&verdicts));
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.code() == "model.stage.unsupported")
    );
}

#[test]
fn unknown_high_risk_capability_closes_route() {
    let mut context = fixture_context(
        "gpt-5.6-terra",
        StageKind::Execute,
        EffortLevel::High,
        RiskLevel::High,
    );
    context.task.required_capabilities = BTreeSet::from([Capability::Vision]);
    context.capabilities =
        CapabilityKnowledge::from(BTreeMap::from([(Capability::Vision, Support::Unknown)]));

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert!(!is_eligible(&verdicts));
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.code() == "host.capability.unknown_high_risk")
    );
}

#[test]
fn engine_preserves_every_verdict_in_fixed_constraint_order() {
    let context = fixture_context(
        "gpt-5.6-sol",
        StageKind::Plan,
        EffortLevel::High,
        RiskLevel::Medium,
    );

    let verdicts = ConstraintEngine::default().evaluate(&context);

    assert_eq!(
        verdicts
            .iter()
            .map(|verdict| verdict.code())
            .collect::<Vec<_>>(),
        vec![
            "host.model.unavailable",
            "host.effort.unsupported",
            "model.stage.supported",
            "host.capability.satisfied",
        ]
    );
}

#[test]
fn no_host_models_means_no_model_is_eligible_for_any_catalog_stage() {
    let catalog = Catalog::from_json(include_bytes!(
        "../../../assets/catalogs/codex-core-v1.json"
    ))
    .unwrap();
    let host = HostSnapshot {
        id: "empty-host".into(),
        harness: HarnessId::parse("codex").unwrap(),
        supported_models: BTreeSet::new(),
        supported_efforts: BTreeMap::new(),
        capabilities: BTreeSet::new(),
    };

    for model_id in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
        let model = catalog
            .model(&ModelId::parse(model_id).unwrap())
            .unwrap()
            .clone();
        for stage in &model.stages {
            let effort = *model.efforts.iter().next().unwrap();
            let context = EligibilityContext {
                task: TaskContext {
                    objective: "catalog invariant".into(),
                    kind: TaskKind::Explore,
                    risk: RiskLevel::Low,
                    required_capabilities: BTreeSet::new(),
                    acceptance: vec![],
                },
                host: host.clone(),
                model: model.clone(),
                stage: *stage,
                effort,
                capabilities: CapabilityKnowledge::default(),
            };

            assert!(
                !is_eligible(&ConstraintEngine::default().evaluate(&context)),
                "{model_id} must not fall back for stage {stage:?}"
            );
        }
    }
}

fn fixture_context(
    model_id: &str,
    stage: StageKind,
    effort: EffortLevel,
    risk: RiskLevel,
) -> EligibilityContext {
    let model = fixture_model(model_id);
    EligibilityContext {
        task: TaskContext {
            objective: "route fixture".into(),
            kind: TaskKind::Implement,
            risk,
            required_capabilities: BTreeSet::new(),
            acceptance: vec!["constraint verdicts".into()],
        },
        host: HostSnapshot {
            id: "codex-host".into(),
            harness: HarnessId::parse("codex").unwrap(),
            supported_models: BTreeSet::from([
                ModelId::parse("gpt-5.6-terra").unwrap(),
                ModelId::parse("gpt-5.6-luna").unwrap(),
            ]),
            supported_efforts: BTreeMap::from([
                (
                    ModelId::parse("gpt-5.6-terra").unwrap(),
                    BTreeSet::from([EffortLevel::Medium, EffortLevel::High]),
                ),
                (
                    ModelId::parse("gpt-5.6-luna").unwrap(),
                    BTreeSet::from([EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]),
                ),
            ]),
            capabilities: BTreeSet::from([Capability::Tools, Capability::Subagents]),
        },
        model,
        stage,
        effort,
        capabilities: CapabilityKnowledge::from(BTreeMap::from([
            (Capability::Tools, Support::Supported),
            (Capability::Subagents, Support::Supported),
        ])),
    }
}

fn fixture_model(model_id: &str) -> ModelSpec {
    match model_id {
        "gpt-5.6-sol" => ModelSpec {
            id: ModelId::parse(model_id).unwrap(),
            stages: BTreeSet::from([StageKind::Plan, StageKind::Execute, StageKind::Verify]),
            efforts: BTreeSet::from([EffortLevel::High, EffortLevel::Xhigh, EffortLevel::Max]),
            capabilities: BTreeSet::from([Capability::Tools, Capability::Subagents]),
            quality_micros: 950_000,
            cost_micros: 900_000,
            latency_micros: 900_000,
            source: "fixture-policy-prior".into(),
        },
        "gpt-5.6-terra" => ModelSpec {
            id: ModelId::parse(model_id).unwrap(),
            stages: BTreeSet::from([StageKind::Execute, StageKind::Verify]),
            efforts: BTreeSet::from([EffortLevel::Medium, EffortLevel::High, EffortLevel::Xhigh]),
            capabilities: BTreeSet::from([Capability::Tools, Capability::Subagents]),
            quality_micros: 820_000,
            cost_micros: 550_000,
            latency_micros: 550_000,
            source: "fixture-policy-prior".into(),
        },
        "gpt-5.6-luna" => ModelSpec {
            id: ModelId::parse(model_id).unwrap(),
            stages: BTreeSet::from([StageKind::Execute]),
            efforts: BTreeSet::from([EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]),
            capabilities: BTreeSet::from([Capability::Tools]),
            quality_micros: 650_000,
            cost_micros: 200_000,
            latency_micros: 200_000,
            source: "fixture-policy-prior".into(),
        },
        _ => panic!("unsupported fixture model {model_id}"),
    }
}
