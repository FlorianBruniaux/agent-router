// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Manual advisory outcomes: schema v3, one immutable outcome per saved
//! decision, consent, retention, purge, export and the followed/overridden
//! report with its sample sizes and UNKNOWN counts.
use std::collections::BTreeSet;

use router_feedback::*;
use router_types::*;
use tempfile::TempDir;

const DAY: u64 = 86400;

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
fn phase(model: &str) -> AdvisoryPhase {
    AdvisoryPhase {
        stage: StageKind::Execute,
        model: ModelId::parse(model).unwrap(),
        effort: EffortLevel::High,
        skills: BTreeSet::from(["tdd".to_owned()]),
    }
}
fn outcome(
    key: &str,
    kind: TaskKind,
    adherence: AdvisoryAdherence,
    result: AdvisoryResult,
) -> AdvisoryOutcomeInput {
    AdvisoryOutcomeInput {
        schema_version: 1,
        observed_at_unix_seconds: 150,
        binding: AdvisoryBinding {
            decision_id: key.repeat(64),
            catalog_version: "catalog-1".into(),
            policy_version: "policy-1".into(),
            task_kind: kind,
            risk: RiskLevel::Medium,
            selection_basis: AdvisorySelectionBasis::CatalogPolicyPrior,
            recommended: vec![phase("model-a")],
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
fn followed(key: &str, result: AdvisoryResult) -> AdvisoryOutcomeInput {
    outcome(
        key,
        TaskKind::Implement,
        AdvisoryAdherence::Followed,
        result,
    )
}
fn overridden(key: &str, result: AdvisoryResult) -> AdvisoryOutcomeInput {
    outcome(
        key,
        TaskKind::Implement,
        AdvisoryAdherence::Overridden,
        result,
    )
}

#[test]
fn recording_needs_collection_consent() {
    let temp = TempDir::new().unwrap();
    let store = FeedbackStore::open(&temp.path().join("feedback")).unwrap();
    assert!(matches!(
        store.record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 100),
        Err(FeedbackError::Consent)
    ));
    assert!(matches!(
        store.advisory_report(100),
        Err(FeedbackError::Consent)
    ));
}

#[test]
fn an_exact_repeat_is_idempotent_and_a_correction_is_a_new_attempt() {
    let (_temp, store) = setup();
    let first = store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 200)
        .unwrap();
    assert_eq!(first.attempt, 1);
    // The same outcome at another second is the same outcome.
    let mut later = followed("a", AdvisoryResult::AcceptedAsIs);
    later.observed_at_unix_seconds = 170;
    assert_eq!(store.record_advisory_outcome(later, 300).unwrap(), first);
    // The same task advised again has the same decision id: a different
    // result is accepted as the next attempt, not refused.
    let second = store
        .record_advisory_outcome(followed("a", AdvisoryResult::Abandoned), 301)
        .unwrap();
    assert_eq!((second.attempt, second.sequence), (2, 2));
    // Going back to an earlier result is also a new attempt.
    let third = store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 302)
        .unwrap();
    assert_eq!(third.attempt, 3);
    assert_eq!(store.advisory_outcomes(303).unwrap().len(), 3);
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::Abandoned), 304)
        .unwrap();

    let report = store.advisory_report(305).unwrap();
    // One decision, counted once with its latest record.
    assert_eq!((report.decisions, report.records), (1, 4));
    let f = report.overall.followed;
    assert_eq!((f.sample_size, f.records), (1, 4));
    assert_eq!(f.abandoned.numerator, 1);
    assert_eq!(f.accepted_as_is.numerator, 0);
}

