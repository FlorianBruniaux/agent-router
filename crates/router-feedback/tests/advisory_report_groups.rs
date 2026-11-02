// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! The advisory report groups latest outcomes by policy version and risk, hides
//! rates under a declared minimum sample and keeps unstated fields UNKNOWN.
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

fn outcome(
    key: &str,
    policy: &str,
    risk: RiskLevel,
    adherence: AdvisoryAdherence,
    result: AdvisoryResult,
    used: Option<AdvisoryUsed>,
) -> AdvisoryOutcomeInput {
    AdvisoryOutcomeInput {
        schema_version: 1,
        observed_at_unix_seconds: 150,
        binding: AdvisoryBinding {
            decision_id: key.repeat(64),
            catalog_version: "catalog-1".into(),
            policy_version: policy.into(),
            task_kind: TaskKind::Implement,
            risk,
            selection_basis: AdvisorySelectionBasis::CatalogPolicyPrior,
            recommended: vec![AdvisoryPhase {
                stage: StageKind::Execute,
                model: ModelId::parse("model-a").unwrap(),
                effort: EffortLevel::High,
                skills: BTreeSet::new(),
            }],
        },
        adherence,
        used,
        result,
        measurements: AdvisoryMeasurements::default(),
    }
}

fn follow(
    key: &str,
    policy: &str,
    risk: RiskLevel,
    result: AdvisoryResult,
) -> AdvisoryOutcomeInput {
    outcome(key, policy, risk, AdvisoryAdherence::Followed, result, None)
}

fn override_with(
    key: &str,
    policy: &str,
    risk: RiskLevel,
    used: AdvisoryUsed,
) -> AdvisoryOutcomeInput {
    outcome(
        key,
        policy,
        risk,
        AdvisoryAdherence::Overridden,
        AdvisoryResult::AcceptedAsIs,
        Some(used),
    )
}

fn record_all(store: &FeedbackStore, inputs: &[AdvisoryOutcomeInput]) {
    for input in inputs {
        store.record_advisory_outcome(input.clone(), 200).unwrap();
    }
}

fn group<'a>(
    report: &'a AdvisoryOutcomeReport,
    policy: &str,
    risk: RiskLevel,
) -> &'a AdvisoryPolicyRiskGroup {
    report
        .groups
        .iter()
        .find(|g| g.policy_version == policy && g.risk == risk)
        .unwrap_or_else(|| panic!("no group {policy} {risk:?}"))
}

#[test]
fn report_groups_by_policy_version_and_risk() {
    let (_t, store) = setup();
    record_all(
        &store,
        &[
            follow("a", "v1", RiskLevel::Low, AdvisoryResult::AcceptedAsIs),
            follow("b", "v1", RiskLevel::Low, AdvisoryResult::Abandoned),
            follow("c", "v2", RiskLevel::Low, AdvisoryResult::AcceptedAsIs),
            follow("d", "v1", RiskLevel::High, AdvisoryResult::AcceptedAsIs),
        ],
    );
    let report = store.advisory_report(300).unwrap();
    assert_eq!(report.groups.len(), 3);
    let v1_low = group(&report, "v1", RiskLevel::Low);
    assert_eq!(v1_low.decisions, 2);
    assert_eq!(v1_low.adherence.followed.sample_size, 2);
    assert_eq!(v1_low.adherence.followed.abandoned.numerator, 1);
    let v2_low = group(&report, "v2", RiskLevel::Low);
    assert_eq!(v2_low.decisions, 1);
    assert_eq!(v2_low.adherence.followed.abandoned.numerator, 0);
    assert_eq!(group(&report, "v1", RiskLevel::High).decisions, 1);
    assert_eq!(report.decisions, 4);
}

#[test]
fn report_counts_the_latest_attempt_per_decision_in_its_group() {
    let (_t, store) = setup();
    record_all(
        &store,
        &[follow("a", "v1", RiskLevel::Low, AdvisoryResult::Abandoned)],
    );
    store
        .record_advisory_outcome(
            follow("a", "v1", RiskLevel::Low, AdvisoryResult::AcceptedAsIs),
            210,
        )
        .unwrap();
    let report = store.advisory_report(300).unwrap();
    let g = group(&report, "v1", RiskLevel::Low);
    assert_eq!(g.decisions, 1);
    assert_eq!(g.adherence.followed.accepted_as_is.numerator, 1);
    assert_eq!(g.adherence.followed.abandoned.numerator, 0);
    assert_eq!(g.adherence.followed.records, 2);
}

