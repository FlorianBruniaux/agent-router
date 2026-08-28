use std::collections::{BTreeMap, BTreeSet};

use router_types::{
    Capability, EffortLevel, HarnessId, HostSnapshot, ModelId, RiskLevel, SkillCandidate, SkillId,
    StageKind, TaskContext, TaskKind, VerificationIndependence,
};

#[test]
fn model_id_rejects_bare_tier_aliases() {
    let error = ModelId::parse("sol").expect_err("bare aliases must be rejected");
    assert_eq!(error.to_string(), "model id 'sol' is not canonical");
}

#[test]
fn host_snapshot_round_trips_with_stable_set_order() {
    let snapshot = HostSnapshot {
        id: "fixture-codex".into(),
        harness: HarnessId::parse("codex").unwrap(),
        supported_models: BTreeSet::from([
            ModelId::parse("gpt-5.6-terra").unwrap(),
            ModelId::parse("gpt-5.6-sol").unwrap(),
        ]),
        supported_efforts: BTreeMap::from([(
            ModelId::parse("gpt-5.6-sol").unwrap(),
            BTreeSet::from([EffortLevel::High, EffortLevel::Xhigh]),
        )]),
        capabilities: BTreeSet::from([Capability::Tools, Capability::Subagents]),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let decoded: HostSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(json.find("gpt-5.6-sol").unwrap() < json.find("gpt-5.6-terra").unwrap());
}

#[test]
fn task_context_requires_explicit_risk_and_acceptance() {
    let task = TaskContext {
        objective: "Implement a bounded parser change".into(),
        kind: TaskKind::Implement,
        risk: RiskLevel::Medium,
        required_capabilities: BTreeSet::from([Capability::Tools]),
        acceptance: vec!["workspace tests pass".into()],
    };
    assert_eq!(task.risk, RiskLevel::Medium);
    assert_eq!(task.acceptance.len(), 1);
}

#[test]
fn skill_candidate_preserves_retrieval_source_and_score() {
    let candidate = SkillCandidate {
        skill: SkillId::parse("tdd").unwrap(),
        source: "bm25-hook-v1".into(),
        score_micros: 812_500,
    };
    assert_eq!(candidate.score_micros, 812_500);
}

#[test]
fn plan_stage_serializes_verification_independence_when_present() {
    let stage = router_types::PlanStage {
        kind: StageKind::Verify,
        model: ModelId::parse("gpt-5.6-terra").unwrap(),
        effort: EffortLevel::High,
        skills: vec![],
        verification_independence: Some(VerificationIndependence::FreshInstance),
    };

    assert_eq!(
        serde_json::to_string(&stage).unwrap(),
        r#"{"kind":"verify","model":"gpt-5.6-terra","effort":"high","skills":[],"verification_independence":"fresh_instance"}"#
    );
}
