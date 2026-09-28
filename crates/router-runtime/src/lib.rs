#![forbid(unsafe_code)]
//! Durable, approval-bound supervision. JSON receipts are never authority.
mod artifact_storage;
mod artifacts;
mod bubblewrap;
mod diagnostics;
mod error;
mod escalation;
mod generated;
mod leases;
mod native_version;
mod outputs;
mod process;
mod resources;
mod sandbox;
mod store;
mod workflow;
mod workspace;
pub use bubblewrap::BubblewrapSandbox;
pub use diagnostics::{
    OwnerLockState, RuntimeLeaseObservations, RuntimeSchemaState, RuntimeStorageCounts,
    RuntimeStorageReport, diagnose_runtime_storage,
};
pub use error::{Result, RuntimeError};
pub use escalation::{EscalationError, EscalationProof, ReviewedAttemptBound};
pub use native_version::{NativeVersionProbe, probe_native_version};
pub use process::Cancellation;
pub use resources::{ResourceLimits, ResourceSupervisor};
pub use sandbox::{DenySandbox, SandboxPolicy, SandboxSupervisor, SeatbeltSandbox};
pub use store::{observe, observe_cleanup, observe_events, request_cancel};
pub use workspace::source_state;

use router_adapter_api::{
    HarnessAdapter, SnapshotValidation, ValidatedHostObservation, authorize_execution,
    content_digest, execution_authorization_digest, is_digest,
};
use router_execution_decision::{ActivationAuthority, ExecutionDecision};
use router_policy::{PhaseRouter, ValidatedPhaseContext, ValidatedPhaseReceipt};
use router_types::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone, Debug)]
pub struct RuntimeLimits {
    pub global_concurrency: usize,
    pub provider_concurrency: usize,
    pub agent_concurrency: usize,
    pub max_attempts: u32,
    pub termination_grace_ms: u64,
    pub snapshot_max_age_seconds: u64,
    pub output_retention: OutputRetentionPolicy,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            global_concurrency: 4,
            provider_concurrency: 2,
            agent_concurrency: 1,
            max_attempts: 2,
            termination_grace_ms: 100,
            snapshot_max_age_seconds: 300,
            output_retention: OutputRetentionPolicy::Disabled,
        }
    }
}
pub struct Runtime {
    store: Store,
    output_store: outputs::OutputStorage,
    artifact_store: artifact_storage::ArtifactStorage,
    limits: RuntimeLimits,
    global: Arc<Semaphore>,
    providers: Mutex<BTreeMap<String, Arc<Semaphore>>>,
    agents: Mutex<BTreeMap<String, Arc<Semaphore>>>,
    plans: Mutex<BTreeMap<String, Arc<Semaphore>>>,
}

/// These handles contain private request material. Neither implements Serialize
/// nor Deserialize, and approval always checks the persisted definition digest.
#[derive(Clone)]
pub struct PendingRun {
    run: RunSummary,
    request: ExecutionRequest,
    context: ValidatedPhaseContext,
    router: PhaseRouter,
    plan: WorkflowPlan,
    node: WorkflowNode,
    receipt: PhaseReceipt,
    decision: ExecutionDecision,
    workflow: Option<WorkflowExecutionContract>,
    artifacts: Option<RunArtifactContract>,
}
pub struct ApprovedRun {
    pending: PendingRun,
}
impl PendingRun {
    pub fn decision(&self) -> &ExecutionDecision {
        &self.decision
    }
    pub fn summary(&self) -> &RunSummary {
        &self.run
    }
    pub fn approval_digest(&self) -> &str {
        &self.run.approval_digest
    }
}
impl ApprovedRun {
    pub fn summary(&self) -> &RunSummary {
        &self.pending.run
    }
}