#[test]
fn report_hides_rates_below_minimum_sample() {
    let (_t, store) = setup();
    let mut inputs = Vec::new();
    for i in 0..ADVISORY_MINIMUM_SAMPLE {
        let key = format!("{i:x}");
        inputs.push(follow(
            &key,
            "v1",
            RiskLevel::Low,
            AdvisoryResult::AcceptedAsIs,
        ));
    }
    inputs.push(follow(
        "f",
        "v2",
        RiskLevel::Low,
        AdvisoryResult::AcceptedAsIs,
    ));
    record_all(&store, &inputs);
    let report = store.advisory_report(300).unwrap();
    assert_eq!(report.minimum_sample, ADVISORY_MINIMUM_SAMPLE);
    let small = group(&report, "v2", RiskLevel::Low);
    assert!(small.insufficient_sample);
    let rate = small.adherence.followed.accepted_as_is;
    assert_eq!((rate.numerator, rate.denominator), (1, 1));
    assert_eq!(rate.rate_micros, None);
    let enough = group(&report, "v1", RiskLevel::Low);
    assert!(!enough.insufficient_sample);
    assert_eq!(
        enough.adherence.followed.accepted_as_is.rate_micros,
        Some(1_000_000)
    );
    // The empty overridden side of a sufficient group has no rate either.
    assert_eq!(enough.adherence.overridden.sample_size, 0);
    assert_eq!(enough.adherence.overridden.abandoned.rate_micros, None);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(
        json["minimum_sample"].as_u64(),
        Some(ADVISORY_MINIMUM_SAMPLE)
    );
    assert!(json["groups"][0]["insufficient_sample"].is_boolean());
}

#[test]
fn report_is_deterministic_across_insertion_order() {
    let inputs = [
        follow("a", "v2", RiskLevel::High, AdvisoryResult::AcceptedAsIs),
        follow("b", "v1", RiskLevel::Low, AdvisoryResult::Abandoned),
        follow("c", "v1", RiskLevel::High, AdvisoryResult::AcceptedAsIs),
        follow(
            "d",
            "v2",
            RiskLevel::Low,
            AdvisoryResult::AcceptedAfterRework,
        ),
    ];
    let (_t1, forward) = setup();
    record_all(&forward, &inputs);
    let mut reversed = inputs.to_vec();
    reversed.reverse();
    let (_t2, backward) = setup();
    record_all(&backward, &reversed);
    let a = forward.advisory_report(300).unwrap();
    let b = backward.advisory_report(300).unwrap();
    let keys = |r: &AdvisoryOutcomeReport| {
        r.groups
            .iter()
            .map(|g| (g.policy_version.clone(), g.risk))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&a),
        vec![
            ("v1".to_owned(), RiskLevel::Low),
            ("v1".to_owned(), RiskLevel::High),
            ("v2".to_owned(), RiskLevel::Low),
            ("v2".to_owned(), RiskLevel::High),
        ]
    );
    assert_eq!(keys(&a), keys(&b));
    assert_eq!(a.groups, b.groups);
}

#[test]
fn override_stats_exclude_unknown_used_fields() {
    let (_t, store) = setup();
    record_all(
        &store,
        &[
            override_with(
                "a",
                "v1",
                RiskLevel::Low,
                AdvisoryUsed {
                    model: Some(ModelId::parse("model-b").unwrap()),
                    effort: None,
                    skills: None,
                },
            ),
            override_with(
                "b",
                "v1",
                RiskLevel::Low,
                AdvisoryUsed {
                    model: None,
                    effort: Some(EffortLevel::Low),
                    skills: Some(BTreeSet::new()),
                },
            ),
        ],
    );
    let report = store.advisory_report(300).unwrap();
    let unknown = group(&report, "v1", RiskLevel::Low)
        .adherence
        .overridden
        .unknown;
    assert_eq!(unknown.used_model, Some(1));
    assert_eq!(unknown.used_effort, Some(1));
    assert_eq!(unknown.used_skills, Some(1));
    // Followed outcomes state no substitute: the used counts do not apply.
    let followed = group(&report, "v1", RiskLevel::Low).adherence.followed;
    assert_eq!(followed.unknown.used_model, None);
}

#[test]
fn report_states_selection_bias_and_that_paired_counting_is_unavailable() {
    let (_t, store) = setup();
    let report = store.advisory_report(300).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["paired_trials"], "not_available");
    let text = json["interpretation_notes"].to_string();
    for needle in ["self-selected", "harder tasks", "advice.json"] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
}
