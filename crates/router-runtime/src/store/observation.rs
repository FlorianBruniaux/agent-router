//! One passive read transaction. Preflight all inputs before JSON/string decoding.
use super::*;
use crate::observation::*;
use router_types::ProcessCleanupEvidence;

const MAX_EVENTS: u64 = 100_000;
const MAX_RUN_EVENTS: u64 = 4096;
const MAX_ROWS: u64 = 10_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ROW_BYTES: u64 = 65_536;

/// Observe an existing run without acquiring ownership, recovering, migrating,
/// checkpointing, changing state or releasing permits. SQLite may use/create
/// WAL/SHM coordination files. This is not a claim of zero filesystem activity.
/// Corrupt, inconsistent or oversized inputs are refused, never truncated into
/// a seemingly complete snapshot. Private identifiers and details are omitted.
pub fn observe_run(root: &Path, id: &str) -> Result<RunObservation> {
    if id.is_empty() || id.len() > 1024 {
        return Err(RuntimeError::Invalid("observation run id"));
    }
    let mut db = control_connection(root, false)?;
    let tx = db.transaction()?;
    let report = project(&tx, id).map_err(|error| match error {
        // serde errors can name an unknown private key even in Debug output.
        RuntimeError::Json(_) => RuntimeError::Integrity("observation malformed data"),
        other => other,
    })?;
    tx.commit()?;
    Ok(report)
}

