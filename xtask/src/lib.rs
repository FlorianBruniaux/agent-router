#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod hook;
pub mod projection;
pub mod transaction;

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("harness.io: unable to {0}")]
    Io(&'static str, #[source] std::io::Error),
    #[error("harness.invalid: {0}")]
    Invalid(&'static str),
    #[error("harness.drift: {0}")]
    Drift(String),
}

pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

pub fn verify_build(root: &Path) -> Result<String> {
    let mut contents = Vec::new();
    for (path, built) in [
        ("xtask/Cargo.toml", include_str!("../Cargo.toml")),
        ("xtask/src/lib.rs", include_str!("lib.rs")),
        ("xtask/src/main.rs", include_str!("main.rs")),
        ("xtask/src/projection.rs", include_str!("projection.rs")),
        ("xtask/src/transaction.rs", include_str!("transaction.rs")),
        ("xtask/src/hook.rs", include_str!("hook.rs")),
    ] {
        if read(root, path)? != built {
            return Err(Error::Invalid("stale binary; rebuild xtask"));
        }
        contents.extend_from_slice(path.as_bytes());
        contents.push(0);
        contents.extend_from_slice(built.as_bytes());
    }
    Ok(digest(&contents))
}

// Never follow a source, destination or ancestor symlink. Callers serialize
// mutation; hostile concurrent filesystem replacement is not an OS sandbox.
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains('\\') {
        return Err(Error::Invalid("unsafe relative path"));
    }
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(part) = component else {
            return Err(Error::Invalid("unsafe relative path"));
        };
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::Invalid("symlink refused"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::Io("inspect path", e)),
        }
    }
    Ok(path)
}

pub fn read(root: &Path, relative: &str) -> Result<String> {
    let path = safe_path(root, relative)?;
    let meta = fs::metadata(&path).map_err(|e| Error::Io("inspect file", e))?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return Err(Error::Invalid("expected regular file below 1 MiB"));
    }
    let file = fs::File::open(path).map_err(|e| Error::Io("open file", e))?;
    let mut bytes = String::new();
    file.take(1024 * 1024 + 1)
        .read_to_string(&mut bytes)
        .map_err(|e| Error::Io("read UTF-8 file", e))?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("file exceeds 1 MiB"));
    }
    Ok(bytes)
}

pub fn json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|_| Error::Invalid("cannot serialize configuration"))
}
