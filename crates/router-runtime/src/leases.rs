//! A durable cleanup claim never follows from a PID or a process-group signal.
use crate::{Result, RuntimeError, digest, outputs::FileIdentity};
use nix::fcntl::{Flock, FlockArg};
use rustix::fs::{AtFlags, Mode, OFlags, mkdirat, openat, statat, unlinkat};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const PENDING: &[u8] = b"router-runtime-lease-v1:pending\n";
const GROUP_CLEAN: &[u8] = b"router-runtime-lease-v1:group-clean\n";
const NAMESPACE_CLEAN: &[u8] = b"router-runtime-lease-v1:namespace-clean\n";
const RECOVERED: &[u8] = b"router-runtime-lease-v1:recovered\n";

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct LeaseRecord {
    name: String,
    identity: FileIdentity,
}
pub(crate) struct LeaseStorage {
    directory: File,
    path: PathBuf,
}
pub(crate) struct CleanupProof {
    _lock: Flock<File>,
    pub evidence: router_types::ProcessCleanupEvidence,
}
impl LeaseStorage {
    pub fn open(root: &Path) -> Result<Self> {
        let root_file = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
            .open(root)?;
        match mkdirat(&root_file, "leases", Mode::RWXU) {
            Ok(()) => root_file.sync_all()?,
            Err(rustix::io::Errno::EXIST) => {}
            Err(e) => return Err(std::io::Error::from(e).into()),
        }
        let directory = File::from(
            openat(
                &root_file,
                "leases",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?,
        );
        if directory.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(RuntimeError::Integrity("lease directory"));
        }
        Ok(Self {
            directory,
            path: root.join("leases"),
        })
    }
    fn name(id: &str, attempt: u32) -> Result<String> {
        Ok(format!(
            "{}.lease",
            digest("runtime-lease-v1", &(id, attempt))?
        ))
    }
    pub fn create(&self, id: &str, attempt: u32) -> Result<(LeaseRecord, PathBuf)> {
        let name = Self::name(id, attempt)?;
        let mut file = File::from(
            openat(
                &self.directory,
                &name,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(std::io::Error::from)?,
        );
        file.write_all(PENDING)?;
        file.sync_all()?;
        self.directory.sync_all()?;
        Ok((
            LeaseRecord {
                name: name.clone(),
                identity: FileIdentity::from_file(&file)?,
            },
            self.path.join(name),
        ))
    }
    /// Removes the lease file of `(id, attempt)` only when it is provably a
    /// lease created and never committed: a regular, single-link, private file
    /// of this user, holding exactly the pending marker, with no other holder.
    /// The caller must hold the journal write transaction and have observed no
    /// `attempt_leases` row, so no helper can have been handed this file.
    /// Anything else, including a lookup failure, is left untouched.
    pub fn reclaim_uncommitted(&self, id: &str, attempt: u32) -> Result<bool> {
        let name = Self::name(id, attempt)?;
        let Ok(fd) = openat(
            &self.directory,
            &name,
            OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) else {
            return Ok(false);
        };
        let file = File::from(fd);
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.mode() & 0o077 != 0
            || meta.uid() != rustix::process::getuid().as_raw()
            || meta.len() != PENDING.len() as u64
        {
            return Ok(false);
        }
        let Ok(mut lock) = Flock::lock(file, FlockArg::LockExclusiveNonblock) else {
            return Ok(false);
        };
        let mut bytes = Vec::new();
        (&mut *lock).take(96).read_to_end(&mut bytes)?;
        if bytes != PENDING {
            return Ok(false);
        }
        // The name must still designate the file we hold and locked.
        let current = statat(&self.directory, &name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(std::io::Error::from)?;
        let same = FileIdentity {
            device: current.st_dev as u64,
            inode: current.st_ino as u64,
        }
        .matches(&lock)?;
        if !same {
            return Ok(false);
        }
        unlinkat(&self.directory, &name, AtFlags::empty()).map_err(std::io::Error::from)?;
        self.directory.sync_all()?;
        Ok(true)
    }
    /// The caller retains this lock through the durable terminal transition.
    /// Invalidating pending under the lock prevents a delayed helper spawning.
    pub fn cleanup_proof(
        &self,
        id: &str,
        attempt: u32,
        record: &LeaseRecord,
    ) -> Result<CleanupProof> {
        if record.name != Self::name(id, attempt)? {
            return Err(RuntimeError::CleanupUnknown);
        }
        let file = File::from(
            openat(
                &self.directory,
                &record.name,
                OFlags::RDWR | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| RuntimeError::CleanupUnknown)?,
        );
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o077 != 0
            || !record.identity.matches(&file)?
        {
            return Err(RuntimeError::CleanupUnknown);
        }
        let mut lock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|_| RuntimeError::CleanupUnknown)?;
        let mut status = Vec::new();
        (&mut *lock).take(96).read_to_end(&mut status)?;
        let evidence = if status == PENDING {
            lock.seek(SeekFrom::Start(0))?;
            lock.write_all(RECOVERED)?;
            lock.set_len(RECOVERED.len() as u64)?;
            lock.sync_all()?;
            router_types::ProcessCleanupEvidence::NotStarted
        } else if status == RECOVERED {
            router_types::ProcessCleanupEvidence::NotStarted
        } else if status == GROUP_CLEAN {
            router_types::ProcessCleanupEvidence::GroupTerminated
        } else if status == NAMESPACE_CLEAN {
            router_types::ProcessCleanupEvidence::NamespaceTerminated
        } else {
            return Err(RuntimeError::CleanupUnknown);
        };
        Ok(CleanupProof {
            _lock: lock,
            evidence,
        })
    }
}

/// Observation only: even a cleanup marker is not an opaque cleanup proof.
pub(crate) enum LeaseObservation {
    Held,
    Pending,
    CleanupMarked,
    Missing,
    Ambiguous,
}
pub(crate) fn observe_lease(
    root: &File,
    id: &str,
    attempt: u32,
    record: &LeaseRecord,
) -> LeaseObservation {
    observe_lease_checked(root, id, attempt, record).unwrap_or(LeaseObservation::Ambiguous)
}
fn observe_lease_checked(
    root: &File,
    id: &str,
    attempt: u32,
    record: &LeaseRecord,
) -> Result<LeaseObservation> {
    if attempt == 0 || attempt > 5 || record.name != LeaseStorage::name(id, attempt)? {
        return Ok(LeaseObservation::Ambiguous);
    }
    let open = |directory: &File, name: &str, flags: OFlags| -> std::io::Result<File> {
        openat(
            directory,
            name,
            flags | OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(std::io::Error::from)
    };
    let directory = match open(root, "leases", OFlags::DIRECTORY) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LeaseObservation::Missing),
        Err(e) => return Err(e.into()),
    };
    let meta = directory.metadata()?;
    if meta.permissions().mode() & 0o077 != 0 || meta.uid() != rustix::process::getuid().as_raw() {
        return Ok(LeaseObservation::Ambiguous);
    }
    let file = match open(&directory, &record.name, OFlags::empty()) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LeaseObservation::Missing),
        Err(e) => return Err(e.into()),
    };
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.len() > 96
        || meta.mode() & 0o077 != 0
        || meta.uid() != rustix::process::getuid().as_raw()
        || !record.identity.matches(&file)?
    {
        return Ok(LeaseObservation::Ambiguous);
    }
    let mut lock = match Flock::lock(file, FlockArg::LockSharedNonblock) {
        Ok(lock) => lock,
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => return Ok(LeaseObservation::Held),
        Err(_) => return Ok(LeaseObservation::Ambiguous),
    };
    let mut bytes = Vec::new();
    (&mut *lock).take(97).read_to_end(&mut bytes)?;
    Ok(if bytes == PENDING {
        LeaseObservation::Pending
    } else if bytes == GROUP_CLEAN || bytes == NAMESPACE_CLEAN || bytes == RECOVERED {
        LeaseObservation::CleanupMarked
    } else {
        LeaseObservation::Ambiguous
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // router-sandbox-helper cannot depend on this crate, and this crate cannot
    // link the helper binary, so the two spellings are pinned against each
    // other by reading the helper source.
    const HELPER: &str = include_str!("../../router-sandbox-helper/src/watchdog.rs");

    fn literal(marker: &[u8]) -> String {
        format!(
            "b\"{}\"",
            std::str::from_utf8(marker).unwrap().replace('\n', "\\n")
        )
    }
    #[test]
    fn helper_lease_markers_equal_the_runtime_markers() {
        for (name, marker) in [
            ("PENDING", PENDING),
            ("GROUP_CLEAN", GROUP_CLEAN),
            ("NAMESPACE_CLEAN", NAMESPACE_CLEAN),
        ] {
            assert!(
                HELPER.contains(&literal(marker)),
                "helper watchdog no longer spells {name} as {}",
                literal(marker)
            );
        }
        // The active marker exists only in the helper, but must stay in the
        // same versioned family as the markers the runtime reads.
        assert!(HELPER.contains(&literal(b"router-runtime-lease-v1:active\n")));
    }
}
