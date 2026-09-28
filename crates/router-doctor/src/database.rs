//! Inspect an already-open descriptor, never reopen an untrusted pathname.
use crate::{DoctorError, MAX_FILE_BYTES, Result};
use router_types::DeepDoctorCode;
use std::fs::File;

pub(crate) fn inspect(file: &File, bytes: &[u8], queries: &mut usize) -> Result<DeepDoctorCode> {
    if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
        return Ok(DeepDoctorCode::DatabaseIntegrityFailed);
    }
    // WAL-mode databases require a consistent checkpoint/export. The immutable
    // reader must not silently omit live WAL content or create shared memory.
    if bytes[18] == 2 || bytes[19] == 2 {
        return Ok(DeepDoctorCode::DatabaseSnapshotRequired);
    }
    inspect_descriptor(file, queries)
}

#[cfg(unix)]
fn inspect_descriptor(file: &File, queries: &mut usize) -> Result<DeepDoctorCode> {
    use rusqlite::{Connection, OpenFlags};
    use std::os::fd::AsRawFd;

    let path = format!("/dev/fd/{}", file.as_raw_fd());
    // macOS fdescfs exposes a regular descriptor directly. Linux commonly uses
    // symlinks here, which SQLite resolves back to a mutable pathname. Refuse
    // that fallback rather than weaken the descriptor traversal guarantee.
    for component in ["/dev", "/dev/fd", path.as_str()] {
        let Ok(metadata) = std::fs::symlink_metadata(component) else {
            return Ok(DeepDoctorCode::DatabaseReadOnlyUnsupported);
        };
        if metadata.file_type().is_symlink() {
            return Ok(DeepDoctorCode::DatabaseReadOnlyUnsupported);
        }
    }
    let connection = Connection::open_with_flags(
        format!("file:{path}?mode=ro&immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| DoctorError::Read)?;
    if !connection
        .is_readonly("main")
        .map_err(|_| DoctorError::Read)?
    {
        return Err(DoctorError::Unsupported);
    }
    inspect_connection(&connection, queries)
}

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

#[cfg(not(unix))]
fn inspect_descriptor(_: &File, _: &mut usize) -> Result<DeepDoctorCode> {
    Ok(DeepDoctorCode::DatabaseReadOnlyUnsupported)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn sqlite_uses_pinned_descriptor_after_original_path_is_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("original.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE proof(id INTEGER PRIMARY KEY);")
            .unwrap();
        drop(connection);
        let file = File::open(&path).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, temp.path().join("retained.db")).unwrap();
        fs::write(temp.path().join("other.db"), b"invalid unrelated database").unwrap();
        symlink(temp.path().join("other.db"), &path).unwrap();
        let mut queries = 0;
        assert_eq!(
            inspect(&file, &bytes, &mut queries).unwrap(),
            DeepDoctorCode::DatabaseIntegrityValidated
        );
        assert_eq!(queries, 5);
        assert_eq!(
            fs::read(temp.path().join("other.db")).unwrap(),
            b"invalid unrelated database"
        );
        assert_eq!(fs::read(temp.path().join("retained.db")).unwrap(), bytes);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
    }
}
