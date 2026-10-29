// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::*;
use router_types::{VerificationOutcome, VerificationRecord};

fn fixture() -> (tempfile::TempDir, Store) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("runtime")).unwrap();
    let mut run = tests::summary("run");
    run.state = RunState::Verifying;
    store.register(&run).unwrap();
    (temp, store)
}
fn record(outcome: VerificationOutcome) -> VerificationRecord {
    VerificationRecord {
        verifier: "explicit-checker".into(),
        evidence_sha256: "e".repeat(64),
        artifact_sha256: None,
        validation: VerificationOutcome::Passed,
        objective: outcome,
    }
}

#[test]
fn verification_history_rolls_back_with_projection_and_transition() {
    for failed_kind in [
        "verification.attested.v1",
        "verification.recorded",
        "transition",
    ] {
        let (_temp, store) = fixture();
        let first = record(VerificationOutcome::Unknown);
        store.record_verification("run", &first, None).unwrap();
        let events = store.events().unwrap();
        // Fail after the projection write, including after both history events
        // on the state-transition path. The whole transaction must roll back.
        store.lock().unwrap().execute_batch(&format!(
            "CREATE TRIGGER fail_verification BEFORE INSERT ON events WHEN NEW.kind='{failed_kind}' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;"
        )).unwrap();
        assert!(
            store
                .record_verification(
                    "run",
                    &record(VerificationOutcome::Passed),
                    Some((RunState::Verified, None))
                )
                .is_err(),
            "{failed_kind}"
        );
        assert_eq!(store.verification_record("run").unwrap(), Some(first));
        assert_eq!(store.get("run").unwrap().state, RunState::Verifying);
        assert_eq!(store.events().unwrap(), events);
        store.verify_events().unwrap();
    }
}

#[test]
fn verification_history_refuses_corrupted_chain_before_writing() {
    let (_temp, store) = fixture();
    let first = record(VerificationOutcome::Unknown);
    store.record_verification("run", &first, None).unwrap();
    store.lock().unwrap().execute_batch(
        "DROP TRIGGER events_no_update; UPDATE events SET detail='tampered' WHERE sequence=(SELECT max(sequence) FROM events);"
    ).unwrap();
    let events = store.events().unwrap();
    assert!(matches!(
        store.record_verification(
            "run",
            &record(VerificationOutcome::Passed),
            Some((RunState::Verified, None))
        ),
        Err(RuntimeError::Integrity("event chain"))
    ));
    assert_eq!(store.verification_record("run").unwrap(), Some(first));
    assert_eq!(store.events().unwrap(), events);
    assert_eq!(store.get("run").unwrap().state, RunState::Verifying);
}

#[test]
fn verification_history_preserves_legacy_current_record_as_snapshot_only() {
    for mixed_writer in [false, true] {
        let (temp, store) = fixture();
        if mixed_writer {
            let mut earlier = record(VerificationOutcome::Unknown);
            earlier.validation = VerificationOutcome::Unknown;
            store.record_verification("run", &earlier, None).unwrap();
        }
        let old = record(VerificationOutcome::Unknown);
        {
            // Reproduce the previous writer: mutable record plus digest-only event.
            let mut db = store.lock().unwrap();
            let tx = db.transaction().unwrap();
            tx.execute(
                "INSERT OR REPLACE INTO verifications VALUES(?1,?2)",
                params!["run", serde_json::to_string(&old).unwrap()],
            )
            .unwrap();
            let run = load_opt(&tx, "run").unwrap().unwrap();
            append(&tx, &run, "verification.recorded", &old.evidence_sha256).unwrap();
            tx.commit().unwrap();
        }
        let before = store.events().unwrap();
        drop(store);
        let store = Store::open(&temp.path().join("runtime")).unwrap();
        assert_eq!(store.events().unwrap(), before); // opening never backfills history
        let new = record(VerificationOutcome::Passed);
        // Same evidence hash, different outcome: hashes alone cannot reconstruct it.
        store
            .record_verification("run", &new, Some((RunState::Verified, None)))
            .unwrap();
        drop(store);
        let store = Store::open(&temp.path().join("runtime")).unwrap();
        let events = store.events().unwrap();
        assert_eq!(&events[..before.len()], before);
        let snapshots: Vec<_> = events
            .iter()
            .filter(|e| e.kind == "verification.legacy_snapshot.v1")
            .collect();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            serde_json::from_str::<VerificationRecord>(&snapshots[0].detail).unwrap(),
            old
        );
        let history: Vec<_> = events
            .iter()
            .filter(|e| e.kind == "verification.attested.v1")
            .collect();
        assert_eq!(history.len(), if mixed_writer { 2 } else { 1 });
        assert_eq!(
            serde_json::from_str::<VerificationRecord>(&history.last().unwrap().detail).unwrap(),
            new
        );
        assert_eq!(store.verification_record("run").unwrap(), Some(new));
        assert_eq!(store.get("run").unwrap().state, RunState::Verified);
    }
}
#[test]
fn verification_history_limit_failure_cannot_overwrite_current_attestation() {
    let (_temp, store) = fixture();
    let first = record(VerificationOutcome::Unknown);
    store.record_verification("run", &first, None).unwrap();
    let events = store.events().unwrap();
    let mut oversized = record(VerificationOutcome::Passed);
    oversized.verifier = "x".repeat(2049);
    assert!(matches!(
        store.record_verification("run", &oversized, Some((RunState::Verified, None))),
        Err(RuntimeError::Limit)
    ));
    assert_eq!(store.verification_record("run").unwrap(), Some(first));
    assert_eq!(store.get("run").unwrap().state, RunState::Verifying);
    assert_eq!(store.events().unwrap(), events);
}

#[test]
fn verification_history_concurrent_conclusions_retain_only_the_winner() {
    let (_temp, store) = fixture();
    let first = record(VerificationOutcome::Unknown);
    store.record_verification("run", &first, None).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let tasks: Vec<_> = ["checker-a", "checker-b"]
            .into_iter()
            .map(|name| {
                let store = &store;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut record = record(VerificationOutcome::Passed);
                    record.verifier = name.into();
                    barrier.wait();
                    store.record_verification("run", &record, Some((RunState::Verified, None)))
                })
            })
            .collect();
        tasks
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(RuntimeError::Verification)))
            .count(),
        1
    );
    let history: Vec<VerificationRecord> = store
        .events()
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "verification.attested.v1")
        .map(|e| serde_json::from_str(&e.detail).unwrap())
        .collect();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0], first);
    assert_eq!(
        store.verification_record("run").unwrap(),
        Some(history[1].clone())
    );
    assert_eq!(history[1].objective, VerificationOutcome::Passed);
    store.verify_events().unwrap();
}
