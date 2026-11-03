// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! The advisory outcome carries the `request_hash` of the saved advice (ADR
//! 0056): store schema 4 keeps every schema 3 row with the hash UNKNOWN, and the
//! report counts paired trials by that hash, one arm per policy and catalog,
//! never mixing policies.
use std::{collections::BTreeSet, os::unix::fs::PermissionsExt, path::Path};

use router_feedback::*;
use router_types::*;
use tempfile::TempDir;

fn prefs() -> FeedbackPreferences {
    FeedbackPreferences {
        collection_enabled: true,
        ..Default::default()
    }
}

fn setup() -> (TempDir, FeedbackStore) {
    let temp = TempDir::new().unwrap();
    let store = FeedbackStore::open(&temp.path().join("feedback")).unwrap();
    store.configure(prefs(), 100).unwrap();
    (temp, store)
}

fn arm(
    key: &str,
    request: Option<&str>,
    policy: &str,
    catalog: &str,
    adherence: AdvisoryAdherence,
    result: AdvisoryResult,
) -> AdvisoryOutcomeInput {
    AdvisoryOutcomeInput {
        schema_version: 1,
        observed_at_unix_seconds: 150,
        binding: AdvisoryBinding {
            decision_id: key.repeat(64),
            request_hash: request.map(|r| r.repeat(64)),
            catalog_version: catalog.into(),
            policy_version: policy.into(),
            task_kind: TaskKind::Implement,
            risk: RiskLevel::Medium,
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
        result,
        measurements: AdvisoryMeasurements::default(),
    }
}

fn followed(key: &str, request: Option<&str>, policy: &str) -> AdvisoryOutcomeInput {
    arm(
        key,
        request,
        policy,
        "catalog-1",
        AdvisoryAdherence::Followed,
        AdvisoryResult::AcceptedAsIs,
    )
}

fn overridden(key: &str, request: Option<&str>, policy: &str) -> AdvisoryOutcomeInput {
    arm(
        key,
        request,
        policy,
        "catalog-1",
        AdvisoryAdherence::Overridden,
        AdvisoryResult::Abandoned,
    )
}

fn arm_row<'a>(
    report: &'a AdvisoryOutcomeReport,
    policy: &str,
    catalog: &str,
) -> &'a AdvisoryPairedArm {
    report
        .paired_trials
        .arms
        .iter()
        .find(|arm| arm.policy_version == policy && arm.catalog_version == catalog)
        .unwrap_or_else(|| panic!("no arm {policy}/{catalog}"))
}

/// The schema exactly as the version 3 code created it.
const V3_SCHEMA: &str = "CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE feedback(sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_hash TEXT NOT NULL, origin TEXT NOT NULL, observed_at INTEGER NOT NULL, event TEXT NOT NULL, UNIQUE(run_hash,origin));
    CREATE TRIGGER feedback_immutable BEFORE UPDATE ON feedback BEGIN SELECT RAISE(ABORT,'immutable feedback'); END;
    CREATE TABLE policies(id TEXT PRIMARY KEY, artifact TEXT NOT NULL, state TEXT NOT NULL);
    CREATE TRIGGER policy_artifacts_immutable BEFORE UPDATE OF artifact ON policies BEGIN SELECT RAISE(ABORT,'immutable policy artifact'); END;
    CREATE TABLE policy_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT, artifact_id TEXT NOT NULL REFERENCES policies(id), event TEXT NOT NULL);
    CREATE TRIGGER policy_events_immutable BEFORE UPDATE ON policy_events BEGIN SELECT RAISE(ABORT,'immutable policy event'); END;
    CREATE TABLE facts(sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_key TEXT NOT NULL, kind TEXT NOT NULL, observed_at INTEGER NOT NULL, fact TEXT NOT NULL, UNIQUE(run_key,kind));
    CREATE TRIGGER facts_immutable BEFORE UPDATE ON facts BEGIN SELECT RAISE(ABORT,'immutable fact'); END;
    CREATE TABLE advisory_outcomes(sequence INTEGER PRIMARY KEY AUTOINCREMENT, decision_id TEXT NOT NULL, attempt INTEGER NOT NULL, observed_at INTEGER NOT NULL, event TEXT NOT NULL, UNIQUE(decision_id,attempt));
    CREATE TRIGGER advisory_outcomes_immutable BEFORE UPDATE ON advisory_outcomes BEGIN SELECT RAISE(ABORT,'immutable advisory outcome'); END;
    PRAGMA user_version=3;";

