use crate::{Cancellation, Result, RuntimeError, digest, process::capture};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};
use tokio::process::Command;

pub(crate) trait CaptureObserver: Send + Sync {
    fn before_capture(&self) -> Result<()>;
    fn after_capture(&self) -> Result<()>;
}

pub(crate) async fn git(root: &Path, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>> {
    git_observed(root, args, input, None).await
}
pub(crate) async fn git_observed(
    root: &Path,
    args: &[&str],
    input: Vec<u8>,
    observer: Option<&dyn CaptureObserver>,
) -> Result<Vec<u8>> {
    let names = git_raw(
        root,
        &[
            "config",
            "--local",
            "--includes",
            "--name-only",
            "--null",
            "--list",
        ],
        vec![],
        observer,
    )
    .await?;
    if names
        .split(|b| *b == 0)
        .any(|name| name.to_ascii_lowercase().starts_with(b"filter."))
    {
        return Err(RuntimeError::Invalid(
            "repository filters are not permitted",
        ));
    }
    git_raw(root, args, input, observer).await
}
async fn git_raw(
    root: &Path,
    args: &[&str],
    input: Vec<u8>,
    observer: Option<&dyn CaptureObserver>,
) -> Result<Vec<u8>> {
    let mut command = Command::new("/usr/bin/git");
    command
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = capture_observed(command, input, observer).await?;
    if output.outcome.exit_code != Some(0) || output.output_limited {
        return Err(RuntimeError::Process);
    }
    Ok(output.stdout)
}
async fn capture_observed(
    command: Command,
    input: Vec<u8>,
    observer: Option<&dyn CaptureObserver>,
) -> Result<crate::process::Capture> {
    if let Some(observer) = observer {
        observer.before_capture()?;
    }
    let output = capture(
        command,
        input,
        15_000,
        16 * 1024 * 1024,
        0,
        &Cancellation::default(),
    )
    .await?;
    if let Some(observer) = observer {
        observer.after_capture()?;
    }
    Ok(output)
}
pub async fn source_state(root: &Path) -> Result<String> {
    source_state_observed(root, None).await
}
pub(crate) async fn source_state_observed(
    root: &Path,
    observer: Option<&dyn CaptureObserver>,
) -> Result<String> {
    let canonical = fs::canonicalize(root)?;
    if canonical != root || root == Path::new("/") {
        return Err(RuntimeError::Invalid("canonical project path"));
    }
    let top = git_observed(root, &["rev-parse", "--show-toplevel"], vec![], observer).await?;
    if top.strip_suffix(b"\n") != Some(root.as_os_str().as_encoded_bytes()) {
        return Err(RuntimeError::Invalid("repository root"));
    }
    let status = git_observed(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        vec![],
        observer,
    )
    .await?;
    if !status.is_empty() {
        return Err(RuntimeError::SourceChanged);
    }
    let head = git_observed(root, &["rev-parse", "--verify", "HEAD"], vec![], observer).await?;
    let head = String::from_utf8(head).map_err(|_| RuntimeError::Invalid("git head"))?;
    let head = head.trim();
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(RuntimeError::Invalid("git head"));
    }
    Ok(head.into())
}
pub(crate) fn safe_scope(root: &Path, scope: &[String]) -> Result<()> {
    for relative in scope {
        if relative.is_empty()
            || relative.len() > 1024
            || relative.contains(['\0', '\n', '\r', '\\'])
            || !Path::new(relative)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            || relative.split('/').any(|s| s == ".git")
        {
            return Err(RuntimeError::Scope);
        }
        let mut cursor = root.to_path_buf();
        for part in Path::new(relative).components() {
            cursor.push(part);
            match fs::symlink_metadata(&cursor) {
                Ok(m) if m.file_type().is_symlink() => return Err(RuntimeError::Scope),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}
pub(crate) async fn create(
    root: &Path,
    storage: &Path,
    run_id: &str,
    head: &str,
    observer: Option<&dyn CaptureObserver>,
) -> Result<PathBuf> {
    let parent = storage.join("worktrees");
    if !parent.exists() {
        fs::DirBuilder::new().mode(0o700).create(&parent)?;
    }
    let path = parent.join(run_id);
    if path.exists() {
        return Err(RuntimeError::Integrity("worktree exists"));
    }
    let path_text = path
        .to_str()
        .ok_or(RuntimeError::Invalid("utf8 worktree"))?;
    git_observed(
        root,
        &["worktree", "add", "--detach", path_text, head],
        vec![],
        observer,
    )
    .await?;
    Ok(path)
}
pub(crate) async fn patch(
    worktree: &Path,
    scope: &[String],
    base: &str,
) -> Result<(Vec<u8>, Vec<String>)> {
    patch_observed(worktree, scope, base, None).await
}
pub(crate) async fn patch_observed(
    worktree: &Path,
    scope: &[String],
    base: &str,
    observer: Option<&dyn CaptureObserver>,
) -> Result<(Vec<u8>, Vec<String>)> {
    safe_scope(worktree, scope)?;
    // The private worktree index is isolated from the user's repository index.
    // Intent-to-add permits binary/untracked additions in a reviewable patch.
    git_observed(
        worktree,
        &["add", "--intent-to-add", "--all", "--", "."],
        vec![],
        observer,
    )
    .await?;
    let changed = git_observed(
        worktree,
        &[
            "diff",
            "--name-only",
            "--no-ext-diff",
            "--no-textconv",
            "-z",
            base,
            "--",
        ],
        vec![],
        observer,
    )
    .await?;
    let mut paths = BTreeSet::new();
    for path in changed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = std::str::from_utf8(path).map_err(|_| RuntimeError::Scope)?;
        if !scope.iter().any(|s| {
            path == s
                || path
                    .strip_prefix(s)
                    .is_some_and(|tail| tail.starts_with('/'))
        }) {
            return Err(RuntimeError::Scope);
        }
        safe_scope(worktree, &[path.into()])?;
        paths.insert(path.to_owned());
    }
    let patch = git_observed(
        worktree,
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            base,
            "--",
        ],
        vec![],
        observer,
    )
    .await?;
    Ok((patch, paths.into_iter().collect()))
}
pub(crate) fn write_artifact(storage: &Path, run_id: &str, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(RuntimeError::Limit);
    }
    let parent = storage.join("artifacts");
    if !parent.exists() {
        fs::DirBuilder::new().mode(0o700).create(&parent)?;
    }
    let path = parent.join(format!("{run_id}.patch"));
    let temp = parent.join(format!("{run_id}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, &path)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(path)
}
pub(crate) fn read_artifact(storage: &Path, run_id: &str, expected_hash: &str) -> Result<Vec<u8>> {
    let parent = storage.join("artifacts");
    let meta = fs::symlink_metadata(&parent)?;
    if !meta.is_dir() || meta.file_type().is_symlink() || meta.permissions().mode() & 0o077 != 0 {
        return Err(RuntimeError::Integrity("artifact directory"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(parent.join(format!("{run_id}.patch")))?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 || meta.len() > 16 * 1024 * 1024 {
        return Err(RuntimeError::Integrity("artifact file"));
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 || router_adapter_api::content_digest(&bytes) != expected_hash
    {
        return Err(RuntimeError::Integrity("artifact bytes changed"));
    }
    Ok(bytes)
}
pub(crate) fn integration_digest(
    run_id: &str,
    source: &str,
    artifact: &str,
    paths: &[String],
) -> Result<String> {
    digest("runtime-integration-v1", &(run_id, source, artifact, paths))
}

#[cfg(test)]
mod observer_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Observer {
        reject_start: bool,
        starts: AtomicUsize,
        returns: AtomicUsize,
    }
    impl CaptureObserver for Observer {
        fn before_capture(&self) -> Result<()> {
            if self.reject_start {
                return Err(RuntimeError::Integrity("observer refused"));
            }
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn after_capture(&self) -> Result<()> {
            self.returns.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[tokio::test]
    async fn spawn_error_leaves_an_open_capture_window() {
        let observer = Observer::default();
        let temp = tempfile::tempdir().unwrap();
        let command = Command::new(temp.path().join("missing-program"));
        assert!(
            capture_observed(command, vec![], Some(&observer))
                .await
                .is_err()
        );
        assert_eq!(observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(observer.returns.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn observer_failure_prevents_the_process_side_effect() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("marker");
        let observer = Observer {
            reject_start: true,
            ..Observer::default()
        };
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("printf started > marker")
            .current_dir(temp.path());
        assert!(matches!(
            capture_observed(command, vec![], Some(&observer)).await,
            Err(RuntimeError::Integrity("observer refused"))
        ));
        assert!(!marker.exists());
        assert_eq!(observer.starts.load(Ordering::SeqCst), 0);
        assert_eq!(observer.returns.load(Ordering::SeqCst), 0);
    }
}
