//! Synthetic stores, never provider data. These tests catch incoherent snapshots,
//! recovery-on-read, private field leaks and decoding before size admission.
use router_runtime::{Runtime, RuntimeError, RuntimeLimits, observe_run};
use router_types::{
    OutputRetentionPolicy, ProcessCleanupEvidence, RunState, RunSummary, RuntimeEvent,
};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
};

const CANARY: &str = "PRIVATE_PROMPT_PROVIDER_SECRET_CANARY";
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("runtime");
        drop(Runtime::open(&root, RuntimeLimits::default()).unwrap());
        Self { _temp: temp, root }
    }
    fn db(&self) -> Connection {
        let db = Connection::open(self.root.join("runtime.sqlite3")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(2)).unwrap();
        db
    }
    fn seed(&self) {
        let mut db = self.db();
        let tx = db.transaction().unwrap();
        let run = summary(RunState::Cancelled);
        tx.execute(
            "INSERT INTO runs VALUES(?1,?2,?3,?4)",
            params![
                CANARY,
                run.plan_id,
                run.node_id,
                serde_json::to_string(&run).unwrap()
            ],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO run_attempts VALUES(?1,1,'cancelled')",
            [CANARY],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO attempt_leases VALUES(?1,1,?2,0,NULL)",
            params![CANARY, CANARY],
        )
        .unwrap();
        append(&tx, RunState::Running, "attempt.started", CANARY);
        append(&tx, RunState::Running, "control.cancel_requested", CANARY);
        append(&tx, RunState::Cancelled, "transition", CANARY);
        tx.commit().unwrap();
    }
}
fn summary(state: RunState) -> RunSummary {
    RunSummary {
        run_id: CANARY.into(),
        definition_hash: "a".repeat(64),
        approval_digest: "b".repeat(64),
        receipt_id: CANARY.into(),
        plan_id: CANARY.into(),
        node_id: CANARY.into(),
        state,
        attempts: 1,
        failure: None,
        worktree: Some(CANARY.into()),
        artifact_sha256: None,
        output_retention: OutputRetentionPolicy::Disabled,
        workflow: None,
        artifact_contract_sha256: None,
    }
}
fn hash(domain: &str, value: &impl serde::Serialize) -> String {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update(serde_json::to_vec(value).unwrap());
    hex::encode(hash.finalize())
}
fn append(db: &Connection, state: RunState, kind: &str, detail: &str) {
    let (seq, prev): (u64, String) = db
        .query_row(
            "SELECT sequence,event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, String::new()));
    let e = RuntimeEvent {
        sequence: seq + 1,
        run_id: CANARY.into(),
        kind: kind.into(),
        state,
        detail: detail.into(),
        previous_hash: prev,
        event_hash: String::new(),
    };
    let digest = hash(
        "runtime-event-v1",
        &(
            e.sequence,
            &e.run_id,
            &e.kind,
            e.state,
            &e.detail,
            &e.previous_hash,
        ),
    );
    db.execute(
        "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            e.sequence,
            e.run_id,
            e.kind,
            serde_json::to_string(&e.state).unwrap(),
            e.detail,
            e.previous_hash,
            digest
        ],
    )
    .unwrap();
}
fn logical_state(db: &Connection) -> String {
    let summary: String = db
        .query_row("SELECT summary FROM runs", [], |r| r.get(0))
        .unwrap();
    let lease: (String, u32, Option<String>) = db
        .query_row(
            "SELECT record,resolved,evidence FROM attempt_leases",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let events: u32 = db
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    let blocks: u32 = db
        .query_row("SELECT COUNT(*) FROM cleanup_blocks", [], |r| r.get(0))
        .unwrap();
    format!("{summary}{lease:?}{events}{blocks}")
}
#[test]
fn cancelled_request_is_not_cleanup_and_all_private_fields_are_redacted() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    let before = logical_state(&db);
    fs::write(f.root.join("private-output"), CANARY).unwrap();
    let o = observe_run(&f.root, CANARY).unwrap();
    assert_eq!(o.schema_version, 1);
    assert_eq!(o.known.state, RunState::Cancelled);
    assert_eq!(o.known.attempts, 1);
    assert_eq!(o.known.cleanup, ProcessCleanupEvidence::Unknown);
    assert_eq!(o.known.events.len(), 3);
    assert_eq!(o.horizon.sequence, 3);
    assert_eq!(
        o.known.events.last().unwrap().event_sha256,
        o.horizon.event_sha256.clone().unwrap()
    );
    let json = serde_json::to_string(&o).unwrap();
    assert!(!json.contains(CANARY));
    assert!(!json.contains("worktree"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    for field in [
        "requested_model",
        "requested_effort",
        "observed_model",
        "observed_effort",
        "transport",
        "validation",
        "objective",
        "duration",
        "journal_authenticity",
        "process_liveness",
    ] {
        assert_eq!(value["unknown"][field], "unknown", "{field}");
    }
    assert_eq!(value["completeness"]["journal"], "complete");
    assert_eq!(value["completeness"]["external_observations"], "unknown");
    assert_eq!(logical_state(&db), before);
    assert_eq!(
        fs::read_to_string(f.root.join("private-output")).unwrap(),
        CANARY
    );
    assert_eq!(
        serde_json::to_string(&observe_run(&f.root, CANARY).unwrap()).unwrap(),
        json
    );
}
#[test]
fn resolved_group_cleanup_is_reported_without_implying_current_liveness() {
    let f = Fixture::new();
    f.seed();
    f.db()
        .execute(
            "UPDATE attempt_leases SET resolved=1,evidence='\"group_terminated\"'",
            [],
        )
        .unwrap();
    let o = observe_run(&f.root, CANARY).unwrap();
    assert_eq!(o.known.cleanup, ProcessCleanupEvidence::GroupTerminated);
    f.db()
        .execute("INSERT INTO cleanup_blocks VALUES(?1)", [CANARY])
        .unwrap();
    assert_eq!(
        observe_run(&f.root, CANARY).unwrap().known.cleanup,
        ProcessCleanupEvidence::Unknown
    );
}
#[test]
fn missing_store_and_missing_run_never_create_storage() {
    let temp = tempfile::tempdir().unwrap();
    let absent = temp.path().join("absent");
    assert!(observe_run(&absent, CANARY).is_err());
    assert!(!absent.exists());
    let f = Fixture::new();
    assert!(matches!(
        observe_run(&f.root, CANARY),
        Err(RuntimeError::NotFound)
    ));
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
}
#[test]
fn corrupt_hash_gap_and_summary_mismatch_refuse_reports() {
    for case in ["hash", "gap", "summary"] {
        let f = Fixture::new();
        f.seed();
        let db = f.db();
        db.execute_batch("DROP TRIGGER events_no_update; DROP TRIGGER events_no_delete;")
            .unwrap();
        match case {
            "hash" => {
                db.execute("UPDATE events SET event_hash=?1 WHERE sequence=2", [CANARY])
                    .unwrap();
            }
            "gap" => {
                db.execute("DELETE FROM events WHERE sequence=2", [])
                    .unwrap();
            }
            _ => {
                db.execute(
                    "UPDATE runs SET summary=?1",
                    [serde_json::to_string(&summary(RunState::Verified)).unwrap()],
                )
                .unwrap();
            }
        }
        let error = observe_run(&f.root, CANARY).unwrap_err();
        assert!(
            matches!(error, RuntimeError::Integrity(_)),
            "{case}: {error}"
        );
        assert!(!error.to_string().contains(CANARY));
    }
}
#[test]
fn oversize_untrusted_event_detail_is_refused_before_hash_or_json_decode() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    db.execute_batch("DROP TRIGGER events_no_update").unwrap();
    db.execute(
        "UPDATE events SET detail=?1,event_hash='invalid' WHERE sequence=1",
        [format!("{CANARY}{}", "x".repeat(65_536))],
    )
    .unwrap();
    let error = observe_run(&f.root, CANARY).unwrap_err();
    assert!(matches!(error, RuntimeError::Limit));
    assert!(!error.to_string().contains(CANARY));
}
#[test]
fn oversize_summary_and_cleanup_dependencies_are_refused_before_decode() {
    for column in [
        "summary",
        "evidence",
        "preparation",
        "admission",
        "authority",
    ] {
        let f = Fixture::new();
        f.seed();
        let db = f.db();
        let hostile = format!("{CANARY}{}", "x".repeat(65_536));
        match column {
            "summary" => {
                db.execute("UPDATE runs SET summary=?1", [&hostile])
                    .unwrap();
            }
            "evidence" => {
                db.execute("UPDATE attempt_leases SET evidence=?1", [&hostile])
                    .unwrap();
            }
            "preparation" => {
                db.execute(
                    "INSERT INTO preparation_cycles VALUES(?1,1,?2)",
                    params![CANARY, hostile],
                )
                .unwrap();
            }
            "admission" => {
                db.execute(
                    "INSERT INTO admission_intents VALUES(?1,?2)",
                    params![CANARY, hostile],
                )
                .unwrap();
            }
            _ => {
                db.execute(
                    "INSERT INTO workflow_instances VALUES(?1,?2,'{}')",
                    params![CANARY, hostile],
                )
                .unwrap();
            }
        }
        assert!(
            matches!(observe_run(&f.root, CANARY), Err(RuntimeError::Limit)),
            "{column}"
        );
    }
}
#[test]
fn run_event_ceiling_is_refused_before_decoding_invalid_rows() {
    let f = Fixture::new();
    f.seed();
    f.db().execute_batch("WITH RECURSIVE n(x) AS (SELECT 4 UNION ALL SELECT x+1 FROM n WHERE x<4097) INSERT INTO events SELECT x,'PRIVATE_PROMPT_PROVIDER_SECRET_CANARY','bad','bad','bad','bad','bad' FROM n;").unwrap();
    assert!(matches!(
        observe_run(&f.root, CANARY),
        Err(RuntimeError::Limit)
    ));
}
#[test]
fn unsupported_schema_is_not_migrated_and_errors_do_not_leak_private_data() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    db.pragma_update(None, "user_version", 10).unwrap();
    assert!(matches!(
        observe_run(&f.root, CANARY),
        Err(RuntimeError::Integrity(_))
    ));
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        10
    );
}
#[test]
fn concurrent_commits_never_mix_summary_cleanup_and_event_horizons() {
    let f = Fixture::new();
    f.seed();
    let root = f.root.clone();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = thread::spawn(move || {
        let mut db = Connection::open(root.join("runtime.sqlite3")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(2)).unwrap();
        writer_barrier.wait();
        for n in 0..80 {
            let tx = db.transaction().unwrap();
            let state = if n % 2 == 0 {
                RunState::Running
            } else {
                RunState::Cancelled
            };
            tx.execute(
                "UPDATE runs SET summary=?1",
                [serde_json::to_string(&summary(state)).unwrap()],
            )
            .unwrap();
            tx.execute(
                "UPDATE attempt_leases SET resolved=?1,evidence=?2",
                params![
                    u32::from(n % 2 == 1),
                    if n % 2 == 1 {
                        Some("\"group_terminated\"")
                    } else {
                        None
                    }
                ],
            )
            .unwrap();
            append(&tx, state, "transition", CANARY);
            tx.commit().unwrap();
            thread::yield_now();
        }
    });
    barrier.wait();
    for _ in 0..80 {
        let o = observe_run(&f.root, CANARY).unwrap();
        assert_eq!(o.known.state, o.known.events.last().unwrap().state);
        assert_eq!(o.horizon.sequence, o.known.events.last().unwrap().sequence);
        match o.known.state {
            RunState::Running => assert_eq!(o.known.cleanup, ProcessCleanupEvidence::Unknown),
            RunState::Cancelled => assert_eq!(
                o.known.cleanup,
                if o.horizon.sequence == 3 {
                    ProcessCleanupEvidence::Unknown
                } else {
                    ProcessCleanupEvidence::GroupTerminated
                }
            ),
            state => panic!("unexpected state {state:?}"),
        }
    }
    writer.join().unwrap();
    assert_eq!(observe_run(&f.root, CANARY).unwrap().horizon.sequence, 83);
}

