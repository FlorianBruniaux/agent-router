//! Staging creates immutable Git objects and a dedicated tree ref. It never
//! checks out files, changes the source index, or claims current-source safety.
use crate::{
    Cancellation, PendingRun, Result, Runtime, RuntimeError, digest, process::capture,
    store::Store, workspace,
};
use router_adapter_api::content_digest;
use router_types::*;
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};
use tokio::process::Command;

const MAX_FILES: usize = 256;
const MAX_TREE_ENTRIES: usize = 100_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

impl Runtime {
    /// Preparation writes unreachable objects and a temporary private index.
    /// Review always uses the stored, verified patch, not a mutable worktree.
    pub async fn staged_integration_preview(
        &self,
        pending: &PendingRun,
    ) -> Result<StagedIntegrationPreview> {
        let verification = self.store.staged_verification(&pending.run)?;
        let artifact_sha256 = verification
            .artifact_sha256
            .as_ref()
            .ok_or(RuntimeError::Verification)?
            .clone();
        let bytes =
            workspace::read_artifact(&self.store.root, &pending.run.run_id, &artifact_sha256)?;
        if bytes.contains(&0)
            || bytes
                .split(|b| *b == b'\n')
                .any(|line| line == b"GIT binary patch")
        {
            return Err(RuntimeError::Invalid(
                "binary staged patches are not supported",
            ));
        }
        let root = Path::new(&pending.request.workspace);
        let config = staged_git(
            root,
            None,
            &["config", "--includes", "--name-only", "--null", "--list"],
            vec![],
        )
        .await?;
        if config
            .split(|b| *b == 0)
            .any(|name| name.to_ascii_lowercase().starts_with(b"filter."))
        {
            return Err(RuntimeError::Invalid(
                "repository filters are not permitted",
            ));
        }
        if config.split(|b| *b == 0).any(|name| {
            let name = name.to_ascii_lowercase();
            name == b"extensions.partialclone" || name.ends_with(b".promisor")
        }) {
            return Err(RuntimeError::Invalid(
                "repository lazy fetch is not permitted",
            ));
        }
        staged_git(
            root,
            None,
            &[
                "check-ref-format",
                &format!("refs/agent-router/staged/{}", pending.run.run_id),
            ],
            vec![],
        )
        .await?;
        let destination = staged_git(
            root,
            None,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            vec![],
        )
        .await?;
        let destination = String::from_utf8(destination)
            .map_err(|_| RuntimeError::Invalid("utf8 git directory"))?;
        let destination_git_dir = std::fs::canonicalize(destination.trim())?
            .to_str()
            .ok_or(RuntimeError::Invalid("utf8 git directory"))?
            .to_owned();
        let source = &pending.request.source_state;
        if !valid_oid(source) {
            return Err(RuntimeError::Invalid("source commit"));
        }
        let source_commit = git_text(
            root,
            &["rev-parse", "--verify", &format!("{source}^{{commit}}")],
        )
        .await?;
        if &source_commit != source {
            return Err(RuntimeError::Integrity("source commit changed"));
        }
        let source_tree = git_text(
            root,
            &["rev-parse", "--verify", &format!("{source}^{{tree}}")],
        )
        .await?;
        preflight(root, &source_tree, &bytes, &pending.request.scope).await?;
        let index_dir = tempfile::Builder::new()
            .prefix("staged-index-")
            .tempdir_in(&self.store.root)?;
        let index = index_dir.path().join("index");
        staged_git(root, Some(&index), &["read-tree", &source_tree], vec![]).await?;
        staged_git(
            root,
            Some(&index),
            &["apply", "--cached", "--binary", "--whitespace=nowarn", "-"],
            bytes.clone(),
        )
        .await?;
        let result_tree = oid_text(staged_git(root, Some(&index), &["write-tree"], vec![]).await?)?;
        let changes = staged_git(
            root,
            None,
            &[
                "diff-tree",
                "--raw",
                "--no-abbrev",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "-r",
                "-z",
                &source_tree,
                &result_tree,
            ],
            vec![],
        )
        .await?;
        let fields: Vec<_> = changes
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect();
        if fields.is_empty() {
            return Err(RuntimeError::Invalid("empty staged patch"));
        }
        if fields.len() > MAX_FILES * 2 {
            return Err(RuntimeError::Limit);
        }
        if fields.len() % 2 != 0 {
            return Err(RuntimeError::Integrity("git tree delta"));
        }
        let mut files = Vec::new();
        let mut remaining = MAX_BYTES;
        for pair in fields.chunks_exact(2) {
            let header = std::str::from_utf8(pair[0]).map_err(|_| RuntimeError::Scope)?;
            let path = std::str::from_utf8(pair[1]).map_err(|_| RuntimeError::Scope)?;
            check_path(path, &pending.request.scope)?;
            let parts: Vec<_> = header
                .strip_prefix(':')
                .ok_or(RuntimeError::Integrity("git tree delta"))?
                .split(' ')
                .collect();
            if parts.len() != 5 {
                return Err(RuntimeError::Integrity("git tree delta"));
            }
            let before = blob(root, parts[0], parts[2], &mut remaining).await?;
            let after = blob(root, parts[1], parts[3], &mut remaining).await?;
            files.push(StagedFileChange {
                path: path.into(),
                before,
                after,
            });
        }
        let mut preview = StagedIntegrationPreview {
            schema_version: 1,
            definition_hash: pending.run.definition_hash.clone(),
            destination_git_dir,
            run_id: pending.run.run_id.clone(),
            source_commit,
            source_tree,
            allowed_scope: pending.request.scope.clone(),
            artifact_sha256,
            verification,
            files,
            result_tree,
            approval_digest: String::new(),
            diff: String::from_utf8(bytes).map_err(|_| RuntimeError::Invalid("utf8 patch"))?,
        };
        preview.approval_digest = digest("runtime-staged-integration-v1", &preview)?;
        Ok(preview)
    }