#[test]
fn incoherent_or_malformed_outcomes_are_refused() {
    let (_temp, store) = setup();
    let mut cases = Vec::new();
    // Followed states no substitute.
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.used = overridden("a", AdvisoryResult::AcceptedAsIs).used;
    cases.push(c);
    // An override that says nothing is not an override.
    let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
    c.used = None;
    cases.push(c);
    let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
    c.used = Some(AdvisoryUsed {
        model: None,
        effort: None,
        skills: None,
    });
    cases.push(c);
    // Free text is not an identifier.
    let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
    c.used = Some(AdvisoryUsed {
        model: Some(ModelId::parse("fix the login page in /home/me").unwrap()),
        effort: None,
        skills: None,
    });
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.binding.recommended[0].skills = BTreeSet::from(["a skill with spaces".to_owned()]);
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.binding.decision_id = "short".into();
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.binding.recommended.clear();
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.schema_version = 2;
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.measurements.cost_microusd = Some(MAX_VALUE + 1);
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.observed_at_unix_seconds = 99;
    cases.push(c);
    let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
    c.observed_at_unix_seconds = 201;
    cases.push(c);
    // A path is refused wherever the user or advice.json can put one, even
    // without a space in it.
    for path in [
        "/Users/me/clients/acme/secret-repo",
        "../secret",
        ".hidden",
        "~/notes",
        "dir\\file",
        "a/b",
    ] {
        let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
        c.used = Some(AdvisoryUsed {
            model: Some(ModelId::parse(path).unwrap()),
            effort: None,
            skills: None,
        });
        cases.push(c);
        let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
        c.used = Some(AdvisoryUsed {
            model: None,
            effort: None,
            skills: Some(BTreeSet::from([path.to_owned()])),
        });
        cases.push(c);
        let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
        c.binding.recommended[0].model = ModelId::parse(path).unwrap();
        cases.push(c);
        let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
        c.binding.recommended[0].skills = BTreeSet::from([path.to_owned()]);
        cases.push(c);
        let mut c = followed("a", AdvisoryResult::AcceptedAsIs);
        c.binding.catalog_version = path.into();
        cases.push(c);
    }
    // Restating the whole recommendation is not an override.
    let mut c = overridden("a", AdvisoryResult::AcceptedAsIs);
    c.used = Some(AdvisoryUsed {
        model: Some(ModelId::parse("model-a").unwrap()),
        effort: Some(EffortLevel::High),
        skills: Some(BTreeSet::from(["tdd".to_owned()])),
    });
    cases.push(c);
    for case in cases {
        assert!(store.record_advisory_outcome(case, 200).is_err());
    }
    assert!(store.advisory_outcomes(200).unwrap().is_empty());
}

#[test]
fn a_missing_cost_stays_unknown_and_is_never_stored_as_zero() {
    let (_temp, store) = setup();
    let mut known = followed("a", AdvisoryResult::AcceptedAsIs);
    known.measurements.cost_microusd = Some(0);
    store.record_advisory_outcome(known, 200).unwrap();
    store
        .record_advisory_outcome(followed("b", AdvisoryResult::AcceptedAsIs), 200)
        .unwrap();
    let stored = store.advisory_outcomes(201).unwrap();
    assert_eq!(stored[0].input.measurements.cost_microusd, Some(0));
    assert_eq!(stored[1].input.measurements.cost_microusd, None);
    assert_eq!(stored[1].input.measurements.tokens, None);
    let json = serde_json::to_value(&stored[1]).unwrap();
    assert!(json["input"]["measurements"]["cost_microusd"].is_null());
    let report = store.advisory_report(202).unwrap();
    // An explicit zero is a known cost; the absent one is counted as UNKNOWN.
    assert_eq!(report.overall.followed.sample_size, 2);
    assert_eq!(report.overall.followed.unknown.cost, 1);
    assert_eq!(report.overall.followed.unknown.tokens, 2);
}

