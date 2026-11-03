// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! ADR 0054 "Skills under stable-v2" and ADR 0055: per-stage cap, ranking and
//! floor of skill candidates. Literal values are used on purpose so a change of
//! a constant is a visible test diff.
use std::{fs, path::PathBuf};

use router_catalog::Catalog;
use router_policy::Router;
use router_types::{RouteDecision, RouteRequest, SkillCandidate, SkillId};
use serde_json::{Value, json};

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("router-policy crate is nested beneath the workspace root")
        .join(path)
}

fn catalog() -> Catalog {
    let mut value: Value =
        serde_json::from_slice(&fs::read(root("assets/catalogs/codex-core-v2.json")).unwrap())
            .unwrap();
    let skills = value["skills"].as_array_mut().unwrap();
    for id in ["skill-a", "skill-b", "skill-c", "skill-d", "skill-e"] {
        skills.push(json!({
            "id": id,
            "description": "Synthetic skill for the cap tests.",
            "task_kinds": ["implement"],
            "required_capabilities": [],
            "trust": "builtin"
        }));
    }
    skills.push(json!({
        "id": "skill-untrusted",
        "description": "Synthetic untrusted skill.",
        "task_kinds": ["implement"],
        "required_capabilities": [],
        "trust": "untrusted"
    }));
    Catalog::from_json(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn request(candidates: &[(&str, &str, i64)]) -> RouteRequest {
    let mut request: RouteRequest = serde_json::from_slice(
        &fs::read(root("tests/fixtures/requests/codex-implementation.json")).unwrap(),
    )
    .unwrap();
    request.skill_candidates = candidates
        .iter()
        .map(|(id, source, score)| SkillCandidate {
            skill: SkillId::parse((*id).to_owned()).unwrap(),
            source: (*source).to_owned(),
            score_micros: *score,
        })
        .collect();
    request
}

fn stage_skills(router: &Router, request: &RouteRequest) -> Vec<Vec<String>> {
    let decision = router.route(request, &catalog()).unwrap();
    decision
        .selected_plan()
        .expect("a plan is selected")
        .stages
        .iter()
        .map(|stage| {
            stage
                .skills
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect()
        })
        .collect()
}

fn models(router: &Router, request: &RouteRequest) -> Vec<String> {
    let decision = router.route(request, &catalog()).unwrap();
    decision
        .selected_plan()
        .unwrap()
        .stages
        .iter()
        .map(|stage| stage.model.as_str().to_owned())
        .collect()
}

#[test]
fn v2_attaches_at_most_two_skills_per_stage() {
    let request = request(&[
        ("skill-a", "ext", 900_000),
        ("skill-b", "ext", 800_000),
        ("skill-c", "ext", 700_000),
        ("skill-d", "ext", 600_000),
    ]);
    for skills in stage_skills(&Router::stable_v2(), &request) {
        assert_eq!(skills, ["skill-a", "skill-b"]);
    }
}

#[test]
fn v2_ranks_operator_first_then_score_then_id() {
    let request = request(&[
        ("skill-a", "ext", 400_000),
        ("skill-b", "ext", 900_000),
        ("skill-c", "ext", 900_000),
        ("skill-d", "operator-declared", 0),
    ]);
    for skills in stage_skills(&Router::stable_v2(), &request) {
        assert_eq!(skills, ["skill-d", "skill-b"]);
    }
    let request = self::request(&[
        ("skill-e", "ext", 500_000),
        ("skill-c", "ext", 500_000),
        ("skill-a", "ext", 300_000),
    ]);
    for skills in stage_skills(&Router::stable_v2(), &request) {
        assert_eq!(skills, ["skill-c", "skill-e"]);
    }
}

#[test]
fn v2_floor_applies_to_non_operator_candidates_only() {
    let request = request(&[
        ("skill-a", "ext", 299_999),
        ("skill-b", "ext", 300_000),
        ("skill-c", "operator-declared", 0),
    ]);
    for skills in stage_skills(&Router::stable_v2(), &request) {
        assert_eq!(skills, ["skill-c", "skill-b"]);
    }
}

#[test]
fn v2_never_attaches_an_untrusted_skill() {
    let request = request(&[
        ("skill-untrusted", "operator-declared", 0),
        ("skill-untrusted", "ext", 1_000_000),
        ("skill-a", "ext", 400_000),
    ]);
    for skills in stage_skills(&Router::stable_v2(), &request) {
        assert_eq!(skills, ["skill-a"]);
    }
}

#[test]
fn skills_never_change_the_selected_models_or_the_utility() {
    let bare = request(&[]);
    let loaded = request(&[
        ("skill-a", "ext", 900_000),
        ("skill-b", "operator-declared", 0),
    ]);
    assert_eq!(
        models(&Router::stable_v2(), &bare),
        models(&Router::stable_v2(), &loaded)
    );
    let utility = |request: &RouteRequest| match Router::stable_v2()
        .route(request, &catalog())
        .unwrap()
        .decision
    {
        RouteDecision::Selected { plan, .. } => plan.utility_micros,
        RouteDecision::NoRoute { .. } => panic!("selected"),
    };
    assert_eq!(utility(&bare), utility(&loaded));
}

#[test]
fn v1_keeps_every_compatible_skill_and_ignores_the_floor() {
    let request = request(&[
        ("skill-a", "ext", 100),
        ("skill-b", "ext", 800_000),
        ("skill-c", "ext", 700_000),
        ("skill-d", "ext", 600_000),
    ]);
    for skills in stage_skills(&Router::stable_v1(), &request) {
        assert_eq!(skills, ["skill-a", "skill-b", "skill-c", "skill-d"]);
    }
}

#[test]
fn v2_selection_is_independent_of_candidate_order() {
    let rows = [
        ("skill-a", "ext", 500_000),
        ("skill-b", "ext", 900_000),
        ("skill-c", "ext", 500_000),
        ("skill-d", "operator-declared", 0),
    ];
    let mut reversed = rows;
    reversed.reverse();
    assert_eq!(
        stage_skills(&Router::stable_v2(), &request(&rows)),
        stage_skills(&Router::stable_v2(), &request(&reversed))
    );
}

/// `codex-core-v3` with 256 compatible synthetic skills and a host that
/// supports every model of the catalog.
fn wide_case() -> (Catalog, RouteRequest) {
    let mut value: Value =
        serde_json::from_slice(&fs::read(root("assets/catalogs/codex-core-v3.json")).unwrap())
            .unwrap();
    let skills = value["skills"].as_array_mut().unwrap();
    skills.clear();
    for index in 0..256 {
        skills.push(json!({
            "id": format!("wide-{index:03}"),
            "description": "Synthetic skill for the work-limit tests.",
            "task_kinds": ["implement"],
            "required_capabilities": [],
            "trust": "builtin"
        }));
    }
    let models = value["models"].as_array().unwrap().clone();
    let mut request = request(&[]);
    request.host.supported_models = models
        .iter()
        .map(|model| serde_json::from_value(model["id"].clone()).unwrap())
        .collect();
    request.host.supported_efforts = models
        .iter()
        .map(|model| {
            (
                serde_json::from_value(model["id"].clone()).unwrap(),
                serde_json::from_value(model["efforts"].clone()).unwrap(),
            )
        })
        .collect();
    request.skill_candidates = (0..256)
        .map(|index| SkillCandidate {
            skill: SkillId::parse(format!("wide-{index:03}")).unwrap(),
            source: "external-declared:wide".into(),
            score_micros: 400_000 + i64::from(index),
        })
        .collect();
    (
        Catalog::from_json(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request,
    )
}

#[test]
fn skills_per_stage_are_capped_and_selection_is_deterministic() {
    let (catalog, request) = wide_case();
    let first = Router::stable_v2().route(&request, &catalog).unwrap();
    let second = Router::stable_v2().route(&request, &catalog).unwrap();
    assert_eq!(first, second);
    let plan = first.selected_plan().expect("selected, no work limit");
    for stage in &plan.stages {
        let ids: Vec<_> = stage.skills.iter().map(SkillId::as_str).collect();
        assert_eq!(ids, ["wide-255", "wide-254"]);
    }
    // Negative control: the frozen v1 rules keep all 256 skills on every stage.
    // On this 6 x 10 x 8 catalog that is 480 x 769 work units, still under the
    // limit, so the control shows the payload the cap removes, not an error.
    let v1 = Router::stable_v1().route(&request, &catalog).unwrap();
    for stage in &v1.selected_plan().unwrap().stages {
        assert_eq!(stage.skills.len(), 256);
    }
}
