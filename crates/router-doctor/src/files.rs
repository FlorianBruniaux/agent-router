//! Descriptor-relative O_NOFOLLOW traversal prevents concurrent parent symlink
//! replacement from redirecting reads. No directory is created or modified.

use crate::{DoctorError, Result};
use std::{
    fs::File,
    path::{Component, Path},
};

pub(crate) fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && !path.contains(['\0', '\n', '\r', '\\'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

#[cfg(unix)]
pub(crate) struct ReadRoot(File);
#[cfg(not(unix))]
pub(crate) struct ReadRoot;

#[cfg(unix)]
impl ReadRoot {
    pub(crate) fn open(path: &str) -> Result<Self> {
        use std::os::fd::AsRawFd;
        if !Path::new(path).is_absolute() || path.len() > 4096 || path.contains('\0') {
            return Err(DoctorError::Path);
        }
        let mut directory = File::open("/").map_err(|_| DoctorError::Read)?;
        for part in Path::new(path).components() {
            match part {
                Component::RootDir => {}
                Component::Normal(name) => directory = open_at(directory.as_raw_fd(), name, true)?,
                _ => return Err(DoctorError::Path),
            }
        }
        Ok(Self(directory))
    }

    pub(crate) fn file(&self, path: &str) -> Result<File> {
        use std::os::fd::AsRawFd;
        if !relative(path) {
            return Err(DoctorError::Path);
        }
        let mut directory = self.0.try_clone().map_err(|_| DoctorError::Read)?;
        let parts: Vec<_> = Path::new(path).components().collect();
        for (index, part) in parts.iter().enumerate() {
            let Component::Normal(name) = part else {
                return Err(DoctorError::Path);
            };
            let last = index + 1 == parts.len();
            let file = open_at(directory.as_raw_fd(), name, !last)?;
            if last {
                if !file.metadata().map_err(|_| DoctorError::Read)?.is_file() {
                    return Err(DoctorError::Path);
                }
                return Ok(file);
            }
            directory = file;
        }
        Err(DoctorError::Path)
    }
}

#[cfg(unix)]
fn open_at(
    directory: std::os::fd::RawFd,
    name: &std::ffi::OsStr,
    is_directory: bool,
) -> Result<File> {
    use std::{
        ffi::CString,
        os::{fd::FromRawFd, unix::ffi::OsStrExt},
    };
    let name = CString::new(name.as_bytes()).map_err(|_| DoctorError::Path)?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if is_directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: name is NUL-terminated, directory remains owned and open, flags
    // request read-only access and no creation. A successful fd is owned once.
    let descriptor = unsafe { libc::openat(directory, name.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(DoctorError::Read);
    }
    // SAFETY: openat just returned a fresh valid descriptor, transferred to File.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(not(unix))]
impl ReadRoot {
    pub(crate) fn open(_: &str) -> Result<Self> {
        Err(DoctorError::Unsupported)
    }
    pub(crate) fn file(&self, _: &str) -> Result<File> {
        Err(DoctorError::Unsupported)
    }
}
