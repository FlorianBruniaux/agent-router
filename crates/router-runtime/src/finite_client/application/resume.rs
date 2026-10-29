// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Reopen only persisted controller context; never infer authority from a path.
use super::*;
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Context {
    version: u8,
    source: PathBuf,
    directory: FileIdentity,
    definition: String,
    execution_approval: String,
    review_sha256: String,
    source_snapshot: String,
    git_snapshot: String,
}
impl Context {
    pub(super) fn capture(
        h: &FiniteClientApplication<'_>,
        review: &FiniteApplicationReview,
    ) -> Result<Self> {
        Ok(Self {
            version: 1,
            source: h.source.clone(),
            directory: h.directory.identity.clone(),
            definition: h.definition.clone(),
            execution_approval: h.execution_approval.clone(),
            review_sha256: crate::digest("finite-resume-original-review-v1", review)?,
            source_snapshot: snapshot(h)?,
            git_snapshot: git_snapshot(h)?,
        })
    }
}
// Include timestamps to reject changed-then-restored bytes on the same inode.
// Atime is deliberately excluded because observation may update it.
fn stamp(m: &fs::Metadata) -> (FileIdentity, u64, u64, i64, i64, i64, i64) {
    (
        FileIdentity::from(m),
        m.nlink(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn flat(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
fn snapshot(h: &FiniteClientApplication<'_>) -> Result<String> {
    use rustix::fs::{Mode, OFlags, openat};
    h.source_identity()?;
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(&h.source)? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| RuntimeError::Scope)?;
        if name == ".git" {
            continue;
        }
        if !flat(&name) || files.len() == 16 {
            return Err(RuntimeError::Scope);
        }
        let mut f = File::from(
            openat(
                &h.directory._file,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?,
        );
        let meta = f.metadata()?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
        {
            return Err(RuntimeError::Scope);
        }
        let before = stamp(&meta);
        let bytes = FiniteClientApplication::bytes(&mut f)?;
        if stamp(&f.metadata()?) != before
            || stamp(&fs::symlink_metadata(h.source.join(&name))?) != before
        {
            return Err(RuntimeError::SourceChanged);
        }
        files.insert(name, (before, content_digest(&bytes)));
    }
    if files.is_empty() {
        return Err(RuntimeError::Scope);
    }
    h.source_identity()?;
    crate::digest("finite-resume-source-snapshot-v1", &files)
}

// Refuse changed repository metadata before invoking Git, including includes,
// alternates and symlinks which could otherwise expand the read boundary.
fn git_snapshot(h: &FiniteClientApplication<'_>) -> Result<String> {
    use std::os::unix::fs::OpenOptionsExt;
    let root = h.source.join(".git");
    let mut pending = vec![root.clone()];
    let mut values = BTreeMap::new();
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        if values.len() + pending.len() >= 256 {
            return Err(RuntimeError::Limit);
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.uid() != rustix::process::geteuid().as_raw()
            || meta.file_type().is_symlink()
            || meta.mode() & 0o022 != 0
        {
            return Err(RuntimeError::Scope);
        }
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| RuntimeError::Scope)?
            .to_path_buf();
        if meta.is_dir() {
            values.insert(relative, (meta.mode(), String::new()));
            for e in fs::read_dir(&path)? {
                if values.len() + pending.len() >= 256 {
                    return Err(RuntimeError::Limit);
                }
                pending.push(e?.path());
            }
        } else if meta.is_file() && meta.nlink() == 1 && meta.len() <= 1_048_576 {
            total += meta.len();
            if total > 8_388_608 {
                return Err(RuntimeError::Limit);
            }
            let mut f = fs::OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
                .open(&path)?;
            if stamp(&f.metadata()?) != stamp(&meta) {
                return Err(RuntimeError::SourceChanged);
            }
            let mut bytes = Vec::new();
            (&mut f).take(1_048_577).read_to_end(&mut bytes)?;
            if bytes.len() > 1_048_576 || stamp(&f.metadata()?) != stamp(&meta) {
                return Err(RuntimeError::SourceChanged);
            }
            values.insert(relative, (meta.mode(), content_digest(&bytes)));
        } else {
            return Err(RuntimeError::Scope);
        }
    }
    h.source_identity()?;
    crate::digest("finite-resume-git-metadata-v1", &values)
}

async fn clean_source(
    h: &FiniteClientApplication<'_>,
    original: &FiniteApplicationReview,
) -> Result<bool> {
    let top = workspace::git(
        &h.source,
        &["--no-optional-locks", "rev-parse", "--show-toplevel"],
        vec![],
    )
    .await?;
    if top.strip_suffix(b"\n") != Some(h.source.as_os_str().as_encoded_bytes()) {
        return Err(RuntimeError::Scope);
    }
    let status = workspace::git(
        &h.source,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
        vec![],
    )
    .await?;
    let head = oid(workspace::git(
        &h.source,
        &["--no-optional-locks", "rev-parse", "HEAD"],
        vec![],
    )
    .await?)?;
    let tree = oid(workspace::git(
        &h.source,
        &[
            "--no-optional-locks",
            "rev-parse",
            &format!("{}^{{tree}}", h.revision),
        ],
        vec![],
    )
    .await?)?;
    Ok(status.is_empty() && head == h.revision && tree == original.source_tree)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FiniteResumeState {
    PreimageIntact,
    CandidateObserved,
    Divergent,
    Completed,
    Consumed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FiniteResumeReview {
    pub schema_version: u8,
    pub run_id: String,
    pub state: FiniteResumeState,
    pub original_approval: String,
    pub history_sha256: String,
    pub source_snapshot: String,
    pub observed_sha256: String,
    pub candidate_sha256: String,
    pub approval_digest: Option<String>,
}
/// Display-only envelope; the persisted original review format stays unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FiniteResumePresentation {
    pub review: FiniteResumeReview,
    pub original: FiniteApplicationReview,
    pub diff: String,
}
/// Lives only in this reopened controller. Neither original approval nor a
/// review from a different handle can authorize a write. No worker is started.
pub struct FiniteResume<'a> {
    h: FiniteClientApplication<'a>,
    context: Context,
    original: FiniteApplicationReview,
    nonce: [u8; 32],
    expires: Instant,
    _target: File,
}
impl Runtime {
    pub fn finite_resume(&self, id: &str) -> Result<FiniteResume<'_>> {
        self.verify_integrity()?;
        self.store.require_cleanup_complete()?;
        let events = self.events()?;
        if events.len() > 256 {
            return Err(RuntimeError::Limit);
        }
        let rows: Vec<_> = events
            .iter()
            .filter(|e| e.run_id == id && e.kind.starts_with("integration."))
            .collect();
        if rows.len() < 2
            || rows[0].kind != "integration.context.v1"
            || rows[1].kind != "integration.intent"
        {
            return Err(RuntimeError::Integrity("missing original resume context"));
        }
        let context: Context = serde_json::from_str(&rows[0].detail)?;
        let original: FiniteApplicationReview = serde_json::from_str(&rows[1].detail)?;
        let run = self.store.get(id)?;
        if context.version != 1
            || original.run_id != id
            || !flat(&original.output)
            || context.source.parent() != self.store.root.parent()
            || context.source == self.store.root
            || context.definition != run.definition_hash
            || context.execution_approval != run.approval_digest
            || context.review_sha256
                != crate::digest("finite-resume-original-review-v1", &original)?
            || run.artifact_sha256.as_ref() != Some(&original.artifact_sha256)
            || run.state != RunState::Verified
            || self.cleanup_evidence(id)? != ProcessCleanupEvidence::NamespaceTerminated
        {
            return Err(RuntimeError::Verification);
        }
        let directory = directory(context.source.to_str().ok_or(RuntimeError::Scope)?, true)?;
        if directory.identity != context.directory {
            return Err(RuntimeError::SourceChanged);
        }
        let h = FiniteClientApplication {
            runtime: self,
            run_id: id.into(),
            definition: context.definition.clone(),
            execution_approval: context.execution_approval.clone(),
            owner: String::new(),
            independent: false,
            source: context.source.clone(),
            directory,
            revision: original.source_revision.clone(),
            relative: original.output.clone(),
            preimage: original.preimage_sha256.clone(),
        };
        let mut unsigned = original.clone();
        unsigned.approval_digest.clear();
        if crate::digest(
            "finite-controller-accept-apply-v1",
            &(
                &unsigned,
                &h.definition,
                &h.execution_approval,
                &h.source,
                &h.directory.identity,
            ),
        )? != original.approval_digest
        {
            return Err(RuntimeError::Approval);
        }
        let target = h.target(false)?;
        let mut nonce = [0; 32];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        Ok(FiniteResume {
            h,
            context,
            original,
            nonce,
            expires: Instant::now() + Duration::from_secs(60),
            _target: target,
        })
    }
}
impl FiniteResume<'_> {
    #[cfg(test)]
    pub(super) fn expire(&mut self) {
        self.expires = Instant::now();
    }
    pub async fn review(&self) -> Result<FiniteResumeReview> {
        self.h.runtime.verify_integrity()?;
        let accepted = VerificationRecord {
            verifier: "explicit-controller-candidate-acceptance-v1".into(),
            evidence_sha256: crate::digest("finite-controller-acceptance-v1", &self.original)?,
            artifact_sha256: Some(self.original.artifact_sha256.clone()),
            validation: VerificationOutcome::Passed,
            objective: VerificationOutcome::Passed,
        };
        if self.h.runtime.verification_record(&self.h.run_id)?.as_ref() != Some(&accepted) {
            return Err(RuntimeError::Verification);
        }

        self.h.runtime.store.require_cleanup_complete()?;
        if git_snapshot(&self.h)? != self.context.git_snapshot {
            return Err(RuntimeError::SourceChanged);
        }
        let events = self.h.runtime.events()?;
        if events.len() > 256 {
            return Err(RuntimeError::Limit);
        }
        let rows: Vec<_> = events
            .iter()
            .filter(|e| e.run_id == self.h.run_id && e.kind.starts_with("integration."))
            .collect();
        let observed = content_digest(&FiniteClientApplication::bytes(&mut self.h.target(false)?)?);
        let current_snapshot = snapshot(&self.h)?;
        let state = if rows.iter().any(|e| {
            matches!(
                e.kind.as_str(),
                "integration.applied" | "integration.resume_applied.v1"
            )
        }) {
            FiniteResumeState::Completed
        } else if rows
            .iter()
            .any(|e| e.kind == "integration.resume_intent.v1")
        {
            FiniteResumeState::Consumed
        } else if observed == self.original.candidate_sha256 {
            FiniteResumeState::CandidateObserved
        } else if current_snapshot == self.context.source_snapshot
            && observed == self.h.preimage
            && clean_source(&self.h, &self.original).await?
        {
            FiniteResumeState::PreimageIntact
        } else {
            FiniteResumeState::Divergent
        };
        let mut review = FiniteResumeReview {
            schema_version: 1,
            run_id: self.h.run_id.clone(),
            state,
            original_approval: self.original.approval_digest.clone(),
            history_sha256: events
                .last()
                .ok_or(RuntimeError::Verification)?
                .event_hash
                .clone(),
            source_snapshot: current_snapshot,
            observed_sha256: observed,
            candidate_sha256: self.original.candidate_sha256.clone(),
            approval_digest: None,
        };
        if review.state == FiniteResumeState::PreimageIntact && Instant::now() < self.expires {
            review.approval_digest = Some(crate::digest(
                "finite-explicit-resume-v1",
                &(&review, &self.nonce),
            )?);
        }
        Ok(review)
    }
    pub async fn presentation(&self) -> Result<FiniteResumePresentation> {
        let review = self.review().await?;
        let diff = self.h.captured_diff(&self.original)?;
        Ok(FiniteResumePresentation {
            review,
            original: self.original.clone(),
            diff,
        })
    }
    pub async fn validate_presentation(&self, packet: &FiniteResumePresentation) -> Result<()> {
        if packet.diff.len() > 131072
            || content_digest(packet.diff.as_bytes()) != self.original.artifact_sha256
            || &self.presentation().await? != packet
        {
            return Err(RuntimeError::Integrity(
                "finite resume presentation changed",
            ));
        }
        Ok(())
    }
    pub async fn apply_presented(
        &self,
        packet: &FiniteResumePresentation,
        approved: &str,
    ) -> Result<()> {
        self.validate_presentation(packet).await?;
        self.apply(&packet.review, approved).await
    }
    pub async fn apply(&self, review: &FiniteResumeReview, approved: &str) -> Result<()> {
        self.apply_observed(review, approved, None).await
    }
    pub(super) async fn apply_observed(
        &self,
        review: &FiniteResumeReview,
        approved: &str,
        observer: Option<&dyn workspace::CaptureObserver>,
    ) -> Result<()> {
        if &self.review().await? != review || review.approval_digest.as_deref() != Some(approved) {
            return Err(RuntimeError::Approval);
        }
        let patch = workspace::read_artifact(
            &self.h.runtime.store.root,
            &self.h.run_id,
            &self.original.artifact_sha256,
        )?;
        if patch.is_empty() || patch.len() > 131072 {
            return Err(RuntimeError::Limit);
        }
        let candidate = workspace::git(
            &self.h.source,
            &[
                "show",
                &format!("{}:{}", self.original.candidate_tree, self.h.relative),
            ],
            vec![],
        )
        .await?;
        if candidate.len() > 32768 || content_digest(&candidate) != self.original.candidate_sha256 {
            return Err(RuntimeError::Integrity("resume candidate"));
        }
        // Recheck after all awaited reads. The store flock excludes other
        // controllers; exclusive same-uid source ownership remains required.
        if &self.review().await? != review {
            return Err(RuntimeError::Approval);
        }
        let mut file = self.h.target(true)?;
        let identity = FileIdentity::from(&file.metadata()?);
        self.h.runtime.store.claim_finite_resume(
            &self.h.run_id,
            &review.history_sha256,
            approved,
        )?;
        if let Some(observer) = observer {
            observer.before_capture()?;
        }
        if Instant::now() >= self.expires
            || snapshot(&self.h)? != review.source_snapshot
            || git_snapshot(&self.h)? != self.context.git_snapshot
            || FileIdentity::from(&self.h.target(false)?.metadata()?) != identity
            || content_digest(&FiniteClientApplication::bytes(&mut file)?) != self.h.preimage
        {
            return Err(RuntimeError::SourceChanged);
        }
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&candidate)?;
        file.set_len(candidate.len() as u64)?;
        file.sync_all()?;
        if let Some(observer) = observer {
            observer.after_capture()?;
        }
        self.h.source_identity()?;
        if FileIdentity::from(&self.h.target(false)?.metadata()?) != identity
            || FiniteClientApplication::bytes(&mut file)? != candidate
        {
            return Err(RuntimeError::Integrity("resume result"));
        }
        self.h.runtime.store.event(
            &self.h.run_id,
            "integration.resume_applied.v1",
            &self.original.artifact_sha256,
        )?;
        Ok(())
    }
}