    pub async fn stage_integration(
        &self,
        pending: &PendingRun,
        approved_digest: &str,
    ) -> Result<StagedIntegrationResult> {
        if pending
            .context
            .request()
            .permissions
            .get(&PhasePermission::Integrate)
            != Some(&CapabilitySupport::Supported)
            || !pending
                .context
                .request()
                .restrictions
                .allowed_mutations
                .as_ref()
                .is_some_and(|m| m.contains(&Mutation::Integrate))
            || !pending
                .plan
                .task
                .allowed_mutations
                .contains(&Mutation::Integrate)
        {
            return Err(RuntimeError::Approval);
        }
        let preview = self.staged_integration_preview(pending).await?;
        if preview.approval_digest != approved_digest {
            return Err(RuntimeError::Approval);
        }
        publish(
            &self.store,
            Path::new(&pending.request.workspace),
            &pending.run,
            &preview,
            #[cfg(test)]
            None,
        )
        .await
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq)]
enum Stop {
    Claimed,
    Published,
}

async fn publish(
    store: &Store,
    root: &Path,
    expected: &RunSummary,
    preview: &StagedIntegrationPreview,
    #[cfg(test)] stop: Option<Stop>,
) -> Result<StagedIntegrationResult> {
    store.claim_staged(expected, preview)?;
    #[cfg(test)]
    if stop == Some(Stop::Claimed) {
        return Err(RuntimeError::Invalid("injected interruption"));
    }
    let result = StagedIntegrationResult {
        schema_version: 1,
        run_id: preview.run_id.clone(),
        approval_digest: preview.approval_digest.clone(),
        staged_ref: format!("refs/agent-router/staged/{}", preview.run_id),
        result_tree: preview.result_tree.clone(),
    };
    // Refuse pre-existing symbolic destinations. The subsequent create is
    // atomic among managed direct-ref publishers. Unrelated symbolic-ref
    // writers racing in this dedicated namespace are outside this contract.
    if symbolic_destination(root, &result.staged_ref).await? {
        return Err(RuntimeError::Integrity("staged destination is symbolic"));
    }
    let transaction = format!(
        "option no-deref\ncreate {} {}\n",
        result.staged_ref, result.result_tree
    );
    staged_git(
        root,
        None,
        &["update-ref", "--stdin"],
        transaction.into_bytes(),
    )
    .await?;
    #[cfg(test)]
    if stop == Some(Stop::Published) {
        return Err(RuntimeError::Invalid("injected interruption"));
    }
    store.complete_staged(&result)?;
    Ok(result)
}

