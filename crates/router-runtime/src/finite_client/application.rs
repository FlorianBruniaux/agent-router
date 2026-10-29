// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Separate controller approval for a private, single-file candidate application.
//! Worker approval and verifier handles never acquire this authority implicitly.
use super::{ApprovedFiniteClientRun, FileIdentity, PinnedIdentity, directory};
use crate::{Result, Runtime, RuntimeError, content_digest, source_state, workspace};
use router_types::{ProcessCleanupEvidence, RunState, VerificationOutcome, VerificationRecord};
use serde::{Deserialize, Serialize};
pub(super) mod presentation;
pub(super) mod resume;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteApplicationReview {
    pub run_id: String,
    pub source_revision: String,
    pub source_tree: String,
    pub candidate_tree: String,
    pub artifact_sha256: String,
    pub technical_evidence_sha256: String,
    pub technical_record_sha256: String,
    pub output: String,
    pub preimage_sha256: String,
    pub candidate_sha256: String,
    pub approval_digest: String,
}

/// Nonserializable, controller-held authority for a private sibling of the store.
/// No generic PendingRun, execution, ref publication or reopen API is exposed.
/// The source must have one exclusive controller writer after namespace cleanup.
pub struct FiniteClientApplication<'runtime> {
    runtime: &'runtime Runtime,
    run_id: String,
    definition: String,
    execution_approval: String,
    owner: String,
    independent: bool,
    source: PathBuf,
    directory: PinnedIdentity,
    revision: String,
    relative: String,
    preimage: String,
}

impl Runtime {
    /// Retain a distinct controller capability before dispatch; no side effect.
    /// A later explicit accept-and-apply digest is still required.
    pub fn finite_client_application<'a>(
        &'a self,
        approved: &ApprovedFiniteClientRun,
    ) -> Result<FiniteClientApplication<'a>> {
        self.finite_client_verifier(approved)?;
        let p = &approved.approved.pending;
        let view = approved
            .client
            .reviewed
            .owned_workspace
            .as_ref()
            .ok_or(RuntimeError::Scope)?;
        let d = view.dispatch();
        if d.owner != self.store.root || d.source.parent() != self.store.root.parent() {
            return Err(RuntimeError::Scope);
        }
        Ok(FiniteClientApplication {
            runtime: self,
            run_id: p.run.run_id.clone(),
            definition: p.run.definition_hash.clone(),
            execution_approval: p.run.approval_digest.clone(),
            owner: p.node.owner.clone(),
            independent: p.decision.selection().candidate.verification
                == router_types::PhaseVerification::Independent,
            directory: directory(d.source.to_str().ok_or(RuntimeError::Scope)?, true)?,
            source: d.source,
            revision: d.head,
            relative: d.relative,
            preimage: d.preimage.ok_or(RuntimeError::Scope)?,
        })
    }
}