#[test]
fn legacy_schema_keeps_cleanup_unknown_without_migration() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    db.pragma_update(None, "user_version", 2).unwrap();
    db.execute(
        "UPDATE attempt_leases SET resolved=1,evidence='\"group_terminated\"'",
        [],
    )
    .unwrap();
    assert_eq!(
        observe_run(&f.root, CANARY).unwrap().known.cleanup,
        ProcessCleanupEvidence::Unknown
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
}
#[test]
fn unknown_private_json_field_is_not_exposed_by_display_or_debug_errors() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    let mut value = serde_json::to_value(summary(RunState::Cancelled)).unwrap();
    value[CANARY] = serde_json::json!(CANARY);
    db.execute("UPDATE runs SET summary=?1", [value.to_string()])
        .unwrap();
    let error = observe_run(&f.root, CANARY).unwrap_err();
    assert!(!error.to_string().contains(CANARY));
    assert!(!format!("{error:?}").contains(CANARY));
}
#[test]
fn global_horizon_includes_other_runs_without_exporting_their_private_data() {
    let f = Fixture::new();
    f.seed();
    let db = f.db();
    let mut run = summary(RunState::Planned);
    run.run_id = "other_private_run".into();
    run.attempts = 0;
    db.execute(
        "INSERT INTO runs VALUES(?1,?2,?3,?4)",
        params![
            run.run_id,
            run.plan_id,
            run.node_id,
            serde_json::to_string(&run).unwrap()
        ],
    )
    .unwrap();
    let prev: String = db
        .query_row("SELECT event_hash FROM events WHERE sequence=3", [], |r| {
            r.get(0)
        })
        .unwrap();
    let hash = hash(
        "runtime-event-v1",
        &(
            4u64,
            "other_private_run",
            "transition",
            RunState::Planned,
            CANARY,
            &prev,
        ),
    );
    db.execute(
        "INSERT INTO events VALUES(4,?1,'transition','\"planned\"',?2,?3,?4)",
        params![run.run_id, CANARY, prev, hash],
    )
    .unwrap();
    let o = observe_run(&f.root, CANARY).unwrap();
    assert_eq!(o.horizon.sequence, 4);
    assert_eq!(o.known.events.last().unwrap().sequence, 3);
    assert!(
        !serde_json::to_string(&o)
            .unwrap()
            .contains("other_private_run")
    );
}
