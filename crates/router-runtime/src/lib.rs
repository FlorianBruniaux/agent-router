#![forbid(unsafe_code)]
//! Durable, approval-bound supervision. JSON receipts are never authority.
mod bubblewrap;
mod error;
mod outputs;
mod process;
mod resources;
mod sandbox;
mod store;
mod workflow;
mod workspace;
pub use bubblewrap::BubblewrapSandbox;
pub use error::{Result, RuntimeError};
pub use process::Cancellation;
pub use resources::{ResourceLimits, ResourceSupervisor};
pub use sandbox::{DenySandbox, SandboxPolicy, SandboxSupervisor, SeatbeltSandbox};
pub use store::{observe, observe_events, request_cancel};
pub use workspace::source_state;

use router_adapter_api::{
    HarnessAdapter, SnapshotValidation, ValidatedHostObservation, authorize_execution,
    content_digest, execution_authorization_digest, is_digest,
};
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
    plan: WorkflowPlan,
    node: WorkflowNode,
    receipt: PhaseReceipt,
    workflow: Option<WorkflowExecutionContract>,
}
pub struct ApprovedRun {
    pending: PendingRun,
}
impl PendingRun {
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
            global: Arc::new(Semaphore::new(limits.global_concurrency)),
            limits,
            providers: Mutex::new(BTreeMap::new()),
            agents: Mutex::new(BTreeMap::new()),
            plans: Mutex::new(BTreeMap::new()),
        })
    }
    pub fn summary(&self, id: &str) -> Result<RunSummary> {
        self.store.get(id)
    }
    pub fn events(&self) -> Result<Vec<RuntimeEvent>> {
        self.store.events()
    }
    pub fn verify_integrity(&self) -> Result<()> {
        self.store.verify_events()
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
    fn definition_hash(
        &self,
        request: &ExecutionRequest,
        receipt: &PhaseReceipt,
        plan: &WorkflowPlan,
        node_id: &str,
        workflow: Option<&WorkflowRunBinding>,
    ) -> Result<String> {
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
        if selection.context_hash() != context.context_hash() {
            return Err(RuntimeError::Route);
        }
        let receipt = selection.receipt().clone();
        let workflow_binding =
            workflow::validate(workflow, &plan, &node, &request, context, &receipt)?;
        let selected = receipt.selected.as_ref().ok_or(RuntimeError::Route)?;
        router
            .validate_selection(context, selected)
            .map_err(|_| RuntimeError::Route)?;
        validate_binding(
            &request,
            &selected.candidate,
            &receipt,
            &node,
            context,
            &router
                .execution_capabilities(context, selected)
                .map_err(|_| RuntimeError::Route)?,
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
        )?;
        let approval_digest = digest(
            "runtime-approval-v1",
            &(run_id, &definition_hash, &request.source_state),
        )?;
        let summary = RunSummary {
            run_id: run_id.into(),
            definition_hash,
            approval_digest,
            receipt_id: receipt.receipt_id.clone(),
            plan_id: plan.plan_id.clone(),
            node_id: node_id.into(),
            state: RunState::Planned,
            attempts: 0,
            failure: None,
            worktree: None,
            artifact_sha256: None,
            output_retention: self.limits.output_retention,
            workflow: workflow_binding,
        };
        let mut run = self.store.register_workflow(
            &summary,
            workflow.map(|(contract, _)| contract),
            &request,
            self.limits.max_attempts,
        )?;
        if run.state == RunState::Planned {
            run = self
                .store
                .transition(run_id, RunState::AwaitingApproval, "reviewed", None)?;
        }
        Ok(PendingRun {
            run,
            request,
            context: context.clone(),
            plan,
            node,
            receipt,
            workflow: workflow.map(|(contract, _)| contract.clone()),
        })
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
        if selection.context_hash() != context.context_hash() {
            return Err(RuntimeError::Route);
        }
        let receipt = selection.receipt().clone();
        let binding = workflow::validate(workflow, &plan, &node, &request, context, &receipt)?;
        let selected = receipt.selected.as_ref().ok_or(RuntimeError::Route)?;
        router
            .validate_selection(context, selected)
            .map_err(|_| RuntimeError::Route)?;
        validate_binding(
            &request,
            &selected.candidate,
            &receipt,
            &node,
            context,
            &router
                .execution_capabilities(context, selected)
                .map_err(|_| RuntimeError::Route)?,
        )?;
        let definition_hash =
            self.definition_hash(&request, &receipt, &plan, node_id, binding.as_ref())?;
        if definition_hash != run.definition_hash || binding != run.workflow {
            return Err(RuntimeError::Integrity("definition changed"));
        }
        Ok(PendingRun {
            run,
            request,
            context: context.clone(),
            plan,
            node,
            receipt,
            workflow: workflow.map(|(contract, _)| contract.clone()),
        })
    }
    /// Retry is explicit, requires a new approval call and never repeats writes.
    pub fn retry(&self, mut pending: PendingRun) -> Result<PendingRun> {
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
        let pending = approved.pending;
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
        self.store
            .transition_queued(id, RunState::Preparing, "capacity_acquired", None)?;
        let prepared = self.prepare(&pending, adapter, supervisor).await;
        let (run, command) = match prepared {
            Ok(v) => v,
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
        self.store.attempt(id, self.limits.max_attempts)?;
        self.store
            .transition(id, RunState::Running, "spawn_requested", None)?;
        let process = process::capture(
            command,
            run.stdin.as_bytes().to_vec(),
            run.timeout_ms,
            run.max_output_bytes,
            self.limits.termination_grace_ms,
            &cancel,
        );
        tokio::pin!(process);
        let captured = loop {
            tokio::select! {
                result=&mut process=>break result,
                _=tokio::time::sleep(std::time::Duration::from_millis(20))=>{if self.store.cancel_requested(id)?{cancel.cancel();}}
            }
        };
        let output = match captured {
            Ok(v) => v,
            Err(error) => {
                self.store
                    .finish_attempt(id, "spawn_or_supervision_failed")?;
                self.store.transition(
                    id,
                    RunState::Failed,
                    "spawn_or_supervision_failed",
                    Some(RunFailure::Spawn),
                )?;
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
                _ => (RunState::Failed, Some(RunFailure::Crash)),
            }
        };
        self.store
            .finish_attempt(id, &format!("{:?}", output.outcome.termination))?;
        self.store
            .transition(id, state, "transport_observed", failure)?;
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
    ) -> Result<(PreparedRun, tokio::process::Command)> {
        if !supervisor.resource_limits_enforced() {
            return Err(RuntimeError::Sandbox);
        }
        let root = Path::new(&pending.request.workspace);
        if source_state(root).await? != pending.request.source_state {
            return Err(RuntimeError::SourceChanged);
        }
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
        supervisor.command(
            &initial,
            &SandboxPolicy {
                network: request.required_capabilities.contains(&Capability::Network),
            },
        )?;
        if request.sandbox == ExecutionSandbox::WorkspaceWrite {
            self.store
                .writer_lock(id, &request.workspace, &request.scope)?;
            let path = workspace::create(root, &self.store.root, id, &request.source_state).await?;
            let path = path
                .to_str()
                .ok_or(RuntimeError::Invalid("utf8 worktree"))?;
            self.store.worktree(id, Some(path))?;
            request.workspace = path.into();
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
        let command = supervisor.command(
            &run,
            &SandboxPolicy {
                network: request.required_capabilities.contains(&Capability::Network),
            },
        )?;
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
            pending.request.host.as_str(),
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
        if pending
            .receipt
            .selected
            .as_ref()
            .is_some_and(|s| s.candidate.verification == PhaseVerification::Independent)
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
        || request.receipt_id != receipt.receipt_id
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
        || candidate.estimated_cost_microusd > node.budget.max_cost_microusd
        || candidate.estimated_tokens > node.budget.max_tokens
        || candidate.estimated_elapsed_ms > node.budget.max_elapsed_ms
        || request.timeout_ms > context.request().task.budget.max_elapsed_ms
        || (node.mode == NodeMode::IsolatedWrite) != (sandbox == ExecutionSandbox::WorkspaceWrite)
    {
        return Err(RuntimeError::Route);
    }
    Ok(())
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
