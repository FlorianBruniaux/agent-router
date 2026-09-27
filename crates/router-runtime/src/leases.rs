//! A durable cleanup claim never follows from a PID or a process-group signal.
use crate::{Result, RuntimeError, digest, outputs::FileIdentity};
use nix::fcntl::{Flock, FlockArg};
use rustix::fs::{Mode, OFlags, mkdirat, openat};
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