fn version(file: &Path) -> u32 {
    rusqlite::Connection::open(file)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

#[test]
fn a_version_3_store_migrates_keeping_every_outcome_with_an_unknown_request_hash() {
    // Rows as the version 3 code wrote them: an outcome without a request hash
    // has the same bytes and outcome hash before and after this change.
    let (_source, store) = setup();
    for input in [
        followed("a", None, "stable-v1"),
        overridden("b", None, "stable-v2"),
        arm(
            "b",
            None,
            "stable-v2",
            "catalog-1",
            AdvisoryAdherence::Followed,
            AdvisoryResult::AcceptedAfterRework,
        ),
    ] {
        store.record_advisory_outcome(input, 200).unwrap();
    }
    let rows = store.advisory_outcomes(200).unwrap();
    assert_eq!(rows.len(), 3);
    let texts: Vec<String> = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect();
    assert!(texts.iter().all(|text| !text.contains("request_hash")));
    drop(store);

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feedback");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = path.join("feedback.sqlite3");
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch(V3_SCHEMA).unwrap();
    let consent = FeedbackConsent {
        preferences: prefs(),
        revision: 1,
        updated_at_unix_seconds: 100,
    };
    db.execute(
        "INSERT INTO settings VALUES('consent',?1)",
        [serde_json::to_string(&consent).unwrap()],
    )
    .unwrap();
    for (row, text) in rows.iter().zip(&texts) {
        db.execute(
            "INSERT INTO advisory_outcomes(sequence,decision_id,attempt,observed_at,event) VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![
                row.sequence,
                row.input.binding.decision_id,
                row.attempt,
                row.input.observed_at_unix_seconds,
                text
            ],
        )
        .unwrap();
    }
    drop(db);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(version(&file), 3);

    let store = FeedbackStore::open(&path).unwrap();
    assert_eq!(version(&file), 4);
    let migrated = store.advisory_outcomes(250).unwrap();
    assert_eq!(migrated, rows);
    assert!(
        migrated
            .iter()
            .all(|row| row.input.binding.request_hash.is_none())
    );
    let stored: Vec<(String, Option<String>)> = rusqlite::Connection::open(&file)
        .unwrap()
        .prepare("SELECT event, request_hash FROM advisory_outcomes ORDER BY sequence")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(
        stored,
        texts.iter().map(|t| (t.clone(), None)).collect::<Vec<_>>()
    );

    // Old decisions count as UNKNOWN in the paired section, never as a pair.
    let report = store.advisory_report(250).unwrap();
    assert_eq!(report.decisions, 2);
    assert_eq!(report.paired_trials.unknown_request_hash, 2);
    assert_eq!(report.paired_trials.trials, 0);
    assert!(report.paired_trials.arms.is_empty());

    // The migrated store takes outcomes with a request hash.
    store
        .record_advisory_outcome(followed("c", Some("e"), "stable-v1"), 260)
        .unwrap();
    assert_eq!(store.advisory_outcomes(261).unwrap().len(), 4);
}

