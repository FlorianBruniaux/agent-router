// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! New owner-private output directories for explicit exports. Unix only;
//! callers report their own unsupported code elsewhere.
use crate::CliError;
use std::path::{Path, PathBuf};

/// Each exported document is limited to this size, newline included.
pub(crate) const MAX_DOCUMENT: usize = 1024 * 1024;

/// The directory `publish` would create for `path`: its canonical parent
/// joined with its final component. Nothing is created.
pub(crate) fn destination(path: &Path, code: &'static str) -> Result<PathBuf, CliError> {
    let fail = || CliError::Product(code);
    let filename = path.file_name().ok_or_else(fail)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|_| fail())?;
    Ok(parent.join(filename))
}

/// All documents are computed before mkdir. Each complete file is published by
/// an atomic no-clobber hard link, in the given order, so the last document is
/// the completion marker. Interrupted publication may leave a private partial
/// directory; never replace or recursively remove it, since it may now contain
/// someone else's files. Every failure reports the caller's stable `code`.
pub(crate) fn publish(
    path: &Path,
    documents: Vec<(&'static str, Vec<u8>)>,
    code: &'static str,
) -> Result<PathBuf, CliError> {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    };
    let fail = || CliError::Product(code);
    if documents
        .iter()
        .any(|(_, bytes)| bytes.len() + 1 > MAX_DOCUMENT)
    {
        return Err(fail());
    }
    let destination = destination(path, code)?;
    let parent = destination.parent().ok_or_else(fail)?.to_owned();
    let parent_metadata = fs::metadata(&parent).map_err(|_| fail())?;
    if !parent_metadata.is_dir() || parent_metadata.mode() & 0o022 != 0 {
        return Err(fail());
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&destination)
        .map_err(|_| fail())?;
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(&destination)
        .map_err(|_| fail())?;
    let identity = directory.metadata().map_err(|_| fail())?;
    let check = || -> Result<(), CliError> {
        let live = fs::symlink_metadata(&destination).map_err(|_| fail())?;
        let current_parent = fs::metadata(&parent).map_err(|_| fail())?;
        if !live.is_dir()
            || live.dev() != identity.dev()
            || live.ino() != identity.ino()
            || live.mode() & 0o077 != 0
            || live.uid() != identity.uid()
            || current_parent.dev() != parent_metadata.dev()
            || current_parent.ino() != parent_metadata.ino()
            || current_parent.mode() & 0o022 != 0
        {
            return Err(fail());
        }
        Ok(())
    };
    for (name, mut bytes) in documents {
        check()?;
        bytes.push(b'\n');
        let temporary = destination.join(format!(".{name}.pending"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|_| fail())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| fail())?;
        check()?;
        fs::hard_link(&temporary, destination.join(name)).map_err(|_| fail())?;
        fs::remove_file(&temporary).map_err(|_| fail())?;
    }
    directory.sync_all().map_err(|_| fail())?;
    check()?;
    fs::File::open(&parent)
        .and_then(|file| file.sync_all())
        .map_err(|_| fail())?;
    Ok(destination)
}