fn check_path(path: &str, scope: &[String]) -> Result<()> {
    if path.is_empty()
        || path.len() > 1024
        || (path.chars().any(char::is_control) || path.contains('\\'))
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        || path.split('/').any(|s| s.eq_ignore_ascii_case(".git"))
        || !scope.iter().any(|s| {
            path == s
                || path
                    .strip_prefix(s)
                    .is_some_and(|tail| tail.starts_with('/'))
        })
    {
        return Err(RuntimeError::Scope);
    }
    Ok(())
}
fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn oid_text(bytes: Vec<u8>) -> Result<String> {
    let value = String::from_utf8(bytes).map_err(|_| RuntimeError::Integrity("git object id"))?;
    let value = value.trim();
    if !valid_oid(value) {
        return Err(RuntimeError::Integrity("git object id"));
    }
    Ok(value.into())
}
async fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    oid_text(staged_git(root, None, args, vec![]).await?)
}
/// Bound the index inventory and source bytes before cached apply can inflate
/// objects. Text additions cannot exceed the patch length; binary deltas are
/// rejected before this function. This is not an OS process-memory guarantee.
async fn preflight(root: &Path, tree: &str, patch: &[u8], scope: &[String]) -> Result<()> {
    let stats = staged_git(
        root,
        None,
        &["apply", "--numstat", "-z", "-"],
        patch.to_vec(),
    )
    .await?;
    let paths: Vec<_> = stats.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
    if paths.is_empty() {
        return Err(RuntimeError::Invalid("empty staged patch"));
    }
    if paths.len() > MAX_FILES {
        return Err(RuntimeError::Limit);
    }
    let mut changed = Vec::new();
    for row in paths {
        let parts: Vec<_> = row.splitn(3, |b| *b == b'\t').collect();
        if parts.len() != 3 || parts[0] == b"-" || parts[1] == b"-" {
            return Err(RuntimeError::Invalid(
                "binary staged patches are not supported",
            ));
        }
        let path = std::str::from_utf8(parts[2]).map_err(|_| RuntimeError::Scope)?;
        check_path(path, scope)?;
        changed.push(path);
    }
    let inventory = staged_git(root, None, &["ls-tree", "-r", "-z", tree], vec![]).await?;
    let mut entries = BTreeMap::new();
    for row in inventory.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        if entries.len() >= MAX_TREE_ENTRIES {
            return Err(RuntimeError::Limit);
        }
        let separator = row
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(RuntimeError::Integrity("git tree entry"))?;
        let header = std::str::from_utf8(&row[..separator])
            .map_err(|_| RuntimeError::Integrity("git tree entry"))?;
        let parts: Vec<_> = header.split(' ').collect();
        if parts.len() != 3 || !valid_oid(parts[2]) {
            return Err(RuntimeError::Integrity("git tree entry"));
        }
        entries.insert(&row[separator + 1..], (parts[0], parts[2]));
    }
    let mut remaining = MAX_BYTES
        .checked_sub(patch.len())
        .ok_or(RuntimeError::Limit)?;
    for path in changed {
        if let Some((mode, oid)) = entries.get(path.as_bytes()) {
            if !matches!(*mode, "100644" | "100755") {
                return Err(RuntimeError::Scope);
            }
            let size = blob_size(root, oid).await?;
            remaining = remaining.checked_sub(size).ok_or(RuntimeError::Limit)?;
        }
    }
    Ok(())
}
async fn blob_size(root: &Path, oid: &str) -> Result<usize> {
    let size = staged_git(root, None, &["cat-file", "-s", oid], vec![]).await?;
    std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or(RuntimeError::Integrity("git blob size"))
}
async fn blob(
    root: &Path,
    mode: &str,
    oid: &str,
    remaining: &mut usize,
) -> Result<Option<StagedBlob>> {
    if mode == "000000" && oid.bytes().all(|b| b == b'0') {
        return Ok(None);
    }
    if !matches!(mode, "100644" | "100755") || !valid_oid(oid) {
        return Err(RuntimeError::Scope);
    }
    let size = blob_size(root, oid).await?;
    if size > *remaining {
        return Err(RuntimeError::Limit);
    }
    let bytes = staged_git(root, None, &["cat-file", "blob", oid], vec![]).await?;
    if bytes.len() != size {
        return Err(RuntimeError::Integrity("git blob size"));
    }
    *remaining -= size;
    Ok(Some(StagedBlob {
        oid: oid.into(),
        sha256: content_digest(&bytes),
        mode: mode.into(),
    }))
}
fn git_command(root: &Path, index: Option<&Path>, args: &[&str]) -> Command {
    let mut command = Command::new("/usr/bin/git");
    command
        .args([
            "--no-replace-objects",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    command
}
async fn symbolic_destination(root: &Path, reference: &str) -> Result<bool> {
    let command = git_command(root, None, &["symbolic-ref", "--quiet", reference]);
    let output = capture(
        command,
        vec![],
        15_000,
        MAX_BYTES,
        0,
        &Cancellation::default(),
    )
    .await?;
    if output.output_limited {
        return Err(RuntimeError::Process);
    }
    match output.outcome.exit_code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(RuntimeError::Process),
    }
}
async fn staged_git(
    root: &Path,
    index: Option<&Path>,
    args: &[&str],
    input: Vec<u8>,
) -> Result<Vec<u8>> {
    let output = capture(
        git_command(root, index, args),
        input,
        15_000,
        MAX_BYTES,
        0,
        &Cancellation::default(),
    )
    .await?;
    if output.outcome.exit_code != Some(0) || output.output_limited {
        return Err(RuntimeError::Process);
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interrupted_publication_never_retries_or_reports_completion_after_reopen() {
        for stop in [Stop::Claimed, Stop::Published] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("repo");
            std::fs::create_dir(&root).unwrap();
            staged_git(&root, None, &["init", "-q"], vec![])
                .await
                .unwrap();
            let tree = git_text(&root, &["mktree"]).await.unwrap();
            let storage = temp.path().join("store");
            let store = Store::open(&storage).unwrap();
            let mut run = crate::store::tests::summary("interrupted");
            run.state = RunState::Verifying;
            run.artifact_sha256 = Some(content_digest(b"fixture"));
            store.register(&run).unwrap();
            let verification = VerificationRecord {
                verifier: "independent".into(),
                evidence_sha256: "a".repeat(64),
                artifact_sha256: run.artifact_sha256.clone(),
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            };
            run = store
                .record_verification(&run.run_id, &verification, Some((RunState::Verified, None)))
                .unwrap();
            let preview = StagedIntegrationPreview {
                schema_version: 1,
                definition_hash: run.definition_hash.clone(),
                destination_git_dir: root.join(".git").to_str().unwrap().into(),
                run_id: run.run_id.clone(),
                source_commit: "1".repeat(40),
                source_tree: tree.clone(),
                allowed_scope: vec!["src".into()],
                artifact_sha256: run.artifact_sha256.clone().unwrap(),
                verification,
                files: vec![],
                result_tree: tree.clone(),
                approval_digest: "b".repeat(64),
                diff: String::new(),
            };
            assert!(matches!(
                publish(&store, &root, &run, &preview, Some(stop)).await,
                Err(RuntimeError::Invalid("injected interruption"))
            ));
            let reference = git_text(
                &root,
                &[
                    "rev-parse",
                    "--verify",
                    "refs/agent-router/staged/interrupted",
                ],
            )
            .await;
            if stop == Stop::Published {
                assert_eq!(reference.unwrap(), tree);
            } else {
                assert!(reference.is_err());
            }
            drop(store);
            let store = Store::open(&storage).unwrap();
            assert!(matches!(
                publish(&store, &root, &run, &preview, None).await,
                Err(RuntimeError::Integrity(
                    "staged integration already claimed"
                ))
            ));
            let events = store.events().unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.kind == "integration.staged.claimed")
                    .count(),
                1
            );
            assert!(
                !events
                    .iter()
                    .any(|e| e.kind == "integration.staged.completed")
            );
            // Losing a convenience artifact row must not erase the journal claim.
            let db = rusqlite::Connection::open(storage.join("runtime.sqlite3")).unwrap();
            let final_records: u64 = db
                .query_row(
                    "SELECT COUNT(*) FROM artifacts WHERE kind='staged-integration-result'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(final_records, 0);
            db.execute(
                "DELETE FROM artifacts WHERE kind='staged-integration-claim'",
                [],
            )
            .unwrap();
            assert!(matches!(
                publish(&store, &root, &run, &preview, None).await,
                Err(RuntimeError::Integrity(
                    "staged integration already claimed"
                ))
            ));
        }
    }

    #[tokio::test]
    async fn staged_blobs_refuse_symlinks_submodules_and_other_modes() {
        for mode in ["120000", "160000", "040000", "100600"] {
            let mut remaining = MAX_BYTES;
            assert!(matches!(
                blob(Path::new("/"), mode, &"1".repeat(40), &mut remaining).await,
                Err(RuntimeError::Scope)
            ));
        }
    }

    #[test]
    fn staged_paths_refuse_terminal_control_characters() {
        for path in ["src/a\tb", "src/a\u{1b}b", "src/a\u{7f}b"] {
            assert!(matches!(
                check_path(path, &["src".into()]),
                Err(RuntimeError::Scope)
            ));
        }
    }
}
