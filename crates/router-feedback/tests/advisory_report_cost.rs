// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! The advisory report sums the known cost and token measurements per adherence
//! side. A missing value is never a zero, and a mean needs a minimum of known
//! values.
use std::collections::BTreeSet;

use router_feedback::*;
use router_types::*;
use tempfile::TempDir;

fn setup() -> (TempDir, FeedbackStore) {
    let temp = TempDir::new().unwrap();
    let store = FeedbackStore::open(&temp.path().join("feedback")).unwrap();
    store
        .configure(
            FeedbackPreferences {
                collection_enabled: true,
                ..Default::default()
            },
            100,
        )
        .unwrap();
    (temp, store)
}

fn record(
    key: &str,
    request: Option<&str>,
    policy: &str,
    adherence: AdvisoryAdherence,
    cost: Option<u64>,
    tokens: Option<u64>,
) -> AdvisoryOutcomeInput {
    AdvisoryOutcomeInput {
        schema_version: 1,
        observed_at_unix_seconds: 150,
        binding: AdvisoryBinding {
            decision_id: key.repeat(64),
            request_hash: request.map(|r| r.repeat(64)),
            catalog_version: "catalog-1".into(),
            policy_version: policy.into(),
            task_kind: TaskKind::Implement,
            risk: RiskLevel::Low,
            selection_basis: AdvisorySelectionBasis::CatalogPolicyPrior,
            recommended: vec![AdvisoryPhase {
                stage: StageKind::Execute,
                model: ModelId::parse("model-a").unwrap(),
                effort: EffortLevel::High,
                skills: BTreeSet::new(),
            }],
        },
        adherence,
        used: (adherence == AdvisoryAdherence::Overridden).then(|| AdvisoryUsed {
            model: Some(ModelId::parse("model-b").unwrap()),
            effort: None,
            skills: None,
        }),
        result: AdvisoryResult::AcceptedAsIs,
        measurements: AdvisoryMeasurements {
            cost_microusd: cost,
            tokens,
        },
    }
}

fn record_all(store: &FeedbackStore, inputs: &[AdvisoryOutcomeInput]) {
    for input in inputs {
        store.record_advisory_outcome(input.clone(), 200).unwrap();
    }
}

fn keys(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{i:x}")).collect()
}

#[test]
fn a_missing_cost_is_not_a_zero() {
    let (_t, store) = setup();
    // Six followed decisions, four with a cost. 6 is above the minimum sample
    // but 4 known values are below it: the mean stays null.
    let costs = [Some(100), Some(200), Some(300), Some(400), None, None];
    let inputs: Vec<_> = keys(6)
        .iter()
        .zip(costs)
        .map(|(k, c)| record(k, None, "v1", AdvisoryAdherence::Followed, c, None))
        .collect();
    record_all(&store, &inputs);
    let report = store.advisory_report(300).unwrap();
    let followed = &report.groups[0].adherence.followed;
    assert_eq!(followed.sample_size, 6);
    assert_eq!(followed.unknown.cost, 2);
    assert_eq!(followed.cost_microusd.records, 4);
    assert_eq!(followed.cost_microusd.total, 1000);
    assert_eq!(followed.cost_microusd.mean, None);
    // No token was stated: zero known values, a zero total and no mean.
    assert_eq!(followed.tokens.records, 0);
    assert_eq!(followed.tokens.total, 0);
    assert_eq!(followed.tokens.mean, None);
}

#[test]
fn the_mean_appears_from_the_minimum_of_known_values_and_rounds_half_up() {
    let (_t, store) = setup();
    let known = ADVISORY_MINIMUM_SAMPLE as usize;
    // 5 known costs of 1,1,1,1,2 = 6 -> 1.2 -> 1; plus one unknown that must not
    // lower the mean.
    let mut inputs: Vec<_> = keys(known)
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let cost = if i == known - 1 { 2 } else { 1 };
            record(
                k,
                None,
                "v1",
                AdvisoryAdherence::Followed,
                Some(cost),
                Some(10),
            )
        })
        .collect();
    inputs.push(record(
        "f",
        None,
        "v1",
        AdvisoryAdherence::Followed,
        None,
        Some(15),
    ));
    record_all(&store, &inputs);
    let report = store.advisory_report(300).unwrap();
    let followed = &report.groups[0].adherence.followed;
    assert_eq!(followed.cost_microusd.records, 5);
    assert_eq!(followed.cost_microusd.total, 6);
    assert_eq!(followed.cost_microusd.mean, Some(1));
    // Tokens: 5 x 10 + 15 = 65 over 6 known values -> 10.83 -> 11.
    assert_eq!(followed.tokens.records, 6);
    assert_eq!(followed.tokens.total, 65);
    assert_eq!(followed.tokens.mean, Some(11));
}

