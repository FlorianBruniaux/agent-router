use crate::projection::{self, Files, INVENTORY};
use crate::{Error, Result, digest, json, read, safe_path};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const JOURNAL: &str = ".harness-state/rollback.json";
const LOCK: &str = ".harness-state/lock";

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn lock(root: &Path) -> Result<Lock> {
    let state = safe_path(root, ".harness-state")?;
    fs::create_dir_all(state).map_err(|e| Error::Io("create project state", e))?;
    let path = safe_path(root, LOCK)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| {
            Error::Io(
                "lock harness (another writer or interrupted transaction)",
                e,
            )
        })?;
    Ok(Lock(path))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    before: Option<String>,
    after: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    changes: BTreeMap<String, Change>,
}

fn existing(root: &Path, path: &str) -> Result<Option<String>> {
    let file = safe_path(root, path)?;
    if !file.exists() {
        return Ok(None);
    }
    read(root, path).map(Some)
}

fn write(root: &Path, relative: &str, content: Option<&str>) -> Result<()> {
    let path = safe_path(root, relative)?;
    if let Some(content) = content {
        let parent = path.parent().ok_or(Error::Invalid("missing parent"))?;
        fs::create_dir_all(parent).map_err(|e| Error::Io("create projection directory", e))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| Error::Io("create atomic temporary file", e))?;
        tmp.write_all(content.as_bytes())
            .map_err(|e| Error::Io("write temporary file", e))?;
        tmp.as_file()
            .sync_all()
            .map_err(|e| Error::Io("sync temporary file", e))?;
        tmp.persist(path)
            .map_err(|e| Error::Io("replace projection", e.error))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| Error::Io("remove owned projection", e))?;
    }
    Ok(())
}

pub fn apply(root: &Path, desired: Files) -> Result<usize> {
    let _lock = lock(root)?;
    let old = if safe_path(root, INVENTORY)?.exists() {
        projection::inventory(root)?.files
    } else {
        BTreeMap::new()
    };
    let paths: BTreeSet<_> = old
        .keys()
        .chain(desired.keys())
        .cloned()
        .chain(std::iter::once(INVENTORY.to_string()))
        .collect();
    projection::reject_stale(root, &paths)?;
    let mut changes = BTreeMap::new();
    for path in paths {
        if !projection::owned(&path) {
            return Err(Error::Invalid("unowned output"));
        }
        let before = existing(root, &path)?;
        let after = desired.get(&path).cloned();
        if let Some(hash) = old.get(&path) {
            if before.as_ref().map(|s| digest(s.as_bytes())).as_ref() != Some(hash) {
                return Err(Error::Drift(path));
            }
        } else if path != INVENTORY && before.is_some() && before != after {
            return Err(Error::Invalid("refusing to overwrite an unowned file"));
        }
        if before != after {
            changes.insert(path, Change { before, after });
        }
    }
    if changes.is_empty() {
        return Ok(0);
    }
    let journal = Journal {
        version: 1,
        changes,
    };
    let serialized = json(&journal)?;
    // The recovery reader accepts at most 1 MiB. Refuse before writing any
    // projection rather than successfully installing an unrecoverable bundle.
    if serialized.len() > 1024 * 1024 {
        return Err(Error::Invalid("transaction journal exceeds 1 MiB"));
    }
    write(root, JOURNAL, Some(&serialized))?;
    // Recheck the entire preflight immediately before the first mutation.
    for (path, change) in &journal.changes {
        if existing(root, path)? != change.before {
            return Err(Error::Invalid("concurrent edit"));
        }
    }
    for (path, change) in &journal.changes {
        write(root, path, change.after.as_deref())?;
    }
    if !desired.is_empty() {
        for (path, content) in &desired {
            if existing(root, path)?.as_ref() != Some(content) {
                return Err(Error::Invalid(
                    "postimage mismatch; inspect journal and roll back",
                ));
            }
        }
    }
    Ok(journal.changes.len())
}

pub fn rollback(root: &Path) -> Result<usize> {
    let _lock = lock(root)?;
    let journal: Journal = serde_json::from_str(&read(root, JOURNAL)?)
        .map_err(|_| Error::Invalid("rollback journal schema"))?;
    if journal.version != 1 || journal.changes.len() > 100 {
        return Err(Error::Invalid("rollback journal version or size"));
    }
    for (path, change) in &journal.changes {
        if !projection::owned(path) {
            return Err(Error::Invalid("rollback ownership"));
        }
        let now = existing(root, path)?;
        if now != change.after && now != change.before {
            return Err(Error::Invalid("concurrent edit prevents rollback"));
        }
    }
    for (path, change) in &journal.changes {
        write(root, path, change.before.as_deref())?;
    }
    for (path, change) in &journal.changes {
        if existing(root, path)? != change.before {
            return Err(Error::Invalid(
                "rollback verification failed; retain journal",
            ));
        }
    }
    fs::remove_file(safe_path(root, JOURNAL)?)
        .map_err(|e| Error::Io("consume rollback journal", e))?;
    Ok(journal.changes.len())
}
