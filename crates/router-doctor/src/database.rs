//! Inspect an owned SQLite connection, never open auxiliary file descriptors.
use crate::{DoctorError, MAX_FILE_BYTES, Result};
use router_types::DeepDoctorCode;

/// Inspect only the supplied connection. Callers must establish its ownership,
/// read-only source boundary or isolated in-memory snapshot before entering.
pub(crate) fn inspect_connection(
    connection: &rusqlite::Connection,
    queries: &mut usize,
) -> Result<DeepDoctorCode> {
    use rusqlite::{config::DbConfig, limits::Limit};
    use std::time::{Duration, Instant};
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, MAX_FILE_BYTES as i32),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 64 * 1024),
        (Limit::SQLITE_LIMIT_COLUMN, 256),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 64),
        (Limit::SQLITE_LIMIT_VDBE_OP, 1_000_000),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
    ] {
        connection
            .set_limit(limit, value)
            .map_err(|_| DoctorError::Unsupported)?;
    }
    for (configuration, enabled) in [
        (DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false),
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true),
    ] {
        connection
            .set_db_config(configuration, enabled)
            .map_err(|_| DoctorError::Unsupported)?;
    }
    let started = Instant::now();
    let mut ticks = 0usize;
    connection
        .progress_handler(
            1000,
            Some(move || {
                ticks += 1;
                ticks >= 1000 || started.elapsed() >= Duration::from_millis(250)
            }),
        )
        .map_err(|_| DoctorError::Unsupported)?;
    for (name, value) in [
        ("query_only", "ON"),
        ("temp_store", "MEMORY"),
        ("cache_size", "-1024"),
    ] {
        *queries += 1;
        connection
            .pragma_update(None, name, value)
            .map_err(|_| DoctorError::Read)?;
    }
    *queries += 1;
    let integrity: rusqlite::Result<String> =
        connection.query_row("PRAGMA integrity_check(1)", [], |row| row.get(0));
    match integrity {
        Ok(result) if result == "ok" => {}
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            return Err(DoctorError::Limit);
        }
        _ => return Ok(DeepDoctorCode::DatabaseIntegrityFailed),
    }
    *queries += 1;
    let foreign_keys = (|| -> rusqlite::Result<bool> {
        let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        Ok(rows.next()?.is_none())
    })();
    match foreign_keys {
        Ok(true) => Ok(DeepDoctorCode::DatabaseIntegrityValidated),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            Err(DoctorError::Limit)
        }
        _ => Ok(DeepDoctorCode::DatabaseIntegrityFailed),
    }
}