#[test]
fn the_request_hash_is_stored_beside_the_event_and_checked_on_read() {
    let (temp, store) = setup();
    let event = store
        .record_advisory_outcome(followed("a", Some("e"), "stable-v1"), 200)
        .unwrap();
    assert_eq!(
        event.input.binding.request_hash.as_deref(),
        Some("e".repeat(64).as_str())
    );
    drop(store);
    let file = temp.path().join("feedback/feedback.sqlite3");
    let column: Option<String> = rusqlite::Connection::open(&file)
        .unwrap()
        .query_row("SELECT request_hash FROM advisory_outcomes", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(column, Some("e".repeat(64)));
    // A column that disagrees with the hashed event is refused on open.
    rusqlite::Connection::open(&file)
        .unwrap()
        .execute_batch(&format!(
            "DROP TRIGGER advisory_outcomes_immutable; UPDATE advisory_outcomes SET request_hash='{}';",
            "f".repeat(64)
        ))
        .unwrap();
    assert!(matches!(
        FeedbackStore::open(&temp.path().join("feedback")),
        Err(FeedbackError::Integrity("advisory outcome"))
    ));
}

#[test]
fn a_request_hash_that_is_not_a_digest_is_refused() {
    let (_t, store) = setup();
    let mut input = followed("a", None, "stable-v1");
    input.binding.request_hash = Some("x".repeat(64));
    assert!(matches!(
        store.record_advisory_outcome(input, 200),
        Err(FeedbackError::Invalid("advisory binding"))
    ));
}

#[test]
fn a_later_attempt_cannot_move_a_decision_to_another_request() {
    let (_t, store) = setup();
    store
        .record_advisory_outcome(followed("a", Some("e"), "stable-v1"), 200)
        .unwrap();
    let moved = overridden("a", Some("f"), "stable-v1");
    assert!(matches!(
        store.record_advisory_outcome(moved, 201),
        Err(FeedbackError::Invalid("advisory request hash"))
    ));
    // The same request, or an older export that does not state it, is accepted.
    store
        .record_advisory_outcome(overridden("a", Some("e"), "stable-v1"), 202)
        .unwrap();
    store
        .record_advisory_outcome(followed("a", None, "stable-v1"), 203)
        .unwrap();
    assert_eq!(store.advisory_outcomes(204).unwrap().len(), 3);
}

#[test]
fn report_counts_paired_trials() {
    let (_t, store) = setup();
    // Trial "e": both policies, v1 followed, v2 overridden (neither plan is
    // "the" plan for the report: each arm counts in its own policy row).
    store
        .record_advisory_outcome(followed("a", Some("e"), "stable-v1"), 200)
        .unwrap();
    store
        .record_advisory_outcome(overridden("b", Some("e"), "stable-v2"), 200)
        .unwrap();
    // Trial "f": a correction of the v2 arm counts once, with its latest attempt.
    store
        .record_advisory_outcome(followed("c", Some("f"), "stable-v1"), 200)
        .unwrap();
    store
        .record_advisory_outcome(overridden("d", Some("f"), "stable-v2"), 200)
        .unwrap();
    store
        .record_advisory_outcome(followed("d", Some("f"), "stable-v2"), 201)
        .unwrap();
    // One arm only, and one decision recorded before the key existed.
    store
        .record_advisory_outcome(followed("1", Some("0"), "stable-v2"), 201)
        .unwrap();
    store
        .record_advisory_outcome(followed("2", None, "stable-v1"), 201)
        .unwrap();

    let report = store.advisory_report(300).unwrap();
    let paired = &report.paired_trials;
    assert_eq!(paired.pairing_key, "request_hash");
    assert_eq!(paired.trials, 2);
    assert_eq!(paired.unpaired_decisions, 1);
    assert_eq!(paired.unknown_request_hash, 1);
    assert_eq!(paired.ambiguous_trials, 0);
    assert_eq!(paired.arms.len(), 2);

    let v1 = arm_row(&report, "stable-v1", "catalog-1");
    assert_eq!(v1.trials, 2);
    assert_eq!(v1.adherence.followed.sample_size, 2);
    assert_eq!(v1.adherence.overridden.sample_size, 0);
    let v2 = arm_row(&report, "stable-v2", "catalog-1");
    assert_eq!(v2.trials, 2);
    assert_eq!(v2.adherence.followed.sample_size, 1);
    assert_eq!(v2.adherence.followed.records, 2);
    assert_eq!(v2.adherence.overridden.sample_size, 1);
    // Unpaired and UNKNOWN decisions stay out of every arm.
    assert_eq!(
        paired
            .arms
            .iter()
            .map(|arm| arm.adherence.followed.sample_size + arm.adherence.overridden.sample_size)
            .sum::<u64>(),
        4
    );
    // Below the minimum sample the arm prints counts, not rates.
    assert!(v1.insufficient_sample);
    assert_eq!(v1.adherence.followed.accepted_as_is.numerator, 2);
    assert_eq!(v1.adherence.followed.accepted_as_is.denominator, 2);
    assert_eq!(v1.adherence.followed.accepted_as_is.rate_micros, None);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema_version"], 4);
    assert_eq!(
        json["paired_trials"]["arms"][0]["policy_version"],
        "stable-v1"
    );
}

#[test]
fn report_counts_paired_trials_with_more_than_two_arms() {
    let (_t, store) = setup();
    // One request under three configurations: two policies on one catalog and
    // one policy on a second catalog.
    store
        .record_advisory_outcome(followed("a", Some("e"), "stable-v1"), 200)
        .unwrap();
    store
        .record_advisory_outcome(overridden("b", Some("e"), "stable-v2"), 200)
        .unwrap();
    store
        .record_advisory_outcome(
            arm(
                "c",
                Some("e"),
                "stable-v2",
                "catalog-2",
                AdvisoryAdherence::Followed,
                AdvisoryResult::AcceptedAfterRework,
            ),
            200,
        )
        .unwrap();
    let report = store.advisory_report(300).unwrap();
    let paired = &report.paired_trials;
    assert_eq!(paired.trials, 1);
    assert_eq!(paired.arms.len(), 3);
    for (policy, catalog, followed, overridden) in [
        ("stable-v1", "catalog-1", 1, 0),
        ("stable-v2", "catalog-1", 0, 1),
        ("stable-v2", "catalog-2", 1, 0),
    ] {
        let row = arm_row(&report, policy, catalog);
        assert_eq!(row.trials, 1, "{policy}/{catalog}");
        assert_eq!(row.adherence.followed.sample_size, followed);
        assert_eq!(row.adherence.overridden.sample_size, overridden);
    }
    // stable-v2 on catalog-2 is not folded into stable-v2 on catalog-1.
    assert_eq!(
        arm_row(&report, "stable-v2", "catalog-2")
            .adherence
            .followed
            .accepted_after_rework
            .numerator,
        1
    );
}

#[test]
fn a_trial_with_two_decisions_of_one_configuration_is_not_counted_as_arms() {
    let (_t, store) = setup();
    store
        .record_advisory_outcome(followed("a", Some("e"), "stable-v1"), 200)
        .unwrap();
    store
        .record_advisory_outcome(overridden("b", Some("e"), "stable-v1"), 200)
        .unwrap();
    store
        .record_advisory_outcome(followed("c", Some("e"), "stable-v2"), 200)
        .unwrap();
    let paired = store.advisory_report(300).unwrap().paired_trials;
    assert_eq!(paired.trials, 0);
    assert_eq!(paired.ambiguous_trials, 1);
    assert!(paired.arms.is_empty());
}

#[test]
fn paired_rates_appear_from_the_minimum_sample() {
    let (_t, store) = setup();
    for (index, request) in ["0", "1", "2", "3", "4"].iter().enumerate() {
        let left = ["a", "b", "c", "d", "e"][index];
        let right = ["5", "6", "7", "8", "9"][index];
        store
            .record_advisory_outcome(followed(left, Some(request), "stable-v1"), 200)
            .unwrap();
        store
            .record_advisory_outcome(overridden(right, Some(request), "stable-v2"), 200)
            .unwrap();
    }
    let report = store.advisory_report(300).unwrap();
    assert_eq!(report.paired_trials.trials, ADVISORY_MINIMUM_SAMPLE);
    let v1 = arm_row(&report, "stable-v1", "catalog-1");
    assert!(!v1.insufficient_sample);
    assert_eq!(
        v1.adherence.followed.accepted_as_is.rate_micros,
        Some(1_000_000)
    );
    // The overridden side of the v1 arm is empty: no rate, not zero.
    assert_eq!(v1.adherence.overridden.abandoned.rate_micros, None);
    let v2 = arm_row(&report, "stable-v2", "catalog-1");
    assert_eq!(
        v2.adherence.overridden.abandoned.rate_micros,
        Some(1_000_000)
    );
}
