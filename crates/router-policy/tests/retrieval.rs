use std::collections::BTreeSet;

use router_policy::{SkillRetriever, StaticSkillRetriever, parse_candidate_document};
use router_types::{Capability, RiskLevel, TaskContext, TaskKind};

#[test]
fn parses_versioned_bm25_candidates_without_loading_skill_bodies() {
    let bytes = include_bytes!("../../../tests/fixtures/requests/bm25-candidates.json");

    let candidates = parse_candidate_document(bytes).unwrap();

    assert_eq!(candidates[0].skill.as_str(), "tdd");
    assert_eq!(candidates[0].source, "ai-agents-bm25-v1");
    assert_eq!(candidates[0].score_micros, 910_000);
}

#[test]
fn rejects_unknown_candidate_schema() {
    let bytes = br#"{"schema_version":"2","source":"ai-agents-bm25-v1","candidates":[]}"#;

    let error = parse_candidate_document(bytes).unwrap_err();

    assert_eq!(error.code(), "retrieval.schema.unsupported");
    assert_eq!(error.path(), "schema_version");
}

#[test]
fn rejects_duplicate_skill_ids() {
    let bytes = br#"{
        "schema_version":"1",
        "source":"ai-agents-bm25-v1",
        "candidates":[
            {"skill":"tdd","score_micros":910000},
            {"skill":"tdd","score_micros":900000}
        ]
    }"#;

    let error = parse_candidate_document(bytes).unwrap_err();

    assert_eq!(error.code(), "retrieval.candidate.duplicate_skill");
    assert_eq!(error.path(), "candidates[1].skill");
}

#[test]
fn rejects_empty_sources() {
    let bytes = br#"{"schema_version":"1","source":"","candidates":[]}"#;

    let error = parse_candidate_document(bytes).unwrap_err();

    assert_eq!(error.code(), "retrieval.source.empty");
}

#[test]
fn rejects_scores_outside_the_normalized_range() {
    for score_micros in [-1, 1_000_001] {
        let bytes = format!(
            r#"{{"schema_version":"1","source":"ai-agents-bm25-v1","candidates":[{{"skill":"tdd","score_micros":{score_micros}}}]}}"#
        );

        let error = parse_candidate_document(bytes.as_bytes()).unwrap_err();

        assert_eq!(error.code(), "retrieval.candidate.score_out_of_range");
        assert_eq!(error.path(), "candidates[0].score_micros");
    }
}

#[test]
fn static_retriever_returns_the_validated_candidates_for_a_task() {
    let candidates = parse_candidate_document(include_bytes!(
        "../../../tests/fixtures/requests/bm25-candidates.json"
    ))
    .unwrap();
    let retriever = StaticSkillRetriever::new(candidates.clone());

    let retrieved = retriever.retrieve(&task_context()).unwrap();

    assert_eq!(retrieved, candidates);
}

fn task_context() -> TaskContext {
    TaskContext {
        objective: "verify the candidate boundary".into(),
        kind: TaskKind::Implement,
        risk: RiskLevel::Low,
        required_capabilities: BTreeSet::<Capability>::new(),
        acceptance: Vec::new(),
    }
}