fn project(db: &Connection, id: &str) -> Result<RunObservation> {
    // Validate the schema in this snapshot too, not just before BEGIN.
    let schema: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(2..=SCHEMA_VERSION).contains(&schema) {
        return Err(RuntimeError::Integrity("observation schema"));
    }
    // No deserialization precedes admission. Every field read by full-chain
    // authority verification is byte-accounted.
    admit(
        db,
        "SELECT length(CAST(run_id AS BLOB))+length(CAST(kind AS BLOB))+length(CAST(state AS BLOB))+length(CAST(detail AS BLOB))+length(CAST(previous_hash AS BLOB))+length(CAST(event_hash AS BLOB)) AS bytes FROM events",
        &[],
        MAX_EVENTS,
    )?;
    admit(
        db,
        "SELECT length(CAST(summary AS BLOB)) AS bytes FROM runs WHERE run_id=?1",
        &[&id],
        1,
    )?;
    admit(
        db,
        "SELECT 0 AS bytes FROM events WHERE run_id=?1",
        &[&id],
        MAX_RUN_EVENTS,
    )?;
    admit(
        db,
        "SELECT 0 AS bytes FROM run_attempts WHERE run_id=?1",
        &[&id],
        5,
    )?;
    if schema >= 6 {
        admit(
            db,
            "SELECT length(CAST(record AS BLOB))+COALESCE(length(CAST(evidence AS BLOB)),0) AS bytes FROM attempt_leases WHERE run_id=?1",
            &[&id],
            5,
        )?;
    }
    if schema >= 7 {
        admit(
            db,
            "SELECT length(CAST(instance_id AS BLOB))+length(CAST(contract_hash AS BLOB)) AS bytes FROM workflow_instances",
            &[],
            MAX_ROWS,
        )?;
        admit(
            db,
            "SELECT length(CAST(instance_id AS BLOB))+length(CAST(pin AS BLOB)) AS bytes FROM workflow_plan_authority",
            &[],
            MAX_ROWS,
        )?;
    }
    if schema >= 8 {
        admit(
            db,
            "SELECT length(CAST(record AS BLOB)) AS bytes FROM preparation_cycles WHERE run_id=?1",
            &[&id],
            1024,
        )?;
    }
    if schema >= 9 {
        admit(
            db,
            "SELECT length(CAST(record AS BLOB)) AS bytes FROM admission_intents WHERE run_id=?1",
            &[&id],
            1,
        )?;
    }
    verify_events(db)?;
    let run = load_opt(db, id)?.ok_or(RuntimeError::NotFound)?;
    if run.run_id != id
        || !router_adapter_api::is_digest(&run.definition_hash)
        || !router_adapter_api::is_digest(&run.approval_digest)
    {
        return Err(RuntimeError::Integrity("observation run binding"));
    }
    let mut query = db.prepare(&format!(
        "{EVENT_COLUMNS} WHERE run_id=?1 ORDER BY sequence"
    ))?;
    let mut rows = query.query([id])?;
    let mut references = Vec::new();
    let mut starts = 0u32;
    while let Some(row) = rows.next()? {
        let event = event_from_row(row)?;
        starts += u32::from(event.kind == "attempt.started");
        references.push(RunEventReference {
            sequence: event.sequence,
            state: event.state,
            event_sha256: event.event_hash,
        });
    }
    let mut ordinals =
        db.prepare("SELECT attempt FROM run_attempts WHERE run_id=?1 ORDER BY attempt")?;
    let attempts = ordinals
        .query_map([id], |r| r.get::<_, u32>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if references.last().map(|e| e.state) != Some(run.state)
        || run.attempts as usize != attempts.len()
        || attempts.iter().copied().ne(1..=run.attempts)
        || starts != run.attempts
    {
        return Err(RuntimeError::Integrity("observation run history"));
    }
    let (sequence, event_sha256) = db
        .query_row(
            "SELECT sequence,event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, Some(r.get(1)?))),
        )
        .optional()?
        .unwrap_or((0, None));
    let cleanup = if schema < 6
        || db
            .prepare("SELECT 1 FROM attempt_leases WHERE run_id=?1 AND resolved=0 LIMIT 1")?
            .exists([id])?
    {
        ProcessCleanupEvidence::Unknown
    } else {
        match cleanup_evidence(db, id) {
            Ok(evidence) => evidence,
            Err(RuntimeError::CleanupUnknown) => ProcessCleanupEvidence::Unknown,
            Err(error) => return Err(error),
        }
    };
    let unknown = ObservationUnknown::Unknown;
    let mut report = RunObservation {
        schema_version: 1,
        snapshot_sha256: String::new(),
        identity: RunObservationIdentity {
            run_key_sha256: router_adapter_api::content_digest(&serde_json::to_vec(&(
                &run.run_id,
                &run.definition_hash,
                &run.approval_digest,
            ))?),
            definition_sha256: run.definition_hash.clone(),
            receipt_id_sha256: digest("run-observation-receipt-v1", &run.receipt_id)?,
            plan_id_sha256: digest("run-observation-plan-v1", &run.plan_id)?,
            node_id_sha256: digest("run-observation-node-v1", &run.node_id)?,
        },
        horizon: RunObservationHorizon {
            sequence,
            event_sha256,
        },
        known: RunObservationKnown {
            state: run.state,
            attempts: run.attempts,
            failure: run.failure,
            cleanup,
            events: references,
        },
        unknown: RunObservationUnknowns {
            requested_model: unknown,
            requested_effort: unknown,
            observed_model: unknown,
            observed_effort: unknown,
            transport: unknown,
            validation: unknown,
            objective: unknown,
            duration: unknown,
            journal_authenticity: unknown,
            process_liveness: unknown,
        },
        completeness: RunObservationCompleteness {
            journal: ObservationJournalCoverage::Complete,
            external_observations: unknown,
            detail: ObservationDetailCoverage::Redacted,
        },
    };
    // Identity of the redacted projection with an empty self-digest. No wall clock
    // or filesystem path participates, so a repeat read is byte deterministic.
    report.snapshot_sha256 = digest("run-observation-v1", &report)?;
    Ok(report)
}

/// Only numeric lengths/counts cross this boundary. SQLite scans at most the
/// row ceiling plus a sentinel; no untrusted text is allocated or decoded here.
fn admit(
    db: &Connection,
    select: &str,
    params: &[&dyn rusqlite::ToSql],
    max_rows: u64,
) -> Result<()> {
    let sql = format!(
        "SELECT COUNT(*),COALESCE(SUM(bytes),0),COALESCE(MAX(bytes),0) FROM ({select} LIMIT {})",
        max_rows + 1
    );
    let (count, bytes, largest): (u64, u64, u64) =
        db.query_row(&sql, params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    if count > max_rows || bytes > MAX_BYTES || largest > MAX_ROW_BYTES {
        return Err(RuntimeError::Limit);
    }
    Ok(())
}