impl Runtime {
    pub fn open(root: &Path, limits: RuntimeLimits) -> Result<Self> {
        if limits.global_concurrency == 0
            || limits.global_concurrency > 32
            || limits.provider_concurrency == 0
            || limits.provider_concurrency > 32
            || limits.agent_concurrency == 0
            || limits.agent_concurrency > 32
            || limits.max_attempts == 0
            || limits.max_attempts > 5
            || limits.termination_grace_ms > 5_000
            || limits.snapshot_max_age_seconds == 0
            || limits.snapshot_max_age_seconds > 86_400
            || matches!(limits.output_retention,OutputRetentionPolicy::PrivateArtifacts {retention_seconds} if !(1..=31_536_000).contains(&retention_seconds))
        {
            return Err(RuntimeError::Limit);
        }
        let store = Store::open(root)?;
        let output_store = outputs::OutputStorage::open(&store.root)?;
        Ok(Self {
            store,
            output_store,
            artifact_store: artifact_storage::ArtifactStorage::open(root)?,
            global: Arc::new(Semaphore::new(limits.global_concurrency)),
            limits,
            providers: Mutex::new(BTreeMap::new()),
            agents: Mutex::new(BTreeMap::new()),
            plans: Mutex::new(BTreeMap::new()),
        })
    }
    pub fn storage_root(&self) -> &Path {
        &self.store.root
    }
    pub fn summary(&self, id: &str) -> Result<RunSummary> {
        self.store.get(id)
    }
    pub fn verification_record(&self, id: &str) -> Result<Option<VerificationRecord>> {
        self.store.verification_record(id)
    }
    pub fn cleanup_evidence(&self, id: &str) -> Result<ProcessCleanupEvidence> {
        self.store.cleanup_evidence(id)
    }
    pub fn events(&self) -> Result<Vec<RuntimeEvent>> {
        self.store.events()
    }
    pub fn verify_integrity(&self) -> Result<()> {
        self.store.verify_events()
    }
    /// Inspect a coherent in-memory copy of the already owned SQLite connection.
    /// This does not certify schema compatibility, journal authenticity, host
    /// liveness or cleanup; those evidence checks remain separate.
    pub fn diagnose_storage(&self) -> router_doctor::Result<router_doctor::DatabaseSnapshotReport> {
        self.store.diagnose_snapshot()
    }
    /// Returns metadata only; retained bytes must still match their recorded hashes.
    pub fn output_artifacts(&self, id: &str) -> Result<Vec<OutputArtifacts>> {
        let records = self.store.output_artifacts(id)?;
        for metadata in &records {
            self.output_store.verify(
                metadata,
                &self.store.output_file_identities(id, metadata.attempt)?,
            )?;
        }
        Ok(records)
    }
    /// Explicit deletion of one registered attempt. No caller-provided path is accepted.
    pub fn purge_output_artifacts(&self, id: &str, attempt: u32) -> Result<OutputArtifacts> {
        let metadata = self
            .store
            .output_artifacts(id)?
            .into_iter()
            .find(|entry| entry.attempt == attempt)
            .ok_or(RuntimeError::NotFound)?;
        if metadata.state == OutputArtifactsState::Purged {
            return Ok(metadata);
        }
        let identities = self.store.output_file_identities(id, attempt)?;
        self.output_store.verify(&metadata, &identities)?;
        let metadata =
            self.store
                .output_artifacts_state(id, attempt, OutputArtifactsState::Purging)?;
        self.output_store.purge(&metadata, &identities)?;
        self.store
            .output_artifacts_state(id, attempt, OutputArtifactsState::Purged)
    }
    /// Caller-scheduled retention, bounded to 100 attempts per invocation.
    pub fn purge_expired_output_artifacts(
        &self,
        now_unix_seconds: u64,
        limit: usize,
    ) -> Result<usize> {
        let records = self
            .store
            .expired_output_artifacts(now_unix_seconds, limit)?;
        for metadata in &records {
            self.purge_output_artifacts(&metadata.run_id, metadata.attempt)?;
        }
        Ok(records.len())
    }
    #[allow(clippy::too_many_arguments)]
    fn definition_hash(
        &self,
        request: &ExecutionRequest,
        receipt: &PhaseReceipt,
        plan: &WorkflowPlan,
        node_id: &str,
        workflow: Option<&WorkflowRunBinding>,
        artifacts: Option<&RunArtifactContract>,
        decision: Option<&ExecutionDecisionBinding>,
        escalation: Option<&EscalationDecisionBinding>,
        generated: Option<&GeneratedDecisionBinding>,
    ) -> Result<String> {
        if let Some(binding) = generated {
            return digest(
                "runtime-definition-v7-generated",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                    workflow,
                    artifacts,
                    binding,
                ),
            );
        }
        if let Some(binding) = escalation {
            return digest(
                "runtime-definition-v6-escalation",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                    workflow,
                    artifacts,
                    binding,
                ),
            );
        }
        if let Some(binding) = decision {
            return digest(
                "runtime-definition-v5-decision",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                    workflow,
                    artifacts,
                    binding,
                ),
            );
        }
        if let Some(artifacts) = artifacts {
            return digest(
                "runtime-definition-v4-artifacts",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                    workflow,
                    artifacts,
                ),
            );
        }
        if let Some(binding) = workflow {
            return digest(
                "runtime-definition-v3-workflow",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                    binding,
                ),
            );
        }
        if self.limits.output_retention.is_disabled() {
            digest("runtime-definition-v1", &(request, receipt, plan, node_id))
        } else {
            digest(
                "runtime-definition-v2-output-retention",
                &(
                    request,
                    receipt,
                    plan,
                    node_id,
                    self.limits.output_retention,
                ),
            )
        }
    }
    fn validate_pending_retention(&self, pending: &PendingRun) -> Result<()> {
        if pending.run.output_retention != self.limits.output_retention
            || self.definition_hash(
                &pending.request,
                &pending.receipt,
                &pending.plan,
                &pending.node.id,
                pending.run.workflow.as_ref(),
                pending.artifacts.as_ref(),
                pending.decision.binding(),
                pending.decision.escalation_binding(),
                pending.decision.generated_binding(),
            )? != pending.run.definition_hash
        {
            return Err(RuntimeError::Approval);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn review(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
    ) -> Result<PendingRun> {
        let receipt = router.route(context).map_err(|_| RuntimeError::Route)?;
        let selection = router
            .validate_receipt(context, &receipt)
            .map_err(|_| RuntimeError::Route)?;
        self.review_selected(
            run_id, router, context, &selection, request, plan, node_id, None,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn review_selected(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        selection: &ValidatedPhaseReceipt,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
    ) -> Result<PendingRun> {
        self.review_with_artifacts(
            run_id, router, context, selection, request, plan, node_id, workflow, None,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn review_with_artifacts(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        selection: &ValidatedPhaseReceipt,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
        artifacts: Option<&RunArtifactContract>,
    ) -> Result<PendingRun> {
        let decision = ExecutionDecision::deterministic(router, context, selection)
            .map_err(|_| RuntimeError::Route)?;
        self.review_decision(
            run_id, router, context, &decision, request, plan, node_id, workflow, artifacts,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn review_decision(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        decision: &ExecutionDecision,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
        artifacts: Option<&RunArtifactContract>,
    ) -> Result<PendingRun> {
        if context.has_signed_catalog() {
            return Err(RuntimeError::Invalid(
                "signed catalog dispatch authority unavailable",
            ));
        }
        if run_id.is_empty()
            || run_id.len() > 96
            || !run_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || run_id.starts_with('.')
        {
            return Err(RuntimeError::Invalid("run id"));
        }
        router_workflow::validate_plan(&plan)
            .map_err(|_| RuntimeError::Invalid("workflow plan"))?;
        if context.request().task != plan.task {
            return Err(RuntimeError::Route);
        }
        let node = plan
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or(RuntimeError::Invalid("node id"))?
            .clone();
        decision.validate_identity(router, context)?;
        self.validate_escalation(router, context, decision, &node, &request, None, false)?;
        if decision.generated_binding().is_some()
            && workflow.is_some_and(|(_, generation)| generation != 0)
        {
            return Err(RuntimeError::Invalid(
                "generated plan retry requires new instance",
            ));
        }
        generated::validate(
            decision,
            &plan,
            node_id,
            &request,
            workflow.map(|(contract, _)| contract),
            false,
        )?;
        let receipt = decision.baseline().clone();
        let workflow_binding = workflow::validate(
            workflow,
            &plan,
            &node,
            &request,
            context,
            decision.selection(),
        )?;
        let selected = decision.selection();
        router
            .validate_selection(context, selected)
            .map_err(|_| RuntimeError::Route)?;
        validate_binding(
            &request,
            &selected.candidate,
            &receipt,
            decision.authorization_id(),
            &node,
            context,
            &router
                .execution_capabilities(context, selected)
                .map_err(|_| RuntimeError::Route)?,
        )?;
        self.validate_artifact_contract(
            artifacts,
            workflow_binding.as_ref(),
            &plan,
            &node,
            &request,
            true,
        )?;
        let root = Path::new(&request.workspace);
        workspace::safe_scope(root, &request.scope)?;
        if source_state(root).await? != request.source_state {
            return Err(RuntimeError::SourceChanged);
        }
        let snapshot = self.snapshot(context)?;
        // Dry-run validation invokes all adapter API hard gates without asserting
        // execution approval. The original execute request remains hash-bound.
        let mut check = request.clone();
        check.mode = ExecutionMode::DryRun;
        authorize_execution(check, &snapshot, &context.request().restrictions, None)?;
        let definition_hash = self.definition_hash(
            &request,
            &receipt,
            &plan,
            node_id,
            workflow_binding.as_ref(),
            artifacts,
            decision.binding(),
            decision.escalation_binding(),
            decision.generated_binding(),
        )?;
        let approval_digest = digest(
            "runtime-approval-v1",
            &(run_id, &definition_hash, &request.source_state),
        )?;
        let summary = RunSummary {
            run_id: run_id.into(),
            definition_hash,
            approval_digest,
            receipt_id: decision.authorization_id().into(),
            plan_id: plan.plan_id.clone(),
            node_id: node_id.into(),
            state: RunState::Planned,
            attempts: 0,
            failure: None,
            worktree: None,
            artifact_sha256: None,
            output_retention: self.limits.output_retention,
            workflow: workflow_binding,
            artifact_contract_sha256: artifacts
                .map(|a| digest("runtime-artifact-contract-v1", a))
                .transpose()?,
        };
        let mut run = self.store.register_workflow(
            &summary,
            workflow.map(|(contract, _)| contract),
            &request,
            self.limits.max_attempts,
            decision.generated_binding(),
        )?;
        if run.state == RunState::Planned {
            run = self
                .store
                .transition(run_id, RunState::AwaitingApproval, "reviewed", None)?;
        }
        let pending = PendingRun {
            run,
            request,
            context: context.clone(),
            router: router.clone(),
            plan,
            node,
            receipt,
            decision: decision.clone(),
            workflow: workflow.map(|(contract, _)| contract.clone()),
            artifacts: artifacts.cloned(),
        };
        self.claim_escalation(&pending)?;
        Ok(pending)
    }
    pub fn approve(&self, pending: PendingRun, approved_digest: &str) -> Result<ApprovedRun> {
        self.validate_pending_retention(&pending)?;
        let current = self.store.get(&pending.run.run_id)?;
        if approved_digest != current.approval_digest
            || current.definition_hash != pending.run.definition_hash
        {
            return Err(RuntimeError::Approval);
        }
        if current.state != RunState::AwaitingApproval {
            return Err(RuntimeError::Transition);
        }
        self.store
            .transition(&current.run_id, RunState::Queued, "explicit_approval", None)?;
        Ok(ApprovedRun { pending })
    }
    /// Reuses an already persisted approval after restart; never approves a new definition.
    pub fn resume_approved(
        &self,
        pending: PendingRun,
        approved_digest: &str,
    ) -> Result<ApprovedRun> {
        self.validate_pending_retention(&pending)?;
        let current = self.store.get(&pending.run.run_id)?;
        if current.state != RunState::Queued
            || current.definition_hash != pending.run.definition_hash
            || current.approval_digest != approved_digest
        {
            return Err(RuntimeError::Approval);
        }
        Ok(ApprovedRun { pending })
    }
    /// Reconstructs an existing immutable definition for historical verification
    /// or artifact management. This never registers a new run, starts a process,
    /// or treats an old snapshot as current execution authority.
    #[allow(clippy::too_many_arguments)]
    pub fn reopen(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
    ) -> Result<PendingRun> {
        let receipt = router.route(context).map_err(|_| RuntimeError::Route)?;
        let selection = router
            .validate_receipt(context, &receipt)
            .map_err(|_| RuntimeError::Route)?;
        self.reopen_selected(
            run_id, router, context, &selection, request, plan, node_id, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn reopen_selected(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        selection: &ValidatedPhaseReceipt,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
    ) -> Result<PendingRun> {
        self.reopen_with_artifacts(
            run_id, router, context, selection, request, plan, node_id, workflow, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn reopen_with_artifacts(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        selection: &ValidatedPhaseReceipt,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
        artifacts: Option<&RunArtifactContract>,
    ) -> Result<PendingRun> {
        let decision = ExecutionDecision::deterministic(router, context, selection)
            .map_err(|_| RuntimeError::Route)?;
        self.reopen_decision(
            run_id, router, context, &decision, request, plan, node_id, workflow, artifacts,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn reopen_decision(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        decision: &ExecutionDecision,
        request: ExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
        workflow: Option<(&WorkflowExecutionContract, u32)>,
        artifacts: Option<&RunArtifactContract>,
    ) -> Result<PendingRun> {
        if context.has_signed_catalog() {
            return Err(RuntimeError::Invalid(
                "signed catalog dispatch authority unavailable",
            ));
        }
        let run = self.store.get(run_id)?;
        router_workflow::validate_plan(&plan)
            .map_err(|_| RuntimeError::Invalid("workflow plan"))?;
        if context.request().task != plan.task {
            return Err(RuntimeError::Route);
        }
        let node = plan
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or(RuntimeError::Invalid("node id"))?
            .clone();
        decision.validate_identity(router, context)?;
        if decision.generated_binding().is_some()
            && workflow.is_some_and(|(_, generation)| generation != 0)
        {
            return Err(RuntimeError::Invalid(
                "generated plan retry requires new instance",
            ));
        }
        generated::validate(
            decision,
            &plan,
            node_id,
            &request,
            workflow.map(|(contract, _)| contract),
            false,
        )?;
        let receipt = decision.baseline().clone();
        let binding = workflow::validate(
            workflow,
            &plan,
            &node,
            &request,
            context,
            decision.selection(),
        )?;
        let selected = decision.selection();
        router
            .validate_selection(context, selected)
            .map_err(|_| RuntimeError::Route)?;
        validate_binding(
            &request,
            &selected.candidate,
            &receipt,
            decision.authorization_id(),
            &node,
            context,
            &router
                .execution_capabilities(context, selected)
                .map_err(|_| RuntimeError::Route)?,
        )?;
        self.validate_artifact_contract(
            artifacts,
            binding.as_ref(),
            &plan,
            &node,
            &request,
            false,
        )?;
        let definition_hash = self.definition_hash(
            &request,
            &receipt,
            &plan,
            node_id,
            binding.as_ref(),
            artifacts,
            decision.binding(),
            decision.escalation_binding(),
            decision.generated_binding(),
        )?;
        if definition_hash != run.definition_hash || binding != run.workflow {
            return Err(RuntimeError::Integrity("definition changed"));
        }
        let pending = PendingRun {
            run,
            request,
            context: context.clone(),
            router: router.clone(),
            plan,
            node,
            receipt,
            decision: decision.clone(),
            workflow: workflow.map(|(contract, _)| contract.clone()),
            artifacts: artifacts.cloned(),
        };
        self.validate_pending_escalation(&pending, false)?;
        self.validate_pending_generated(&pending, false)?;
        Ok(pending)
    }
    /// Retry is explicit, requires a new approval call and never repeats writes.
    pub fn retry(&self, mut pending: PendingRun) -> Result<PendingRun> {
        if pending.decision.generated_binding().is_some() {
            return Err(RuntimeError::Invalid(
                "generated plan requires a new global review",
            ));
        }
        if pending.decision.escalation_binding().is_some() {
            return Err(RuntimeError::Invalid(
                "escalated attempt requires new accounted decision",
            ));
        }
        self.validate_pending_retention(&pending)?;
        self.store
            .retry(&pending.run.run_id, self.limits.max_attempts)?;
        pending.run = self.store.get(&pending.run.run_id)?;
        Ok(pending)
    }
    pub fn cancel_queued(&self, id: &str) -> Result<RunSummary> {
        self.store
            .transition(id, RunState::Cancelled, "explicit_cancel", None)
    }
    fn snapshot(&self, context: &ValidatedPhaseContext) -> Result<ValidatedHostObservation> {
        ValidatedHostObservation::from_observation(
            context.snapshot().clone(),
            SnapshotValidation {
                now_unix_seconds: now()?,
                max_age_seconds: self.limits.snapshot_max_age_seconds,
            },
        )
        .map_err(|_| RuntimeError::Route)
    }
    pub async fn execute(
        &self,
        approved: ApprovedRun,
        adapter: &dyn HarnessAdapter,
        supervisor: &dyn SandboxSupervisor,
        cancel: Cancellation,
    ) -> Result<RunCompletion> {
        self.execute_decision(approved, adapter, supervisor, cancel, None)
            .await
    }
    pub async fn execute_decision(
        &self,
        approved: ApprovedRun,
        adapter: &dyn HarnessAdapter,
        supervisor: &dyn SandboxSupervisor,
        cancel: Cancellation,
        authority: Option<&dyn ActivationAuthority>,
    ) -> Result<RunCompletion> {
        // Routing proof is not current catalog execution authority. Keep all
        // legacy execution entrances closed until a combined dispatch guard is
        // acquired and retained through spawn by the signed-catalog integration.
        if approved.pending.context.has_signed_catalog() {
            return Err(RuntimeError::Invalid(
                "signed catalog dispatch authority unavailable",
            ));
        }
        let pending = approved.pending;
        self.store.require_cleanup_complete()?;
        self.validate_pending_retention(&pending)?;
        let id = &pending.run.run_id;
        let state = self.store.get(id)?.state;
        if state == RunState::Cancelled {
            return Err(RuntimeError::Cancelled);
        }
        if state != RunState::Queued {
            return Err(RuntimeError::Transition);
        }
        self.wait_dependencies(&pending, &cancel).await?;
        let acquired = self.acquire(&pending, &cancel).await;
        let _permits = match acquired {
            Ok(p) => p,
            Err(RuntimeError::Cancelled) => {
                self.store.transition_queued(
                    id,
                    RunState::Cancelled,
                    "cancelled_while_queued",
                    None,
                )?;
                return Err(RuntimeError::Cancelled);
            }
            Err(e) => return Err(e),
        };
        if !self.dependencies_ready(&pending)? {
            return Err(RuntimeError::DependenciesPending);
        }
        let preparation = self.store.begin_preparation(id)?;
        preparation.no_child_yet()?;
        // Keep polling durable controls while preparation runs. In particular,
        // never drop a version probe future before its helper proves cleanup.
        let preparing = async {
            self.validate_pending_escalation(&pending, true)?;
            self.validate_pending_generated(&pending, true)?;
            drop(pending.decision.acquire_dispatch(
                &pending.router,
                &pending.context,
                authority,
                now()?,
            )?);
            self.prepare(&pending, adapter, supervisor, &cancel, &preparation)
                .await
        };
        tokio::pin!(preparing);
        let mut control_error = None;
        let prepared = loop {
            tokio::select! {
                result = &mut preparing => break result,
                _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                    match self.store.cancel_requested(id) {
                        Ok(true) => cancel.cancel(),
                        Ok(false) => {},
                        Err(error) => {
                            cancel.cancel();
                            control_error.get_or_insert(error);
                        }
                    }
                }
            }
        };
        let prepared = match control_error {
            _ if matches!(prepared, Err(RuntimeError::CleanupUnknown)) => prepared,
            Some(error) => Err(error),
            None => {
                if self.store.cancel_requested(id)? {
                    cancel.cancel();
                }
                if cancel.is_cancelled() {
                    Err(RuntimeError::Cancelled)
                } else {
                    prepared
                }
            }
        };
        let (run, mut command) = match prepared {
            Ok(v) => v,
            Err(RuntimeError::Cancelled) => {
                self.store.transition(
                    id,
                    RunState::Cancelled,
                    "cancelled_while_preparing",
                    None,
                )?;
                return Err(RuntimeError::Cancelled);
            }
            Err(error) => {
                self.store.transition(
                    id,
                    RunState::Blocked,
                    "preparation_rejected",
                    Some(RunFailure::Policy),
                )?;
                return Err(error);
            }
        };
        if let Err(error) = self
            .validate_pending_escalation(&pending, true)
            .and_then(|()| self.validate_pending_generated(&pending, true))
        {
            self.store.transition(
                id,
                RunState::Blocked,
                "escalation_changed_before_dispatch",
                Some(RunFailure::Policy),
            )?;
            return Err(error);
        }
        let dispatch = match pending.decision.acquire_dispatch(
            &pending.router,
            &pending.context,
            authority,
            now()?,
        ) {
            Ok(guard) => guard,
            Err(error) => {
                self.store.transition(
                    id,
                    RunState::Blocked,
                    "decision_changed_before_dispatch",
                    Some(RunFailure::Policy),
                )?;
                return Err(error.into());
            }
        };
        // Acquiring current backend authority may take time. Recheck controls
        // after it, before reserving an attempt or starting a helper.
        if self.store.cancel_requested(id)? {
            cancel.cancel();
        }
        if cancel.is_cancelled() {
            self.store
                .transition(id, RunState::Cancelled, "cancelled_before_attempt", None)?;
            return Err(RuntimeError::Cancelled);
        }
        if let Some(path) = self.store.attempt(
            id,
            self.limits.max_attempts,
            supervisor.parent_lifetime_enforced(),
        )? {
            command.env("ROUTER_RUNTIME_LEASE", path);
        }
        if self.store.cancel_requested(id)? {
            cancel.cancel();
        }
        if cancel.is_cancelled() {
            self.store.confirm_attempt_cleanup(id)?;
            self.store.finish_attempt(id, "cancelled_before_spawn")?;
            self.store
                .transition(id, RunState::Cancelled, "cancelled_before_spawn", None)?;
            return Err(RuntimeError::Cancelled);
        }
        self.store
            .transition(id, RunState::Running, "spawn_requested", None)?;
        if self.store.cancel_requested(id)? {
            cancel.cancel();
        }
        let process = process::capture_guarded(
            command,
            run.stdin.as_bytes().to_vec(),
            run.timeout_ms,
            run.max_output_bytes,
            self.limits.termination_grace_ms,
            &cancel,
            dispatch,
        );
        tokio::pin!(process);
        let captured = loop {
            tokio::select! {
                result=&mut process=>break result,
                _=tokio::time::sleep(std::time::Duration::from_millis(20))=>{if self.store.cancel_requested(id)?{cancel.cancel();}}
            }
        };
        // A watchdog's exit or killpg observation is not a descendant-cleanup
        // proof. Keep authority quarantined if its durable claim is absent.
        if let Err(error) = self.store.confirm_attempt_cleanup(id) {
            self.store.finish_attempt(id, "cleanup_unknown")?;
            self.store.transition(
                id,
                RunState::Failed,
                "cleanup_unknown_authority_retained",
                Some(RunFailure::Interrupted),
            )?;
            return Err(error);
        }
        let output = match captured {
            Ok(v) => v,
            Err(RuntimeError::Cancelled) => {
                self.store.finish_attempt(id, "cancelled_before_spawn")?;
                self.store
                    .transition(id, RunState::Cancelled, "cancelled_before_spawn", None)?;
                return Err(RuntimeError::Cancelled);
            }
            Err(error) => {
                // Expiry can cross its exclusive boundary between lease creation
                // and the final synchronous spawn gate. No process was started.
                let (reason, failure) = if matches!(error, RuntimeError::Decision(_)) {
                    ("decision_rejected_at_dispatch", RunFailure::Policy)
                } else {
                    ("spawn_or_supervision_failed", RunFailure::Spawn)
                };
                self.store.finish_attempt(id, reason)?;
                self.store
                    .transition(id, RunState::Failed, reason, Some(failure))?;
                return Err(error);
            }
        };
        let (state, failure) = if output.output_limited {
            (RunState::Failed, Some(RunFailure::OutputLimit))
        } else {
            match output.outcome.termination {
                TransportTermination::Cancelled => (RunState::Cancelled, None),
                TransportTermination::TimedOut => (RunState::TimedOut, Some(RunFailure::Timeout)),
                TransportTermination::Exited if output.outcome.exit_code == Some(0) => {
                    (RunState::SucceededTransport, None)
                }
                TransportTermination::Exited if output.outcome.exit_code.is_some() => {
                    let native = adapter
                        .classify_failure(
                            &output.stdout,
                            output.outcome.clone(),
                            run.max_output_bytes,
                        )
                        .ok()
                        .flatten();
                    let failure = if let Some(evidence) = native {
                        self.store.event(
                            id,
                            "failure.native_classified",
                            &format!("{}:{}", evidence.code(), evidence.source_sha256()),
                        )?;
                        match evidence.kind() {
                            router_adapter_api::NativeFailureKind::Authentication => {
                                RunFailure::Authentication
                            }
                            router_adapter_api::NativeFailureKind::RateLimit => {
                                RunFailure::RateLimit
                            }
                            router_adapter_api::NativeFailureKind::Network => RunFailure::Network,
                        }
                    } else {
                        RunFailure::Crash
                    };
                    (RunState::Failed, Some(failure))
                }
                _ => (RunState::Failed, Some(RunFailure::Crash)),
            }
        };
        self.store.observed_attempt(AttemptEvidence {
            schema_version: 1,
            identity: AttemptIdentity {
                run_id: id.into(),
                attempt: self.store.get(id)?.attempts,
            },
            definition_hash: pending.run.definition_hash.clone(),
            authorization_id: pending.decision.authorization_id().into(),
            decision_origin: pending.decision.origin(),
            sandbox: pending.request.sandbox,
            context_hash: pending.context.context_hash().into(),
            pool_hash: pending.context.pool_hash().into(),
            source_state: pending.request.source_state.clone(),
            task_execution_sha256: escalation::task_execution_hash(&pending.request)?,
            candidate_id: pending.request.candidate_id.clone(),
            model: pending.request.model.clone(),
            effort: pending.request.effort,
            workflow: pending.run.workflow.clone(),
            state,
            failure,
            transport: output.outcome.clone(),
            cleanup: ProcessCleanupEvidence::Unknown,
            observed_cost_microusd: None,
            observed_tokens: None,
            started_event_hash: String::new(),
            finished_event_hash: String::new(),
            transition_event_hash: String::new(),
        })?;
        let output_artifacts =
            if let OutputRetentionPolicy::PrivateArtifacts { retention_seconds } =
                self.limits.output_retention
            {
                let metadata = self.output_store.metadata(
                    id,
                    self.store.get(id)?.attempts,
                    now()?,
                    retention_seconds,
                    (&output.stdout, &output.stderr),
                    output.output_limited,
                )?;
                self.store.reserve_output_artifacts(&metadata)?;
                self.output_store.write(
                    &metadata,
                    &output.stdout,
                    &output.stderr,
                    |stream, identity| {
                        self.store
                            .register_output_file(id, metadata.attempt, stream, &identity)
                    },
                )?;
                Some(self.store.output_artifacts_state(
                    id,
                    metadata.attempt,
                    OutputArtifactsState::Retained,
                )?)
            } else {
                None
            };
        if state == RunState::SucceededTransport {
            if run.sandbox == ExecutionSandbox::WorkspaceWrite {
                let capture = self.capture_artifact(&pending, Path::new(&run.cwd)).await;
                if let Err(error) = capture {
                    self.store
                        .transition(id, RunState::Verifying, "artifact_check", None)?;
                    self.store.transition(
                        id,
                        RunState::VerificationFailed,
                        "artifact_scope_or_integrity_failed",
                        Some(RunFailure::Verification),
                    )?;
                    return Err(error);
                }
            }
            if let Err(error) =
                self.capture_declared_artifacts(&pending, &output.stdout, &output.outcome, adapter)
            {
                self.store
                    .transition(id, RunState::Verifying, "declared_output_check", None)?;
                self.store.transition(
                    id,
                    RunState::VerificationFailed,
                    "declared_output_missing_or_invalid",
                    Some(RunFailure::Verification),
                )?;
                return Err(error);
            }
            self.store.transition(
                id,
                RunState::Verifying,
                "independent_verification_required",
                None,
            )?;
        }
        // Native stdout may describe an outcome, but never sets verification.
        let decoded =
            adapter.decode_stream(&output.stdout, output.outcome.clone(), run.max_output_bytes);
        self.store.event(
            id,
            "protocol.observed",
            if decoded.is_ok() {
                "decoded_not_verified"
            } else {
                "unrecognized_not_verified"
            },
        )?;
        let runtime_evidence = decoded.ok().map(|mut evidence| {
            evidence.validation = VerificationOutcome::Unknown;
            evidence.objective = VerificationOutcome::Unknown;
            evidence
        });
        Ok(RunCompletion {
            run: self.store.get(id)?,
            transport: output.outcome,
            stdout_sha256: content_digest(&output.stdout),
            stderr_sha256: content_digest(&output.stderr),
            stdout_bytes: output.stdout.len(),
            stderr_bytes: output.stderr.len(),
            validation: VerificationOutcome::Unknown,
            objective: VerificationOutcome::Unknown,
            memory_enforcement: supervisor.memory_enforcement(),
            runtime_evidence,
            output_artifacts,
        })
    }
    async fn prepare(
        &self,
        pending: &PendingRun,
        adapter: &dyn HarnessAdapter,
        supervisor: &dyn SandboxSupervisor,
        cancel: &Cancellation,
        preparation: &store::Preparation<'_>,
    ) -> Result<(PreparedRun, tokio::process::Command)> {
        if !supervisor.resource_limits_enforced() {
            return Err(RuntimeError::Sandbox);
        }
        if cancel.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        let root = Path::new(&pending.request.workspace);
        if workspace::source_state_observed(root, Some(preparation)).await?
            != pending.request.source_state
        {
            return Err(RuntimeError::SourceChanged);
        }
        if cancel.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        self.validate_artifact_contract(
            pending.artifacts.as_ref(),
            pending.run.workflow.as_ref(),
            &pending.plan,
            &pending.node,
            &pending.request,
            true,
        )?;
        let snapshot = self.snapshot(&pending.context)?;
        let mut request = pending.request.clone();
        let id = &pending.run.run_id;
        // A dry preparation verifies the native adapter/sandbox before creating
        // a worktree. Approval has already been recorded, but no process runs.
        let auth = execution_authorization_digest(
            &request,
            &snapshot,
            &pending.context.request().restrictions,
        )?;
        let validated = authorize_execution(
            request.clone(),
            &snapshot,
            &pending.context.request().restrictions,
            Some(&auth),
        )?;
        let initial = adapter.prepare(&validated)?;
        pending
            .router
            .validate_execution_assets(
                &pending.context,
                &pending.request,
                initial.stdin.as_bytes(),
                now()?,
            )
            .map_err(|_| RuntimeError::Route)?;
        if adapter.requires_native_version() {
            let probe = native_version::probe_native_version_recorded(
                supervisor,
                &initial,
                &self.store.root,
                cancel,
                preparation,
            )
            .await?;
            adapter.validate_native_version(probe.stdout())?;
            self.store
                .event(id, "native.version_validated", &probe.output_sha256)?;
        }
        if cancel.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        supervisor.command(
            &initial,
            &SandboxPolicy {
                network: request.required_capabilities.contains(&Capability::Network),
            },
        )?;
        if request.sandbox == ExecutionSandbox::WorkspaceWrite {
            self.store
                .writer_lock(id, &request.workspace, &request.scope)?;
            let path = workspace::create(
                root,
                &self.store.root,
                id,
                &request.source_state,
                Some(preparation),
            )
            .await?;
            let path = path
                .to_str()
                .ok_or(RuntimeError::Invalid("utf8 worktree"))?;
            self.store.worktree(id, Some(path))?;
            request.workspace = path.into();
        }
        if let Some(artifacts) = &pending.artifacts {
            let patches: Vec<_> = artifacts
                .inputs
                .iter()
                .filter(|a| a.exposure == ArtifactExposure::ReadOnlyWorkspacePatch)
                .collect();
            if !patches.is_empty() {
                let path = workspace::create(
                    root,
                    &self.store.root,
                    id,
                    &request.source_state,
                    Some(preparation),
                )
                .await?;
                self.store
                    .worktree(id, Some(path.to_str().ok_or(RuntimeError::Scope)?))?;
                for grant in patches {
                    let bytes = self.read_declared_artifact(&grant.artifact_id)?;
                    workspace::git_observed(
                        &path,
                        &["apply", "--check", "--binary", "-"],
                        bytes.clone(),
                        Some(preparation),
                    )
                    .await?;
                    workspace::git_observed(
                        &path,
                        &["apply", "--binary", "-"],
                        bytes,
                        Some(preparation),
                    )
                    .await?;
                }
                let (_, paths) = workspace::patch_observed(
                    &path,
                    &request.scope,
                    &request.source_state,
                    Some(preparation),
                )
                .await?;
                workspace::safe_scope(&path, &paths)?;
                request.workspace = path.to_str().ok_or(RuntimeError::Scope)?.into();
                self.store.event(
                    id,
                    "artifacts.readonly_view",
                    &digest("runtime-artifact-contract-v1", artifacts)?,
                )?;
            }
        }
        if cancel.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        workspace::safe_scope(Path::new(&request.workspace), &request.scope)?;
        let auth = execution_authorization_digest(
            &request,
            &snapshot,
            &pending.context.request().restrictions,
        )?;
        let validated = authorize_execution(
            request.clone(),
            &snapshot,
            &pending.context.request().restrictions,
            Some(&auth),
        )?;
        let run = adapter.prepare(&validated)?;
        // Adapters are trusted code but their output is still bound to the core
        // authorization to detect accidental widening or cross-run reuse.
        if run.authorization_digest != auth
            || run.host != request.host
            || run.receipt_id != request.receipt_id
            || run.source_state != request.source_state
            || run.scope != request.scope
            || run.cwd != request.workspace
            || run.sandbox != request.sandbox
            || run.mode != ExecutionMode::Execute
            || run.requested_model != request.model
            || run.requested_effort != request.effort
            || run.snapshot_hash != snapshot.hash()
            || run.timeout_ms != request.timeout_ms
            || run.max_output_bytes != request.max_output_bytes
            || run.parent_model_changed
        {
            return Err(RuntimeError::Integrity("prepared run binding"));
        }
        // The original request remains the authority for workspace/source/scope.
        // Only the approved runtime-created view may replace the adapter cwd;
        // the actual final stdin is revalidated immediately before dispatch.
        if let Some(gated) = pending
            .router
            .validate_execution_assets(
                &pending.context,
                &pending.request,
                run.stdin.as_bytes(),
                now()?,
            )
            .map_err(|_| RuntimeError::Route)?
        {
            self.store
                .event(id, "assets.final_gate", gated.binding_digest())?;
        }
        let command = supervisor.command(
            &run,
            &SandboxPolicy {
                network: request.required_capabilities.contains(&Capability::Network),
            },
        )?;
        preparation.complete()?;
        Ok((run, command))
    }
    async fn capture_artifact(&self, pending: &PendingRun, path: &Path) -> Result<()> {
        let (bytes, _) =
            workspace::patch(path, &pending.request.scope, &pending.request.source_state).await?;
        workspace::write_artifact(&self.store.root, &pending.run.run_id, &bytes)?;
        self.store
            .artifact(&pending.run.run_id, &content_digest(&bytes))
    }
    async fn acquire(
        &self,
        pending: &PendingRun,
        cancel: &Cancellation,
    ) -> Result<Vec<OwnedSemaphorePermit>> {
        let agent = semaphore(
            &self.agents,
            &pending.node.owner,
            self.limits.agent_concurrency,
        )?;
        let provider = semaphore(
            &self.providers,
            &provider_key(&pending.context, &pending.request.candidate_id),
            self.limits.provider_concurrency,
        )?;
        let mut result = Vec::new();
        let count = pending
            .context
            .request()
            .restrictions
            .max_concurrency
            .unwrap_or(self.limits.global_concurrency)
            .min(pending.plan.max_width)
            .min(self.limits.global_concurrency);
        let plan_key = pending
            .run
            .workflow
            .as_ref()
            .map(|w| w.instance_id.as_str())
            .unwrap_or(&pending.run.run_id);
        let plan = semaphore(&self.plans, plan_key, count)?;
        for (sem, reason) in [
            (plan, QueueReason::GlobalCapacity),
            (agent, QueueReason::AgentCapacity),
            (provider, QueueReason::ProviderCapacity),
            (self.global.clone(), QueueReason::GlobalCapacity),
        ] {
            if sem.available_permits() == 0 {
                self.store
                    .event(&pending.run.run_id, "queued", &format!("{reason:?}"))?;
            }
            if self.store.cancel_requested(&pending.run.run_id)? || cancel.is_cancelled() {
                return Err(RuntimeError::Cancelled);
            }
            let permit = tokio::select! {permit=sem.acquire_owned()=>permit.map_err(|_|RuntimeError::Integrity("semaphore closed"))?,_=self.wait_cancel(&pending.run.run_id,cancel)=>return Err(RuntimeError::Cancelled)};
            result.push(permit);
        }
        Ok(result)
    }
    async fn wait_cancel(&self, id: &str, cancel: &Cancellation) -> Result<()> {
        loop {
            if self.store.cancel_requested(id)? {
                return Ok(());
            }
            tokio::select! {_=cancel.cancelled()=>return Ok(()),_=tokio::time::sleep(std::time::Duration::from_millis(20))=>{}}
        }
    }
    /// The caller is the explicit verifier. Provider stdout cannot call this API.
    pub fn record_verification(
        &self,
        pending: &PendingRun,
        record: VerificationRecord,
    ) -> Result<RunSummary> {
        let run = self.store.get(&pending.run.run_id)?;
        if run.state != RunState::Verifying
            || !is_digest(&record.evidence_sha256)
            || record.verifier.is_empty()
            || record.verifier.len() > 128
            || record.artifact_sha256 != run.artifact_sha256
        {
            return Err(RuntimeError::Verification);
        }
        if pending.decision.selection().candidate.verification == PhaseVerification::Independent
            && record.verifier == pending.node.owner
        {
            return Err(RuntimeError::Verification);
        }
        self.store.verification(&run.run_id, &record)?;
        if record.validation == VerificationOutcome::Passed
            && record.objective == VerificationOutcome::Passed
        {
            self.store.transition(
                &run.run_id,
                RunState::Verified,
                "explicit_verification",
                None,
            )
        } else if record.validation == VerificationOutcome::Failed
            || record.objective == VerificationOutcome::Failed
        {
            self.store.transition(
                &run.run_id,
                RunState::VerificationFailed,
                "explicit_verification",
                Some(RunFailure::Verification),
            )
        } else {
            Ok(run)
        }
    }
    pub async fn integration_preview(&self, pending: &PendingRun) -> Result<IntegrationPreview> {
        let run = self.store.get(&pending.run.run_id)?;
        if run.state != RunState::Verified {
            return Err(RuntimeError::Verification);
        }
        let path = run
            .worktree
            .as_ref()
            .ok_or(RuntimeError::Invalid("no worktree"))?;
        if source_state(Path::new(&pending.request.workspace)).await?
            != pending.request.source_state
        {
            return Err(RuntimeError::SourceChanged);
        }
        let (bytes, paths) = workspace::patch(
            Path::new(path),
            &pending.request.scope,
            &pending.request.source_state,
        )
        .await?;
        let hash = content_digest(&bytes);
        if run.artifact_sha256.as_deref() != Some(&hash) {
            return Err(RuntimeError::Integrity("artifact changed"));
        }
        let approval_digest = workspace::integration_digest(
            &run.run_id,
            &pending.request.source_state,
            &hash,
            &paths,
        )?;
        Ok(IntegrationPreview {
            run_id: run.run_id,
            source_state: pending.request.source_state.clone(),
            artifact_sha256: hash,
            approval_digest,
            changed_paths: paths,
        })
    }
    pub async fn apply(
        &self,
        pending: &PendingRun,
        approved_digest: &str,
    ) -> Result<IntegrationPreview> {
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
        workspace::git(
            Path::new(&pending.request.workspace),
            &["apply", "--binary", "-"],
            bytes,
        )
        .await?;
        self.store
            .event(&run.run_id, "integration.applied", &preview.artifact_sha256)?;
        Ok(preview)
    }
    pub async fn discard(&self, pending: &PendingRun, approved_digest: &str) -> Result<()> {
        let run = self.store.get(&pending.run.run_id)?;
        if !run.state.terminal() || approved_digest != run.approval_digest {
            return Err(RuntimeError::Approval);
        }
        let path = run.worktree.ok_or(RuntimeError::Invalid("no worktree"))?;
        let expected = self.store.root.join("worktrees").join(&run.run_id);
        if Path::new(&path) != expected {
            return Err(RuntimeError::Integrity("worktree path"));
        }
        workspace::git(
            Path::new(&pending.request.workspace),
            &["worktree", "remove", "--force", &path],
            vec![],
        )
        .await?;
        self.store.worktree(&run.run_id, None)?;
        self.store.event(&run.run_id, "integration.discarded", "")
    }
}
fn validate_binding(
    request: &ExecutionRequest,
    candidate: &PhaseCandidate,
    receipt: &PhaseReceipt,
    authorization_id: &str,
    node: &WorkflowNode,
    context: &ValidatedPhaseContext,
    effective_capabilities: &BTreeSet<Capability>,
) -> Result<()> {
    let sandbox = match candidate.sandbox {
        PhaseSandbox::ReadOnly => ExecutionSandbox::ReadOnly,
        PhaseSandbox::WorkspaceWrite => ExecutionSandbox::WorkspaceWrite,
    };
    let tools: BTreeSet<String> = match (&request.tools, candidate.tool_policy) {
        (ToolSelection::Disabled, PhaseToolPolicy::Restricted) => BTreeSet::new(),
        (ToolSelection::AllowList(v), PhaseToolPolicy::Restricted) => v.iter().cloned().collect(),
        (ToolSelection::HostPolicy, PhaseToolPolicy::NativeHost) if candidate.tools.is_empty() => {
            BTreeSet::new()
        }
        _ => return Err(RuntimeError::Route),
    };
    let skills: BTreeSet<_> = request.skills.iter().map(|s| s.id.clone()).collect();
    if request.mode != ExecutionMode::Execute
        || request.receipt_id != authorization_id
        || request.candidate_id != candidate.id
        || request.phase != receipt.phase
        || request.model != candidate.model
        || request.effort != candidate.effort
        || request.agent != candidate.agent
        || skills != candidate.skills
        || tools != candidate.tools
        || request.sandbox != sandbox
        || request.scope != node.scope
        || effective_capabilities != &request.required_capabilities
        || request.timeout_ms > node.budget.max_elapsed_ms
        || request.timeout_ms > context.request().task.budget.max_elapsed_ms
        || candidate.estimated_cost_microusd > node.budget.max_cost_microusd
        || candidate.estimated_tokens > node.budget.max_tokens
        || candidate.estimated_elapsed_ms > node.budget.max_elapsed_ms
        || (node.mode == NodeMode::IsolatedWrite) != (sandbox == ExecutionSandbox::WorkspaceWrite)
    {
        return Err(RuntimeError::Route);
    }
    Ok(())
}
// Provider identity comes from the bound asset contract, never the host name.
// All legacy/unknown providers share one conservative quota bucket.
fn provider_key(context: &ValidatedPhaseContext, candidate_id: &str) -> String {
    context.asset_requirements(candidate_id).map_or_else(
        || "unknown-provider".into(),
        |request| format!("known-provider:{}", request.provider),
    )
}
fn semaphore(
    map: &Mutex<BTreeMap<String, Arc<Semaphore>>>,
    key: &str,
    count: usize,
) -> Result<Arc<Semaphore>> {
    let mut map = map
        .lock()
        .map_err(|_| RuntimeError::Integrity("queue lock"))?;
    if map.len() >= 1024 && !map.contains_key(key) {
        return Err(RuntimeError::Limit);
    }
    Ok(map
        .entry(key.into())
        .or_insert_with(|| Arc::new(Semaphore::new(count)))
        .clone())
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| RuntimeError::Invalid("clock"))
}
fn digest(domain: &str, value: &impl Serialize) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update([0]);
    hash.update(serde_json::to_vec(value)?);
    Ok(hex::encode(hash.finalize()))
}
