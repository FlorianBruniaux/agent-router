//! Passive storage inspection. No Runtime::open, migrations or cleanup recovery.
mod files;
use crate::{
    leases::{LeaseObservation, LeaseRecord, observe_lease},
    store,
};
use router_doctor::{DatabaseSnapshotReport, DoctorError, Result, diagnose_database_snapshot_with};
use router_types::{DeepDoctorCode, DiagnosticState};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{path::Path, time::Duration};

const CURRENT_SCHEMA: u32 = 8;
const MAX_ROWS: u64 = 4096;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLockState {
    Available,
    Busy,
    Missing,
    Unknown,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSchemaState {
    RecognizedReadLayout,
    MigrationRequired,
    FutureVersion,
    InvalidLayout,
}
#[derive(Debug, Serialize)]
pub struct RuntimeStorageCounts {
    pub runs: u64,
    pub events: u64,
    pub writer_reservations: u64,
    pub cleanup_blocks: u64,
    pub preparation_cycles: u64,
    pub recorded_leases: u64,
    pub recorded_resolved_leases: u64,
}
#[derive(Debug, Default, Serialize)]
pub struct RuntimeLeaseObservations {
    pub held: u64,
    pub pending: u64,
    pub cleanup_marked: u64,
    pub missing: u64,
    pub ambiguous: u64,
}
#[derive(Debug, Serialize)]
pub struct RuntimeStorageReport {
    pub schema_version: u32,
    pub observed_at_unix_seconds: u64,
    pub overall: DiagnosticState,
    pub database: DatabaseSnapshotReport,
    pub storage_schema_version: u32,
    pub supported_storage_schema_version: u32,
    pub schema_layout: RuntimeSchemaState,
    pub journal_integrity: DiagnosticState,
    pub journal_authenticity: DiagnosticState,
    pub owner_lock: OwnerLockState,
    pub counts: Option<RuntimeStorageCounts>,
    pub leases: Option<RuntimeLeaseObservations>,
    pub database_and_leases_atomic: bool,
    pub process_liveness: DiagnosticState,
    pub cleanup_authority: DiagnosticState,
}
struct Inspection {
    version: u32,
    layout: RuntimeSchemaState,
    journal: DiagnosticState,
    counts: Option<RuntimeStorageCounts>,
    leases: Vec<(String, u32, String)>,
}

/// Inspect a private existing runtime. Read-only SQLite may coordinate its
/// WAL-index and read marks; this does not promise a filesystem with zero writes.
/// The supplied private directory and its ancestors must be operator-controlled.
/// No run recovery, schema migration, checkpoint or lease resolution is invoked.
pub fn diagnose_runtime_storage(root: &Path, now: u64) -> Result<RuntimeStorageReport> {
    let files = files::Files::open(root)?;
    let owner_lock = files.owner()?;
    let source = Connection::open_with_flags(
        files.path.join("runtime.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NOFOLLOW
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| DoctorError::Read)?;
    source
        .busy_timeout(Duration::ZERO)
        .map_err(|_| DoctorError::Read)?;
    files.check()?;
    let (database, inspected) = diagnose_database_snapshot_with(&source, inspect)?;
    files.check()?;
    // Keep SQLite's own descriptor lifetime intact, including during checks.
    drop(source);
    let leases = inspected.counts.as_ref().map(|_| {
        let mut observations = RuntimeLeaseObservations::default();
        for (id, attempt, encoded) in &inspected.leases {
            let observed = serde_json::from_str::<LeaseRecord>(encoded)
                .ok()
                .map(|record| observe_lease(&files.root, id, *attempt, &record))
                .unwrap_or(LeaseObservation::Ambiguous);
            match observed {
                LeaseObservation::Held => observations.held += 1,
                LeaseObservation::Pending => observations.pending += 1,
                LeaseObservation::CleanupMarked => observations.cleanup_marked += 1,
                LeaseObservation::Missing => observations.missing += 1,
                LeaseObservation::Ambiguous => observations.ambiguous += 1,
            }
        }
        observations
    });
    let overall = if database.integrity != DeepDoctorCode::DatabaseIntegrityValidated
        || matches!(
            inspected.layout,
            RuntimeSchemaState::FutureVersion | RuntimeSchemaState::InvalidLayout
        )
        || inspected.journal == DiagnosticState::Fail
    {
        DiagnosticState::Fail
    } else if inspected.layout != RuntimeSchemaState::RecognizedReadLayout
        || matches!(
            owner_lock,
            OwnerLockState::Missing | OwnerLockState::Unknown
        )
        || inspected
            .counts
            .as_ref()
            .is_some_and(|c| c.cleanup_blocks > 0)
        || leases
            .as_ref()
            .is_some_and(|l| l.pending + l.missing + l.ambiguous > 0)
    {
        DiagnosticState::Warning
    } else {
        DiagnosticState::Pass
    };
    Ok(RuntimeStorageReport {
        schema_version: 1,
        observed_at_unix_seconds: now,
        overall,
        database,
        storage_schema_version: inspected.version,
        supported_storage_schema_version: CURRENT_SCHEMA,
        schema_layout: inspected.layout,
        journal_integrity: inspected.journal,
        journal_authenticity: DiagnosticState::Unknown,
        owner_lock,
        counts: inspected.counts,
        leases,
        database_and_leases_atomic: false,
        process_liveness: DiagnosticState::Unknown,
        cleanup_authority: DiagnosticState::Unknown,
    })
}

fn inspect(db: &Connection, queries: &mut usize) -> Result<Inspection> {
    *queries += 1;
    let version: u32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_error)?;
    let mut result = Inspection {
        version,
        layout: RuntimeSchemaState::InvalidLayout,
        journal: DiagnosticState::Unknown,
        counts: None,
        leases: Vec::new(),
    };
    if version != CURRENT_SCHEMA {
        result.layout = if version < CURRENT_SCHEMA {
            RuntimeSchemaState::MigrationRequired
        } else {
            RuntimeSchemaState::FutureVersion
        };
        return Ok(result);
    }
    // This establishes the read layout, not the authenticity of the schema or
    // the complete runtime write invariants. Never run user-provided SQL/views.
    for (table, columns) in TABLES {
        *queries += 1;
        let is_table: bool = db
            .query_row(
                "SELECT count(*)=1 FROM sqlite_schema WHERE name=?1 AND type='table'",
                [table],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        if !is_table {
            return Ok(result);
        }
        *queries += 1;
        let mut query = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(sql_error)?;
        let names = query
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        if names.iter().map(String::as_str).collect::<Vec<_>>() != *columns {
            return Ok(result);
        }
        // Bound materialized rows and each cell before deserializing or hashing.
        let lengths = columns
            .iter()
            .map(|c| format!("coalesce(length(CAST({c} AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join(",");
        *queries += 1;
        let (count, longest): (u64, u64) = db
            .query_row(
                &format!("SELECT count(*),coalesce(max(max(0,{lengths})),0) FROM {table}"),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql_error)?;
        if count > MAX_ROWS || longest > 65536 {
            return Err(DoctorError::Limit);
        }
    }
    result.layout = RuntimeSchemaState::RecognizedReadLayout;
    // The existing verifier counts its statements, including early failures.
    result.journal = match store::verify_diagnostic_events(db, queries) {
        Ok(()) => DiagnosticState::Pass,
        Err(crate::RuntimeError::Storage(error)) => return Err(sql_error(error)),
        Err(crate::RuntimeError::Limit) => return Err(DoctorError::Limit),
        Err(_) => DiagnosticState::Fail,
    };
    *queries += 1;
    result.counts = Some(db.query_row("SELECT (SELECT count(*) FROM runs),(SELECT count(*) FROM events),(SELECT count(*) FROM writer_locks),(SELECT count(*) FROM cleanup_blocks),(SELECT count(*) FROM preparation_cycles),(SELECT count(*) FROM attempt_leases),(SELECT count(*) FROM attempt_leases WHERE resolved=1)", [], |r| Ok(RuntimeStorageCounts {
        runs:r.get(0)?, events:r.get(1)?, writer_reservations:r.get(2)?, cleanup_blocks:r.get(3)?,
        preparation_cycles:r.get(4)?, recorded_leases:r.get(5)?, recorded_resolved_leases:r.get(6)?,
    })).map_err(sql_error)?);
    *queries += 1;
    let mut query = db
        .prepare("SELECT run_id,attempt,record FROM attempt_leases ORDER BY run_id,attempt")
        .map_err(sql_error)?;
    result.leases = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(result)
}
fn sql_error(error: rusqlite::Error) -> DoctorError {
    match error {
        rusqlite::Error::SqliteFailure(error, _)
            if error.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            DoctorError::Limit
        }
        _ => DoctorError::Schema,
    }
}
const TABLES: &[(&str, &[&str])] = &[
    ("runs", &["run_id", "plan_id", "node_id", "summary"]),
    ("run_attempts", &["run_id", "attempt", "status"]),
    (
        "events",
        &[
            "sequence",
            "run_id",
            "kind",
            "state",
            "detail",
            "previous_hash",
            "event_hash",
        ],
    ),
    ("writer_locks", &["run_id", "workspace", "scope"]),
    ("artifacts", &["run_id", "sha256", "kind"]),
    ("verifications", &["run_id", "record"]),
    ("control_requests", &["run_id", "cancel_requested"]),
    (
        "output_artifacts",
        &["run_id", "attempt", "expires_at", "state", "record"],
    ),
    ("output_files", &["run_id", "attempt", "stream", "identity"]),
    (
        "workflow_instances",
        &["instance_id", "contract_hash", "record"],
    ),
    (
        "workflow_node_runs",
        &[
            "instance_id",
            "node_id",
            "generation",
            "run_id",
            "model",
            "agent",
        ],
    ),
    (
        "declared_artifacts",
        &[
            "artifact_id",
            "run_id",
            "attempt",
            "label",
            "record",
            "identity",
        ],
    ),
    (
        "attempt_leases",
        &["run_id", "attempt", "record", "resolved", "evidence"],
    ),
    ("cleanup_blocks", &["run_id"]),
    ("workflow_plan_authority", &["instance_id", "pin"]),
    ("preparation_cycles", &["run_id", "sequence", "record"]),
];