impl FiniteClientApplication<'_> {
    fn source_identity(&self) -> Result<()> {
        let observed = directory(self.source.to_str().ok_or(RuntimeError::Scope)?, true)?;
        if observed.identity != self.directory.identity
            || FileIdentity::from(&self.directory._file.metadata()?) != self.directory.identity
        {
            return Err(RuntimeError::SourceChanged);
        }
        Ok(())
    }
    fn target(&self, write: bool) -> Result<File> {
        use rustix::fs::{Mode, OFlags, openat};
        self.source_identity()?;
        let flags = if write { OFlags::RDWR } else { OFlags::RDONLY };
        let file = File::from(
            openat(
                &self.directory._file,
                self.relative.as_str(),
                flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?,
        );
        let m = file.metadata()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.len() > 32768
            || m.mode() & 0o7777 != 0o600
            || m.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(RuntimeError::Scope);
        }
        Ok(file)
    }
    fn bytes(file: &mut File) -> Result<Vec<u8>> {
        file.seek(SeekFrom::Start(0))?;
        let mut b = vec![];
        file.take(32769).read_to_end(&mut b)?;
        if b.len() > 32768 {
            return Err(RuntimeError::Limit);
        }
        Ok(b)
    }
    async fn prepared(&self) -> Result<(FiniteApplicationReview, Vec<u8>)> {
        let run = self.runtime.store.get(&self.run_id)?;
        let technical = self
            .runtime
            .verification_record(&self.run_id)?
            .ok_or(RuntimeError::Verification)?;
        if run.state != RunState::Verifying
            || run.definition_hash != self.definition
            || run.approval_digest != self.execution_approval
            || self.runtime.cleanup_evidence(&self.run_id)?
                != ProcessCleanupEvidence::NamespaceTerminated
            || technical.validation != VerificationOutcome::Passed
            || technical.objective != VerificationOutcome::Unknown
            || technical.artifact_sha256 != run.artifact_sha256
        {
            return Err(RuntimeError::Verification);
        }
        self.source_identity()?;
        if source_state(&self.source).await? != self.revision
            || content_digest(&Self::bytes(&mut self.target(false)?)?) != self.preimage
        {
            return Err(RuntimeError::SourceChanged);
        }
        let artifact = run.artifact_sha256.ok_or(RuntimeError::Verification)?;
        let patch = workspace::read_artifact(&self.runtime.store.root, &self.run_id, &artifact)?;
        if patch.is_empty() || patch.len() > 131072 {
            return Err(RuntimeError::Limit);
        }
        // Flat public packets only; bound the inventory before private read-tree.
        let inventory =
            workspace::git(&self.source, &["ls-tree", "-z", &self.revision], vec![]).await?;
        let entries: Vec<_> = inventory
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
            .collect();
        if entries.is_empty()
            || entries.len() > 16
            || entries.iter().any(|e| !e.starts_with(b"100644 blob "))
        {
            return Err(RuntimeError::Scope);
        }
        let temp = tempfile::Builder::new()
            .prefix("finite-application-")
            .tempdir_in(&self.runtime.store.root)?;
        let index = temp.path().join("index");
        workspace::git_index(&self.source, &["read-tree", &self.revision], vec![], &index).await?;
        let source_tree =
            oid(workspace::git_index(&self.source, &["write-tree"], vec![], &index).await?)?;
        workspace::git_index(
            &self.source,
            &["apply", "--cached", "--binary", "-"],
            patch,
            &index,
        )
        .await?;
        let paths = workspace::git_index(
            &self.source,
            &[
                "diff",
                "--cached",
                "--no-renames",
                "--name-only",
                "-z",
                &self.revision,
            ],
            vec![],
            &index,
        )
        .await?;
        if paths != format!("{}\0", self.relative).as_bytes() {
            return Err(RuntimeError::Scope);
        }
        let candidate_tree =
            oid(workspace::git_index(&self.source, &["write-tree"], vec![], &index).await?)?;
        for tree in [&source_tree, &candidate_tree] {
            let row = workspace::git(
                &self.source,
                &["ls-tree", tree, "--", &self.relative],
                vec![],
            )
            .await?;
            if !row.starts_with(b"100644 blob ") {
                return Err(RuntimeError::Scope);
            }
        }
        let candidate = workspace::git(
            &self.source,
            &["show", &format!("{candidate_tree}:{}", self.relative)],
            vec![],
        )
        .await?;
        if candidate.len() > 32768 || content_digest(&candidate) == self.preimage {
            return Err(RuntimeError::Scope);
        }
        let mut review = FiniteApplicationReview {
            run_id: self.run_id.clone(),
            source_revision: self.revision.clone(),
            source_tree,
            candidate_tree,
            artifact_sha256: artifact,
            technical_record_sha256: crate::digest("finite-technical-record-v1", &technical)?,
            technical_evidence_sha256: technical.evidence_sha256,
            output: self.relative.clone(),
            preimage_sha256: self.preimage.clone(),
            candidate_sha256: content_digest(&candidate),
            approval_digest: String::new(),
        };
        review.approval_digest = crate::digest(
            "finite-controller-accept-apply-v1",
            &(
                &review,
                &self.definition,
                &self.execution_approval,
                &self.source,
                &self.directory.identity,
            ),
        )?;
        Ok((review, candidate))
    }
    pub async fn review(&self) -> Result<FiniteApplicationReview> {
        Ok(self.prepared().await?.0)
    }

    /// Explicit controller acceptance of the candidate, followed by one in-place
    /// application. Candidate acceptance is not human acceptance or proof of an
    /// applied source. Interrupted intents remain consumed and are never replayed.
    pub async fn accept_and_apply(
        &self,
        review: &FiniteApplicationReview,
        approved: &str,
    ) -> Result<()> {
        self.apply_observed(review, approved, None).await
    }
    async fn apply_observed(
        &self,
        review: &FiniteApplicationReview,
        approved: &str,
        observer: Option<&dyn workspace::CaptureObserver>,
    ) -> Result<()> {
        let (current, candidate) = self.prepared().await?;
        if &current != review || current.approval_digest != approved {
            return Err(RuntimeError::Approval);
        }
        let mut file = self.target(true)?;
        let identity = FileIdentity::from(&file.metadata()?);
        if source_state(&self.source).await? != self.revision {
            return Err(RuntimeError::SourceChanged);
        }
        self.source_identity()?;
        if FileIdentity::from(&self.target(false)?.metadata()?) != identity
            || content_digest(&Self::bytes(&mut file)?) != self.preimage
        {
            return Err(RuntimeError::SourceChanged);
        }
        let technical = self
            .runtime
            .verification_record(&self.run_id)?
            .ok_or(RuntimeError::Verification)?;
        if crate::digest("finite-technical-record-v1", &technical)?
            != current.technical_record_sha256
            || self.independent && self.owner == "explicit-controller-candidate-acceptance-v1"
        {
            return Err(RuntimeError::Verification);
        }
        let context = resume::Context::capture(self, &current)?;
        self.runtime.store.record_verification_if_current(
            &self.run_id,
            &VerificationRecord {
                verifier: "explicit-controller-candidate-acceptance-v1".into(),
                evidence_sha256: crate::digest("finite-controller-acceptance-v1", &current)?,
                artifact_sha256: Some(current.artifact_sha256.clone()),
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
            Some((RunState::Verified, None)),
            Some(&technical),
        )?;
        self.runtime.store.claim_finite_integration(
            &self.run_id,
            &serde_json::to_string(&current)?,
            &serde_json::to_string(&context)?,
        )?;
        if let Some(observer) = observer {
            observer.before_capture()?;
        }
        // No await between final preimage check and write. Same-uid concurrent
        // writers are outside this exclusively owned private source contract.
        self.source_identity()?;
        if FileIdentity::from(&self.target(false)?.metadata()?) != identity
            || content_digest(&Self::bytes(&mut file)?) != self.preimage
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
        self.source_identity()?;
        if FileIdentity::from(&self.target(false)?.metadata()?) != identity
            || Self::bytes(&mut file)? != candidate
        {
            return Err(RuntimeError::Integrity("finite application result"));
        }
        self.runtime.store.event(
            &self.run_id,
            "integration.applied",
            &current.artifact_sha256,
        )?;
        Ok(())
    }
}
fn oid(bytes: Vec<u8>) -> Result<String> {
    let value = String::from_utf8(bytes).map_err(|_| RuntimeError::Scope)?;
    let value = value.trim();
    if value.len() != 40 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(RuntimeError::Scope);
    }
    Ok(value.into())
}

#[cfg(test)]
mod tests;
