// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Apply a verified patch in place. Interrupted intents are never replayed.
use crate::{PendingRun, Result, Runtime, RuntimeError, content_digest, workspace};
use router_types::IntegrationPreview;
use std::path::Path;

impl Runtime {
    pub async fn apply(
        &self,
        pending: &PendingRun,
        approved_digest: &str,
    ) -> Result<IntegrationPreview> {
        self.apply_observed(pending, approved_digest, None).await
    }
    async fn apply_observed(
        &self,
        pending: &PendingRun,
        approved_digest: &str,
        observer: Option<&dyn workspace::CaptureObserver>,
    ) -> Result<IntegrationPreview> {
        self.require_integration_authority(pending)?;
        let preview = self.integration_preview(pending).await?;
        if preview.approval_digest != approved_digest {
            return Err(RuntimeError::Approval);
        }
        let run = self.store.get(&pending.run.run_id)?;
        let path = run
            .worktree
            .as_ref()
            .ok_or(RuntimeError::Invalid("no worktree"))?;
        let (bytes, _) = workspace::patch(
            Path::new(path),
            &pending.request.scope,
            &pending.request.source_state,
        )
        .await?;
        if content_digest(&bytes) != preview.artifact_sha256 {
            return Err(RuntimeError::Integrity("artifact changed"));
        }
        workspace::safe_scope(
            Path::new(&pending.request.workspace),
            &preview.changed_paths,
        )?;
        workspace::git(
            Path::new(&pending.request.workspace),
            &["apply", "--check", "--binary", "-"],
            bytes.clone(),
        )
        .await?;
        self.store
            .claim_integration(&run.run_id, &preview.approval_digest)?;
        workspace::git_observed(
            Path::new(&pending.request.workspace),
            &["apply", "--binary", "-"],
            bytes,
            observer,
        )
        .await?;
        self.store
            .event(&run.run_id, "integration.applied", &preview.artifact_sha256)?;
        Ok(preview)
    }
}

#[cfg(test)]
mod tests;