#[test]
fn the_report_compares_followed_and_overridden_with_sizes_and_unknown_counts() {
    let (_temp, store) = setup();
    for (key, input) in [
        ("1", followed("1", AdvisoryResult::AcceptedAsIs)),
        ("2", followed("2", AdvisoryResult::AcceptedAfterRework)),
        ("3", followed("3", AdvisoryResult::AcceptedAsIs)),
        ("4", overridden("4", AdvisoryResult::Abandoned)),
        ("5", overridden("5", AdvisoryResult::AcceptedAsIs)),
    ] {
        let _ = key;
        store.record_advisory_outcome(input, 200).unwrap();
    }
    let mut review = outcome(
        "6",
        TaskKind::Review,
        AdvisoryAdherence::Overridden,
        AdvisoryResult::AcceptedAsIs,
    );
    review.binding.selection_basis = AdvisorySelectionBasis::UnratedTieBreak;
    review.used = Some(AdvisoryUsed {
        model: None,
        effort: Some(EffortLevel::Low),
        skills: Some(BTreeSet::new()),
    });
    store.record_advisory_outcome(review, 200).unwrap();

    let report = store.advisory_report(300).unwrap();
    assert_eq!(report.decisions, 6);
    assert_eq!(report.attribution, "not_established");
    let implement = report
        .task_kinds
        .iter()
        .find(|k| k.task_kind == TaskKind::Implement)
        .unwrap();
    let f = implement.adherence.followed;
    assert_eq!(f.sample_size, 3);
    assert_eq!(f.accepted_as_is.numerator, 2);
    assert_eq!(f.accepted_as_is.denominator, 3);
    assert_eq!(f.accepted_as_is.rate_micros, Some(666_667));
    assert_eq!(f.accepted_after_rework.rate_micros, Some(333_333));
    assert_eq!(f.abandoned.rate_micros, Some(0));
    // Not applicable to followed outcomes, so null rather than zero.
    assert_eq!(f.unknown.used_model, None);
    let o = implement.adherence.overridden;
    assert_eq!(o.sample_size, 2);
    assert_eq!(o.abandoned.rate_micros, Some(500_000));
    assert_eq!(o.unknown.used_model, Some(0));
    assert_eq!(o.unknown.used_effort, Some(2));
    assert_eq!(o.unknown.used_skills, Some(2));
    // A kind with no followed outcome has no followed rate, not a zero rate.
    let review = report
        .task_kinds
        .iter()
        .find(|k| k.task_kind == TaskKind::Review)
        .unwrap();
    assert_eq!(review.adherence.followed.sample_size, 0);
    assert_eq!(review.adherence.followed.accepted_as_is.rate_micros, None);
    assert_eq!(review.adherence.overridden.unknown.used_model, Some(1));
    assert_eq!(review.adherence.overridden.unknown.used_skills, Some(0));
    assert_eq!(
        review
            .adherence
            .overridden
            .selection_basis
            .unrated_tie_break,
        1
    );
    assert_eq!(report.overall.followed.sample_size, 3);
    assert_eq!(report.overall.overridden.sample_size, 3);
    // Deterministic.
    assert_eq!(store.advisory_report(300).unwrap(), report);
}

#[test]
fn nothing_but_enums_identifiers_and_numbers_is_stored() {
    let (temp, store) = setup();
    let mut input = overridden("a", AdvisoryResult::AcceptedAfterRework);
    input.used = Some(AdvisoryUsed {
        model: Some(ModelId::parse("model-b").unwrap()),
        effort: Some(EffortLevel::Low),
        skills: Some(BTreeSet::from(["review".to_owned()])),
    });
    store.record_advisory_outcome(input, 200).unwrap();
    drop(store);
    let raw: String = rusqlite::Connection::open(temp.path().join("feedback/feedback.sqlite3"))
        .unwrap()
        .query_row("SELECT event FROM advisory_outcomes", [], |r| r.get(0))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // Every string leaf is a hash, an enum value or a registered identifier.
    fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) => out.push(text.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
            serde_json::Value::Object(fields) => fields.values().for_each(|v| strings(v, out)),
            _ => {}
        }
    }
    let mut leaves = Vec::new();
    strings(&value, &mut leaves);
    assert!(!leaves.is_empty());
    for leaf in leaves {
        assert!(
            leaf.len() <= 64
                && leaf
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)),
            "unexpected stored text: {leaf}"
        );
    }
    let fields: BTreeSet<_> = value["input"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        fields,
        BTreeSet::from(
            [
                "adherence",
                "binding",
                "measurements",
                "observed_at_unix_seconds",
                "result",
                "schema_version",
                "used"
            ]
            .map(String::from)
        )
    );
}

