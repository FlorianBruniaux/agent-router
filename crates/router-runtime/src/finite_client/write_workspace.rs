//! One producer-owned private write view. No process or publication authority.
use super::*;
use std::path::PathBuf;
const MAX_FILE: u64 = 32768;

pub struct FiniteClientWorkspace {
    path: PathBuf,
    parent: PathBuf,
    source: PathBuf,
    owner: PathBuf,
    task_id: String,
    head: String,
    relative: String,
    preimage: String,
    view_identity: PinnedIdentity,
    parent_identity: PinnedIdentity,
    source_identity: PinnedIdentity,
    target_identity: PinnedIdentity,
}
#[derive(Clone)]
pub(super) struct WriteDispatch {
    pub(super) path: PathBuf,
    pub(super) source: PathBuf,
    pub(super) owner: PathBuf,
    pub(super) task_id: String,
    pub(super) head: String,
    pub(super) relative: String,
    pub(super) preimage: String,
}
impl WriteDispatch {
    pub(super) async fn check_sources(
        &self,
        observer: Option<&dyn crate::workspace::CaptureObserver>,
    ) -> Result<()> {
        if crate::workspace::source_state_observed(&self.source, observer).await? != self.head
            || crate::workspace::source_state_observed(&self.path, observer).await? != self.head
        {
            return Err(RuntimeError::SourceChanged);
        }
        Ok(())
    }
}
fn flat_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn target_file(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.nlink() != 1
        || m.len() > MAX_FILE
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o7022 != 0
    {
        return Err(RuntimeError::Scope);
    }
    Ok(file)
}
fn target_hash(path: &Path) -> Result<String> {
    let mut bytes = vec![];
    target_file(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(RuntimeError::Limit);
    }
    Ok(content_digest(&bytes))
}
impl FiniteClientWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn source_state(&self) -> &str {
        &self.head
    }
    pub fn scope(&self) -> &[String] {
        std::slice::from_ref(&self.relative)
    }
    pub fn preimage_sha256(&self) -> &str {
        &self.preimage
    }
    pub(super) fn dispatch(&self) -> WriteDispatch {
        WriteDispatch {
            path: self.path.clone(),
            source: self.source.clone(),
            owner: self.owner.clone(),
            task_id: self.task_id.clone(),
            head: self.head.clone(),
            relative: self.relative.clone(),
            preimage: self.preimage.clone(),
        }
    }
    pub(super) fn check_identity(&self) -> Result<()> {
        if directory(self.path.to_str().ok_or(RuntimeError::Scope)?, true)?.identity
            != self.view_identity.identity
            || directory(self.parent.to_str().ok_or(RuntimeError::Scope)?, true)?.identity
                != self.parent_identity.identity
            || FileIdentity::from(&target_file(&self.path.join(&self.relative))?.metadata()?)
                != self.target_identity.identity
            || FileIdentity::from(&self.target_identity._file.metadata()?)
                != self.target_identity.identity
        {
            return Err(RuntimeError::Integrity("finite workspace identity"));
        }
        Ok(())
    }
    pub(super) fn check_for_review(&self, store: &Path, id: &str, run: &PreparedRun) -> Result<()> {
        self.check_identity()?;
        if self.owner != store
            || self.task_id != id
            || self.path != Path::new(&run.cwd)
            || self.head != run.source_state
            || self.scope() != run.scope
            || run.sandbox != router_types::ExecutionSandbox::WorkspaceWrite
            || directory(self.source.to_str().ok_or(RuntimeError::Scope)?, false)?.identity
                != self.source_identity.identity
            || target_hash(&self.path.join(&self.relative))? != self.preimage
        {
            return Err(RuntimeError::Integrity("finite workspace review"));
        }
        Ok(())
    }
    fn binding(&self) -> Result<String> {
        crate::digest(
            "finite-workspace-review-v1",
            &(
                &self.path,
                &self.parent,
                &self.source,
                &self.owner,
                &self.task_id,
                &self.head,
                &self.relative,
                &self.preimage,
                &self.view_identity.identity,
                &self.parent_identity.identity,
                &self.source_identity.identity,
                &self.target_identity.identity,
            ),
        )
    }
    /// Discards only an unconsumed preparation. Once consumed by a review this
    /// handle is unavailable. Executed views remain retained for patch review.
    pub async fn discard(self) -> Result<()> {
        self.check_identity()?;
        if directory(self.source.to_str().ok_or(RuntimeError::Scope)?, false)?.identity
            != self.source_identity.identity
        {
            return Err(RuntimeError::Integrity("finite workspace source"));
        }
        crate::workspace::git(
            &self.source,
            &[
                "worktree",
                "remove",
                "--force",
                self.path.to_str().ok_or(RuntimeError::Scope)?,
            ],
            vec![],
        )
        .await?;
        Ok(())
    }
}
impl FiniteClientReview {
    /// Binds an actual producer-created view before final approval. Never
    /// relocates a frozen run or accepts an arbitrary path from a receipt.
    pub fn with_write_workspace(mut self, workspace: FiniteClientWorkspace) -> Result<Self> {
        if self.owned_workspace.is_some() || self.owned_assets.is_none() {
            return Err(RuntimeError::Sandbox);
        }
        let state = self.owned_state.as_ref().ok_or(RuntimeError::Sandbox)?;
        workspace.check_for_review(
            state.path().parent().ok_or(RuntimeError::Scope)?,
            &self.manifest.task_id,
            &self.run,
        )?;
        self.approval = crate::digest(
            "finite-client-workspace-v1",
            &(&self.approval, workspace.binding()?),
        )?;
        self.owned_workspace = Some(workspace);
        self.check_files()?;
        Ok(self)
    }
}
impl crate::Runtime {
    /// Copies the clean public Git snapshot into a private, detached write view
    /// before context/review construction. One existing flat file only. This
    /// grants no native execution, admission, publication or integration.
    pub async fn prepare_finite_workspace(
        &self,
        source: &Path,
        task_id: &str,
        scope: &[String],
    ) -> Result<FiniteClientWorkspace> {
        if !router_adapter_api::safe_identifier(task_id)
            || task_id.len() > 96
            || scope.len() != 1
            || !flat_name(&scope[0])
            || source.starts_with(&self.store.root)
            || self.store.root.starts_with(source)
        {
            return Err(RuntimeError::Scope);
        }
        let relative = scope[0].clone();
        let source_identity = directory(source.to_str().ok_or(RuntimeError::Scope)?, false)?;
        crate::workspace::safe_scope(source, scope)?;
        let preimage = target_hash(&source.join(&relative))?;
        let head = crate::source_state(source).await?;
        let parent = self
            .store
            .root
            .parent()
            .ok_or(RuntimeError::Scope)?
            .join(format!(
                "finite-workspaces-{}",
                crate::admission_runtime_root_hash(&self.store.root)?
            ));
        if parent.starts_with(source) || source.starts_with(&parent) {
            return Err(RuntimeError::Scope);
        }
        match fs::DirBuilder::new().mode(0o700).create(&parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let parent_identity = directory(parent.to_str().ok_or(RuntimeError::Scope)?, true)?;
        let path = crate::workspace::create(source, &parent, task_id, &head, None).await?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        let view_identity = directory(path.to_str().ok_or(RuntimeError::Scope)?, true)?;
        let file = target_file(&path.join(&relative))?;
        let target_identity = PinnedIdentity {
            identity: FileIdentity::from(&file.metadata()?),
            _file: file,
        };
        let view = FiniteClientWorkspace {
            path,
            parent,
            source: source.into(),
            owner: self.store.root.clone(),
            task_id: task_id.into(),
            head,
            relative,
            preimage,
            view_identity,
            parent_identity,
            source_identity,
            target_identity,
        };
        if crate::source_state(source).await? != view.head
            || crate::source_state(view.path()).await? != view.head
            || target_hash(&view.path.join(&view.relative))? != view.preimage
        {
            return Err(RuntimeError::SourceChanged);
        }
        view.check_identity()?;
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, crate::Runtime, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let source = base.join("source");
        fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
        fs::write(source.join("allowed.rs"), b"before\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "allowed.rs"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "--no-gpg-sign",
                "-m",
                "public source",
            ],
        ] {
            crate::workspace::git(&source, &args, vec![]).await.unwrap();
        }
        let runtime = crate::Runtime::open(&base.join("runtime"), Default::default()).unwrap();
        (root, runtime, source)
    }
    #[tokio::test]
    async fn finite_workspace_is_prepared_before_review_and_keeps_original_bytes() {
        let (_root, runtime, source) = fixture().await;
        let view = runtime
            .prepare_finite_workspace(&source, "bounded", &["allowed.rs".into()])
            .await
            .unwrap();
        assert_ne!(view.path(), source);
        assert!(!view.path().starts_with(&runtime.store.root));
        assert_eq!(
            crate::source_state(view.path()).await.unwrap(),
            view.source_state()
        );
        assert_eq!(
            fs::read(view.path().join("allowed.rs")).unwrap(),
            b"before\n"
        );
        assert_eq!(fs::read(source.join("allowed.rs")).unwrap(), b"before\n");
        let a = fs::metadata(view.path().join("allowed.rs")).unwrap();
        let b = fs::metadata(source.join("allowed.rs")).unwrap();
        assert_ne!((a.dev(), a.ino()), (b.dev(), b.ino()));
        assert_eq!(a.nlink(), 1);
    }
    #[tokio::test]
    async fn finite_workspace_binds_the_exact_view_and_refuses_stale_preimages() {
        let (_root, runtime, source) = fixture().await;
        let view = runtime
            .prepare_finite_workspace(&source, "bounded", &["allowed.rs".into()])
            .await
            .unwrap();
        let run = PreparedRun {
            schema_version: 1,
            receipt_id: "a".repeat(64),
            source_state: view.head.clone(),
            scope: view.scope().to_vec(),
            authorization_digest: "b".repeat(64),
            snapshot_hash: "c".repeat(64),
            host: router_types::HarnessId::parse("codex").unwrap(),
            requested_model: router_types::ModelId::parse("fixture").unwrap(),
            requested_effort: router_types::EffortLevel::Medium,
            program: "finite-client".into(),
            argv: vec!["--worker".into()],
            cwd: view.path.to_str().unwrap().into(),
            stdin: router_types::PreparedInput::new(vec![]),
            mode: router_types::ExecutionMode::Execute,
            target: router_types::ExecutionTarget::ExternalWorker,
            sandbox: router_types::ExecutionSandbox::WorkspaceWrite,
            timeout_ms: 1000,
            max_output_bytes: 65536,
            degradations: vec![],
            missing_runtime_evidence: vec![],
            parent_model_changed: false,
        };
        view.check_for_review(&runtime.store.root, "bounded", &run)
            .unwrap();
        assert!(
            view.check_for_review(&runtime.store.root, "another", &run)
                .is_err()
        );
        assert!(view.check_for_review(&source, "bounded", &run).is_err());
        let mut relocated = run.clone();
        relocated.cwd = source.to_str().unwrap().into();
        assert!(
            view.check_for_review(&runtime.store.root, "bounded", &relocated)
                .is_err()
        );
        fs::write(view.path.join("allowed.rs"), b"after\n").unwrap();
        assert!(
            view.check_for_review(&runtime.store.root, "bounded", &run)
                .is_err()
        );
        // Scoped mutations must not prevent identity-based cleanup afterwards.
        view.check_identity().unwrap();
        assert_eq!(fs::read(source.join("allowed.rs")).unwrap(), b"before\n");
        let path = view.path.clone();
        view.discard().await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn finite_workspace_rechecks_non_target_files_before_dispatch() {
        let (_root, runtime, source) = fixture().await;
        let view = runtime
            .prepare_finite_workspace(&source, "bounded", &["allowed.rs".into()])
            .await
            .unwrap();
        let dispatch = view.dispatch();
        dispatch.check_sources(None).await.unwrap();
        fs::write(view.path().join("unexpected.rs"), b"drift").unwrap();
        assert!(matches!(
            dispatch.check_sources(None).await,
            Err(RuntimeError::SourceChanged)
        ));
        assert_eq!(fs::read(source.join("allowed.rs")).unwrap(), b"before\n");
        view.discard().await.unwrap();
    }
    #[tokio::test]
    async fn finite_workspace_refuses_directories_multiple_files_and_escapes() {
        let (_root, runtime, source) = fixture().await;
        for scope in [
            vec![],
            vec!["allowed.rs".into(), "other.rs".into()],
            vec!["../outside".into()],
            vec![".git".into()],
            vec!["missing.rs".into()],
            vec!["src/file.rs".into()],
        ] {
            assert!(
                runtime
                    .prepare_finite_workspace(&source, "bad", &scope)
                    .await
                    .is_err()
            );
        }
    }
}
