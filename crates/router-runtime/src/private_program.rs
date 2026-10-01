//! Per-attempt executable copy. The child may read and execute it, never write it.
use crate::{Result, RuntimeError, sandbox::binary_hash};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions, Permissions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MAX_PROGRAM_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) struct PrivateProgram {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl PrivateProgram {
    pub(crate) fn copy(source: &Path, expected_hash: &str, store: &Path) -> Result<Self> {
        let mut source_file = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(source)?;
        let source_meta = source_file.metadata()?;
        if !source_meta.is_file() {
            return Err(RuntimeError::Sandbox);
        }
        if source_meta.len() > MAX_PROGRAM_BYTES {
            return Err(RuntimeError::Limit);
        }
        let temp_root = fs::canonicalize(std::env::temp_dir())?;
        let directory = tempfile::Builder::new()
            .prefix("agent-router-program-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir_in(temp_root)?;
        let dir_path = directory.path();
        let dir_meta = fs::symlink_metadata(dir_path)?;
        if fs::canonicalize(dir_path)? != dir_path
            || dir_path.starts_with(store)
            || !dir_meta.is_dir()
            || dir_meta.uid() != rustix::process::geteuid().as_raw()
            || dir_meta.permissions().mode() & 0o777 != 0o700
        {
            return Err(RuntimeError::Sandbox);
        }
        let path = dir_path.join("program");
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(&path)?;
        let mut hash = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0; 16_384];
        loop {
            let count = source_file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count as u64).ok_or(RuntimeError::Limit)?;
            if total > MAX_PROGRAM_BYTES {
                return Err(RuntimeError::Limit);
            }
            hash.update(&buffer[..count]);
            destination.write_all(&buffer[..count])?;
        }
        if hex::encode(hash.finalize()) != expected_hash {
            return Err(RuntimeError::Integrity(
                "approved binary changed before copy",
            ));
        }
        destination.set_permissions(Permissions::from_mode(0o500))?;
        destination.sync_all()?;
        drop(destination);
        let copy = Self { directory, path };
        copy.verify(expected_hash)?;
        Ok(copy)
    }

    pub(crate) fn verify(&self, expected_hash: &str) -> Result<&Path> {
        let dir_path = self.directory.path();
        let dir_meta = fs::symlink_metadata(dir_path)?;
        let file_meta = fs::symlink_metadata(&self.path)?;
        if fs::canonicalize(dir_path)? != dir_path
            || !dir_meta.is_dir()
            || dir_meta.uid() != rustix::process::geteuid().as_raw()
            || dir_meta.permissions().mode() & 0o777 != 0o700
            || !file_meta.is_file()
            || file_meta.uid() != rustix::process::geteuid().as_raw()
            || file_meta.nlink() != 1
            || file_meta.permissions().mode() & 0o777 != 0o500
            || binary_hash(&self.path)? != expected_hash
        {
            return Err(RuntimeError::Integrity("private program changed"));
        }
        Ok(&self.path)
    }

    pub(crate) fn directory(&self) -> &Path {
        self.directory.path()
    }

    pub(crate) fn cleanup(self) -> Result<()> {
        self.directory.close().map_err(Into::into)
    }
}