#[test]
fn retention_purge_consent_off_and_export_cover_outcomes() {
    let (temp, store) = setup();
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 200)
        .unwrap();
    let export = store.export(300).unwrap();
    assert_eq!(export.advisory_outcomes.len(), 1);
    assert!(
        serde_json::to_string(&export)
            .unwrap()
            .contains("advisory_outcomes")
    );
    // Observed at 150, retained 30 days.
    assert_eq!(store.advisory_outcomes(150 + 30 * DAY).unwrap().len(), 1);
    assert!(
        store
            .advisory_outcomes(150 + 30 * DAY + 1)
            .unwrap()
            .is_empty()
    );

    let mut later = followed("b", AdvisoryResult::Abandoned);
    later.observed_at_unix_seconds = 150 + 30 * DAY + 5;
    store
        .record_advisory_outcome(later, 150 + 30 * DAY + 6)
        .unwrap();
    assert_eq!(store.purge_all(150 + 30 * DAY + 10).unwrap(), 1);
    assert!(
        store
            .advisory_outcomes(150 + 30 * DAY + 11)
            .unwrap()
            .is_empty()
    );
    assert!(
        !serde_json::to_string(&store.export(150 + 30 * DAY + 12).unwrap())
            .unwrap()
            .contains("advisory_outcomes")
    );

    store.configure(prefs(), 150 + 30 * DAY + 20).unwrap();
    let mut again = followed("c", AdvisoryResult::AcceptedAsIs);
    again.observed_at_unix_seconds = 150 + 30 * DAY + 21;
    store
        .record_advisory_outcome(again, 150 + 30 * DAY + 22)
        .unwrap();
    store
        .configure(
            FeedbackPreferences {
                collection_enabled: false,
                ..prefs()
            },
            150 + 30 * DAY + 30,
        )
        .unwrap();
    let count: u64 = rusqlite::Connection::open(temp.path().join("feedback/feedback.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM advisory_outcomes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn an_export_without_outcomes_omits_the_key() {
    let (_temp, store) = setup();
    let export = store.export(300).unwrap();
    assert!(export.advisory_outcomes.is_empty());
    assert!(
        !serde_json::to_string(&export)
            .unwrap()
            .contains("advisory_outcomes")
    );
}

#[test]
fn an_outcome_cannot_be_updated_through_sql() {
    let (temp, store) = setup();
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 200)
        .unwrap();
    drop(store);
    let db = rusqlite::Connection::open(temp.path().join("feedback/feedback.sqlite3")).unwrap();
    assert!(
        db.execute("UPDATE advisory_outcomes SET observed_at=1", [])
            .is_err()
    );
}

fn fixture() -> FeedbackInput {
    parse_feedback(include_bytes!(
        "../../../tests/fixtures/feedback/automated-unknown-v1.json"
    ))
    .unwrap()
}

