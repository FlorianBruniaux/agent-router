//! Durable review binding only. No backend, process or provider authority.
use super::*;
use crate::{ApprovedRun, PendingRun, Runtime};
use router_adapter_api::{
    authorize_execution_with_capability_acceptance,
    execution_authorization_digest_with_capability_acceptance,
};
use router_execution_decision::ExecutionDecision;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_types::{ExecutionRequest, RunState, RunSummary, WorkflowPlan};

/// Owns both reviews without exposing a clonable generic runtime entrance.
/// ```compile_fail
/// use router_runtime::{Runtime, PendingFiniteClientRun};
/// fn bypass(runtime: &Runtime, pending: PendingFiniteClientRun) {
///     runtime.approve(pending, "digest");
/// }
/// ```
pub struct PendingFiniteClientRun {
    pending: PendingRun,
    client: FiniteClientReview,
}
/// The runtime persisted the combined approval. Fresh OS evidence and a
/// dedicated backend are still required; this handle cannot produce a Command.
/// ```compile_fail
/// use router_runtime::{Runtime, ApprovedFiniteClientRun, SandboxSupervisor, Cancellation};
/// use router_adapter_api::HarnessAdapter;
/// async fn bypass(runtime: &Runtime, approved: ApprovedFiniteClientRun,
///                 adapter: &dyn HarnessAdapter, sandbox: &dyn SandboxSupervisor) {
///     runtime.execute(approved, adapter, sandbox, Cancellation::default()).await;
/// }
/// ```
pub struct ApprovedFiniteClientRun {
    approved: ApprovedRun,
    client: ApprovedFiniteClient,
}
impl PendingFiniteClientRun {
    pub fn summary(&self) -> &RunSummary {
        self.pending.summary()
    }
    pub fn approval_digest(&self) -> &str {
        self.pending.approval_digest()
    }
}
impl ApprovedFiniteClientRun {
    pub fn summary(&self) -> &RunSummary {
        self.approved.summary()
    }
    pub fn proof_binding(&self) -> Result<CapabilityProofBinding> {
        self.client
            .proof_binding(&self.client.reviewed.manifest.host_snapshot_id)
    }
}
pub(crate) fn bind_runtime_definition(base: &str, client: Option<&str>) -> Result<String> {
    match client {
        Some(client) => crate::digest("runtime-finite-client-definition-v1", &(base, client)),
        None => Ok(base.into()), // Existing approvals keep their exact definition.
    }
}
impl Runtime {
    fn check_finite_review(
        &self,
        id: &str,
        context: &ValidatedPhaseContext,
        request: &ExecutionRequest,
        client: &FiniteClientReview,
    ) -> Result<()> {
        let run = &client.run;
        let snapshot = self.snapshot(context)?;
        let auth = execution_authorization_digest_with_capability_acceptance(
            request,
            &snapshot,
            &context.request().restrictions,
            context.capability_acceptance(),
        )?;
        let validated = authorize_execution_with_capability_acceptance(
            request.clone(),
            &snapshot,
            &context.request().restrictions,
            context.capability_acceptance(),
            Some(&auth),
        )?;
        let expected = router_adapter_api::prepared_run(
            &validated,
            "finite-client",
            run.argv.clone(),
            vec![],
        )?;
        // Do not authorize relocation: generated startup state binds this cwd.
        if run.authorization_digest != auth
            || run.snapshot_hash != snapshot.hash()
            || run.receipt_id != expected.receipt_id
            || run.host != expected.host
            || run.source_state != expected.source_state
            || run.scope != expected.scope
            || run.cwd != expected.cwd
            || run.sandbox != expected.sandbox
            || run.mode != expected.mode
            || run.requested_model != expected.requested_model
            || run.requested_effort != expected.requested_effort
            || run.timeout_ms != expected.timeout_ms
            || run.max_output_bytes != expected.max_output_bytes
            || run.stdin.as_bytes() != expected.stdin.as_bytes()
            || client.manifest.task_id != id
            || client.manifest.host_snapshot_id != snapshot.host().id
            || !request.required_capabilities.contains(&Capability::Network)
        {
            return Err(RuntimeError::Integrity("finite client runtime request"));
        }
        prepared_digest(run)?;
        let state = client.owned_state.as_ref().ok_or(RuntimeError::Sandbox)?;
        client.owned_assets.as_ref().ok_or(RuntimeError::Sandbox)?;
        if state.path().parent() != Some(self.store.root.as_path()) {
            return Err(RuntimeError::Integrity("finite client runtime owner"));
        }
        if !client
            .resource_arguments
            .windows(2)
            .any(|args| args == ["--address-space-bytes", "8589934592"])
        {
            return Err(RuntimeError::Integrity("finite client 8 GiB bound"));
        }
        client.check_files()
    }
    /// Registers one immutable definition in the existing store. No worktree,
    /// lease, attempt, admission ticket, native probe or provider turn is made.
    #[allow(clippy::too_many_arguments)]
    pub async fn review_finite_client(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        client: FiniteClientReview,
    ) -> Result<PendingFiniteClientRun> {
        self.check_finite_review(run_id, context, &request, &client)?;
        let receipt = router.route(context).map_err(|_| RuntimeError::Route)?;
        let selection = router
            .validate_receipt(context, &receipt)
            .map_err(|_| RuntimeError::Route)?;
        let decision = ExecutionDecision::deterministic(router, context, &selection)
            .map_err(|_| RuntimeError::Route)?;
        let pending = self
            .review_decision_bound(
                run_id,
                router,
                context,
                &decision,
                request,
                plan,
                node_id,
                None,
                None,
                Some(client.approval_digest()),
            )
            .await?;
        self.store
            .event(run_id, "finite_client.review.v1", client.approval_digest())?;
        Ok(PendingFiniteClientRun { pending, client })
    }
    /// Requires the combined runtime digest, never just the preparation digest.
    pub fn approve_finite_client(
        &self,
        pending: PendingFiniteClientRun,
        digest: &str,
    ) -> Result<ApprovedFiniteClientRun> {
        self.check_finite_review(
            &pending.pending.run.run_id,
            &pending.pending.context,
            &pending.pending.request,
            &pending.client,
        )?;
        let client_digest = pending.client.approval_digest().to_owned();
        let approved = self.approve_pending(pending.pending, digest)?;
        let client = pending.client.approve(&client_digest)?;
        Ok(ApprovedFiniteClientRun { approved, client })
    }
    /// Reopens authenticated evidence under this exact queued runtime owner.
    /// This borrow supplies no process or admission authority. A copied JSON
    /// receipt, historical verifier, revoked proof or cancelled run cannot pass.
    pub fn validate_finite_client<'a>(
        &self,
        approved: &'a ApprovedFiniteClientRun,
        source: &EvidenceSource,
        verifier: &StoredCapabilityProofVerifier,
    ) -> Result<FiniteClientValidation<'a>> {
        let pending = &approved.approved.pending;
        self.validate_pending_retention(pending)?;
        let current = self.store.get(&pending.run.run_id)?;
        if current.state != RunState::Queued
            || current.definition_hash != pending.run.definition_hash
            || current.approval_digest != pending.run.approval_digest
            || pending.finite_client_binding.as_deref()
                != Some(approved.client.reviewed.approval_digest())
        {
            return Err(RuntimeError::Approval);
        }
        self.check_finite_review(
            &pending.run.run_id,
            &pending.context,
            &pending.request,
            &approved.client.reviewed,
        )?;
        let snapshot = self.snapshot(&pending.context)?;
        approved.client.validate(
            &approved.client.reviewed.run,
            snapshot.host(),
            source,
            verifier,
            crate::now()?,
        )
    }
}
