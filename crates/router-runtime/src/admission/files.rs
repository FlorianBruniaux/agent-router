use crate::{Result, RuntimeError, outputs::FileIdentity};
use nix::fcntl::{Flock, FlockArg};
use rustix::fs::{Mode, OFlags, openat};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub(super) struct Files {
    pub path: PathBuf,
    dir: File,
    identity: FileIdentity,
    db_identity: FileIdentity,
}
pub(super) struct Owner(Option<Flock<File>>);
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(lock) = self.0.take() {
            let _ = lock.unlock();
        }
    }
}
impl Files {
    pub fn open(root: &Path, create: bool) -> Result<Self> {
        if !root.is_absolute() {
            return Err(RuntimeError::Invalid("admission registry path"));
        }
        if create {
            std::fs::DirBuilder::new().mode(0o700).create(root)?;
        }
        let dir = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
            .open(root)?;
        if dir.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(RuntimeError::Invalid("admission private directory"));
        }
        let path = std::fs::canonicalize(root)?;
        let identity = FileIdentity::from_file(&dir)?;
        let file = private(&dir, "admission.sqlite3", create)?;
        let db_identity = FileIdentity::from_file(&file)?;
        // Pre-create auxiliaries privately; SQLite may remove them on last close.
        for name in ["admission.sqlite3-wal", "admission.sqlite3-shm"] {
            let _ = private(&dir, name, true)?;
        }
        Ok(Self {
            path,
            dir,
            identity,
            db_identity,
        })
    }
    pub fn binding_hash(&self) -> Result<String> {
        crate::digest(
            "runtime-admission-registry-v1",
            &(&self.path, &self.identity, &self.db_identity),
        )
    }
    pub fn check(&self) -> Result<()> {
        let dir = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
            .open(&self.path)?;
        if !self.identity.matches(&dir)?
            || dir.metadata()?.permissions().mode() & 0o077 != 0
            || !self
                .db_identity
                .matches(&private(&dir, "admission.sqlite3", false)?)?
        {
            return Err(RuntimeError::Integrity("admission registry replaced"));
        }
        for name in ["admission.sqlite3-wal", "admission.sqlite3-shm"] {
            if self.path.join(name).exists() {
                private(&dir, name, false)?;
            }
        }
        Ok(())
    }
    fn owner_file(&self, id: &str) -> Result<File> {
        private(
            &self.dir,
            &format!("{}.owner", crate::digest("admission-owner-v1", &id)?),
            true,
        )
    }
    pub fn owner(&self, id: &str) -> Result<Owner> {
        Ok(Owner(Some(
            Flock::lock(self.owner_file(id)?, FlockArg::LockExclusiveNonblock)
                .map_err(|_| RuntimeError::Locked)?,
        )))
    }
    pub fn owner_alive(&self, id: &str) -> Result<bool> {
        match Flock::lock(self.owner_file(id)?, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => {
                lock.unlock().map_err(|_| RuntimeError::Locked)?;
                Ok(false)
            }
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(true),
            Err(_) => Err(RuntimeError::Locked),
        }
    }
}
fn private(dir: &File, name: &str, create: bool) -> Result<File> {
    let flags = OFlags::RDWR
        | OFlags::NOFOLLOW
        | OFlags::NONBLOCK
        | OFlags::CLOEXEC
        | if create {
            OFlags::CREATE
        } else {
            OFlags::empty()
        };
    let file = File::from(
        openat(dir, name, flags, Mode::RUSR | Mode::WUSR).map_err(std::io::Error::from)?,
    );
    let m = file.metadata()?;
    if !m.is_file() || m.nlink() != 1 || m.permissions().mode() & 0o077 != 0 {
        return Err(RuntimeError::Invalid("admission private file"));
    }
    Ok(file)
}