#[test]
fn the_overridden_side_is_independent() {
    let (_t, store) = setup();
    let mut inputs: Vec<_> = keys(5)
        .iter()
        .map(|k| {
            record(
                k,
                None,
                "v1",
                AdvisoryAdherence::Followed,
                Some(100),
                Some(1),
            )
        })
        .collect();
    inputs.push(record(
        "a",
        None,
        "v1",
        AdvisoryAdherence::Overridden,
        Some(900),
        None,
    ));
    record_all(&store, &inputs);
    let report = store.advisory_report(300).unwrap();
    let g = &report.groups[0].adherence;
    assert_eq!(g.followed.cost_microusd.total, 500);
    assert_eq!(g.followed.cost_microusd.mean, Some(100));
    assert_eq!(g.overridden.cost_microusd.records, 1);
    assert_eq!(g.overridden.cost_microusd.total, 900);
    assert_eq!(g.overridden.cost_microusd.mean, None);
    assert_eq!(g.overridden.tokens.records, 0);
    assert_eq!(report.overall.followed.cost_microusd.total, 500);
}

#[test]
fn a_superseded_attempt_is_not_summed() {
    let (_t, store) = setup();
    record_all(
        &store,
        &[record(
            "a",
            None,
            "v1",
            AdvisoryAdherence::Followed,
            Some(100),
            None,
        )],
    );
    store
        .record_advisory_outcome(
            record("a", None, "v1", AdvisoryAdherence::Followed, Some(40), None),
            210,
        )
        .unwrap();
    let report = store.advisory_report(300).unwrap();
    let followed = &report.groups[0].adherence.followed;
    assert_eq!(followed.records, 2);
    assert_eq!(followed.cost_microusd.records, 1);
    assert_eq!(followed.cost_microusd.total, 40);
}

#[test]
fn a_paired_arm_sums_its_own_decisions() {
    let (_t, store) = setup();
    // Two requests, each decided under both policies. v1 costs 100 and 300, v2
    // costs 50 and one unknown.
    record_all(
        &store,
        &[
            record(
                "a",
                Some("1"),
                "v1",
                AdvisoryAdherence::Followed,
                Some(100),
                Some(7),
            ),
            record(
                "b",
                Some("1"),
                "v2",
                AdvisoryAdherence::Followed,
                Some(50),
                Some(3),
            ),
            record(
                "c",
                Some("2"),
                "v1",
                AdvisoryAdherence::Followed,
                Some(300),
                None,
            ),
            record(
                "d",
                Some("2"),
                "v2",
                AdvisoryAdherence::Followed,
                None,
                None,
            ),
        ],
    );
    let report = store.advisory_report(300).unwrap();
    let arm = |policy: &str| {
        report
            .paired_trials
            .arms
            .iter()
            .find(|a| a.policy_version == policy)
            .unwrap()
    };
    let v1 = &arm("v1").adherence.followed;
    assert_eq!(v1.cost_microusd.records, 2);
    assert_eq!(v1.cost_microusd.total, 400);
    assert_eq!(v1.cost_microusd.mean, None);
    assert_eq!(v1.tokens.records, 1);
    assert_eq!(v1.tokens.total, 7);
    let v2 = &arm("v2").adherence.followed;
    assert_eq!(v2.cost_microusd.records, 1);
    assert_eq!(v2.cost_microusd.total, 50);
    assert_eq!(v2.unknown.cost, 1);
}

#[test]
fn the_json_shape_and_the_cost_interpretation_note() {
    let (_t, store) = setup();
    record_all(
        &store,
        &[record(
            "a",
            None,
            "v1",
            AdvisoryAdherence::Followed,
            Some(250),
            None,
        )],
    );
    let json = serde_json::to_value(store.advisory_report(300).unwrap()).unwrap();
    assert_eq!(
        json["groups"][0]["followed"]["cost_microusd"],
        serde_json::json!({"records": 1, "total": 250, "mean": null})
    );
    assert_eq!(
        json["groups"][0]["followed"]["tokens"],
        serde_json::json!({"records": 0, "total": 0, "mean": null})
    );
    assert_eq!(json["schema_version"], 4);
    let notes = json["interpretation_notes"].as_array().unwrap();
    assert_eq!(notes.len(), 5);
    let text = json["interpretation_notes"].to_string();
    for needle in ["self-reported", "task mixes"] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
}
