use crate::{Cancellation, Result, RuntimeError, digest, process::capture};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
use tokio::process::Command;

pub(crate) async fn git(root: &Path, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>> {
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
    git_raw(root, args, input).await
}
async fn git_raw(root: &Path, args: &[&str], input: Vec<u8>) -> Result<Vec<u8>> {
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
    let output = capture(
        command,
        input,
        15_000,
        16 * 1024 * 1024,
        0,
        &Cancellation::default(),
    )
    .await?;
    if output.outcome.exit_code != Some(0) || output.output_limited {
        return Err(RuntimeError::Process);
    }
    Ok(output.stdout)
}
pub async fn source_state(root: &Path) -> Result<String> {
    let canonical = fs::canonicalize(root)?;
    if canonical != root || root == Path::new("/") {
        return Err(RuntimeError::Invalid("canonical project path"));
    }
    let top = git(root, &["rev-parse", "--show-toplevel"], vec![]).await?;
    if top.strip_suffix(b"\n") != Some(root.as_os_str().as_encoded_bytes()) {
        return Err(RuntimeError::Invalid("repository root"));
    }
    let status = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        vec![],
    )
    .await?;
    if !status.is_empty() {
        return Err(RuntimeError::SourceChanged);
    }
    let head = git(root, &["rev-parse", "--verify", "HEAD"], vec![]).await?;
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
    git(
        root,
        &["worktree", "add", "--detach", path_text, head],
        vec![],
    )
    .await?;
    Ok(path)
}
pub(crate) async fn patch(
    worktree: &Path,
    scope: &[String],
    base: &str,
) -> Result<(Vec<u8>, Vec<String>)> {
    safe_scope(worktree, scope)?;
    // The private worktree index is isolated from the user's repository index.
    // Intent-to-add permits binary/untracked additions in a reviewable patch.
    git(
        worktree,
        &["add", "--intent-to-add", "--all", "--", "."],
        vec![],
    )
    .await?;
    let changed = git(
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
    let patch = git(
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
pub(crate) fn integration_digest(
    run_id: &str,
    source: &str,
    artifact: &str,
    paths: &[String],
) -> Result<String> {
    digest("runtime-integration-v1", &(run_id, source, artifact, paths))
}