#[test]
fn report_summary_and_events_ignore_advisory_outcomes() {
    let (_temp, store) = setup();
    store.record(fixture(), 200).unwrap();
    let before = (
        store.report(None, 250).unwrap(),
        store.summary(None, 250).unwrap(),
        store.events(250).unwrap(),
    );
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 250)
        .unwrap();
    store
        .record_advisory_outcome(overridden("b", AdvisoryResult::Abandoned), 250)
        .unwrap();
    let after = (
        store.report(None, 250).unwrap(),
        store.summary(None, 250).unwrap(),
        store.events(250).unwrap(),
    );
    assert_eq!(before, after);
    assert_eq!(after.0.automated_events, 1);
    // With outcomes only, the ordinary report is empty.
    let (_temp, store) = setup();
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 250)
        .unwrap();
    let report = store.report(None, 250).unwrap();
    assert_eq!(
        (report.automated_events, report.human_events, report.facts),
        (0, 0, 0)
    );
    assert!(report.groups.is_empty());
    assert_eq!(store.summary(None, 250).unwrap().runs, 0);
}

/// The schema exactly as the version 2 code created it.
const V2_SCHEMA: &str = "CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE feedback(sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_hash TEXT NOT NULL, origin TEXT NOT NULL, observed_at INTEGER NOT NULL, event TEXT NOT NULL, UNIQUE(run_hash,origin));
    CREATE TRIGGER feedback_immutable BEFORE UPDATE ON feedback BEGIN SELECT RAISE(ABORT,'immutable feedback'); END;
    CREATE TABLE policies(id TEXT PRIMARY KEY, artifact TEXT NOT NULL, state TEXT NOT NULL);
    CREATE TRIGGER policy_artifacts_immutable BEFORE UPDATE OF artifact ON policies BEGIN SELECT RAISE(ABORT,'immutable policy artifact'); END;
    CREATE TABLE policy_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT, artifact_id TEXT NOT NULL REFERENCES policies(id), event TEXT NOT NULL);
    CREATE TRIGGER policy_events_immutable BEFORE UPDATE ON policy_events BEGIN SELECT RAISE(ABORT,'immutable policy event'); END;
    CREATE TABLE facts(sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_key TEXT NOT NULL, kind TEXT NOT NULL, observed_at INTEGER NOT NULL, fact TEXT NOT NULL, UNIQUE(run_key,kind));
    CREATE TRIGGER facts_immutable BEFORE UPDATE ON facts BEGIN SELECT RAISE(ABORT,'immutable fact'); END;
    PRAGMA user_version=2;";

#[test]
fn a_version_2_store_migrates_keeping_its_events_and_facts() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("feedback");
    // A version 2 store made by the previous schema, holding an event and a fact.
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = path.join("feedback.sqlite3");
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch(V2_SCHEMA).unwrap();
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
    drop(db);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    // Use the current code to write the event and the fact, then rewind the
    // schema: the rows are what the v2 code would have stored.
    let event_text;
    {
        let store = FeedbackStore::open(&path).unwrap();
        store.record(fixture(), 200).unwrap();
        store
            .record_fact(
                FeedbackFact {
                    schema_version: 1,
                    run_key_sha256: "a".repeat(64),
                    observed_at_unix_seconds: 150,
                    kind: FeedbackFactKind::Integration {
                        outcome: IntegrationOutcome::Applied,
                    },
                },
                200,
            )
            .unwrap();
        event_text = serde_json::to_string(&store.events(200).unwrap()[0]).unwrap();
    }
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch(
        "DROP TRIGGER advisory_outcomes_immutable; DROP TABLE advisory_outcomes; PRAGMA user_version=2;",
    )
    .unwrap();
    drop(db);
    let version = |p: &std::path::Path| -> u32 {
        rusqlite::Connection::open(p)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    };
    assert_eq!(version(&file), 2);

    let store = FeedbackStore::open(&path).unwrap();
    assert_eq!(version(&file), 3);
    let events = store.events(250).unwrap();
    assert_eq!(serde_json::to_string(&events[0]).unwrap(), event_text);
    assert_eq!(store.facts(250).unwrap().len(), 1);
    store
        .record_advisory_outcome(followed("a", AdvisoryResult::AcceptedAsIs), 260)
        .unwrap();
    assert_eq!(store.advisory_outcomes(261).unwrap().len(), 1);
}
