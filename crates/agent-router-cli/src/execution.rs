use crate::{
    CliError, Outcome,
    command::{ArtifactCommand, PhaseArgs},
    phases,
    product::json_output,
};
use router_adapter_api::{HarnessAdapter, HostObservation, content_digest};
use router_policy::{PhaseRouter, ValidatedPhaseContext, ValidatedPhaseReceipt};
use router_runtime::{
    Cancellation, PendingRun, ResourceLimits, ResourceSupervisor, Runtime, RuntimeError,
    RuntimeLimits, SandboxSupervisor,
};
use router_types::{ExecutionRequest, WorkflowPlan};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    process::ExitCode,
};

const DOCUMENT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    schema_version: u32,
    run_id: String,
    context: PathBuf,
    host_snapshot: PathBuf,
    catalog: PathBuf,
    policy: Option<PathBuf>,
    policy_store: Option<PathBuf>,
    policy_artifact_id: Option<String>,
    plan: PathBuf,
    node_id: String,
    request: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workflow_contract: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation: Option<u32>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    cpu_seconds: u64,
    #[serde(deserialize_with = "explicit_memory_limit")]
    address_space_bytes: Option<u64>,
    max_open_files: u64,
    max_processes: u64,
    max_file_bytes: u64,
}
fn explicit_memory_limit<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    Option::<u64>::deserialize(deserializer)
}
impl From<Limits> for ResourceLimits {
    fn from(value: Limits) -> Self {
        Self {
            cpu_seconds: value.cpu_seconds,
            address_space_bytes: value.address_space_bytes,
            max_open_files: value.max_open_files,
            max_processes: value.max_processes,
            max_file_bytes: value.max_file_bytes,
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SandboxConfig {
    schema_version: u32,
    programs: BTreeMap<String, PathBuf>,
    read_roots: Vec<PathBuf>,
    environment_names: Vec<String>,
    helper: PathBuf,
    bubblewrap: Option<PathBuf>,
    limits: Limits,
    feedback_store: Option<PathBuf>,
    #[serde(default)]
    output_retention: router_types::OutputRetentionPolicy,
}

fn error(error: RuntimeError) -> CliError {
    CliError::Product(match error {
        RuntimeError::Invalid(_) => "runtime.invalid",
        RuntimeError::Io(_) => "runtime.io",
        RuntimeError::Storage(_) => "runtime.storage",
        RuntimeError::Json(_) => "runtime.json",
        RuntimeError::Integrity(_) => "runtime.integrity",
        RuntimeError::Locked => "runtime.locked",
        RuntimeError::Transition => "runtime.transition.invalid",
        RuntimeError::NotFound => "runtime.not_found",
        RuntimeError::Approval => "runtime.approval.required_or_changed",
        RuntimeError::SourceChanged => "runtime.source.changed_or_dirty",
        RuntimeError::Scope => "runtime.scope.denied",
        RuntimeError::Sandbox => "runtime.sandbox.unknown_or_unsafe",
        RuntimeError::ResourceUnsupported(_) => "runtime.resource.unsupported",
        RuntimeError::Route => "runtime.route.invalid",
        RuntimeError::Adapter(_) => "runtime.adapter",
        RuntimeError::Process => "runtime.process.failed",
        RuntimeError::Limit => "runtime.limit",
        RuntimeError::Cancelled => "runtime.cancelled",
        RuntimeError::Verification => "runtime.verification.required",
        RuntimeError::DependenciesPending => "runtime.dependencies.pending",
    })
}
fn block<T>(future: impl Future<Output = Result<T, CliError>>) -> Result<T, CliError> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| CliError::Internal)?
        .block_on(future)
}
fn require_json(json: bool) -> Result<(), CliError> {
    if json {
        Ok(())
    } else {
        Err(CliError::JsonOutputRequired)
    }
}
fn resolve(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}
fn load_definition(path: &Path) -> Result<Definition, CliError> {
    let mut value: Definition = phases::read_json(path, 64 * 1024)?;
    if value.schema_version != 1 {
        return Err(CliError::Product("runtime.definition.schema"));
    }
    let absolute = std::fs::canonicalize(path).map_err(|_| CliError::RequestRead)?;
    let base = absolute.parent().ok_or(CliError::RequestRead)?;
    value.context = resolve(base, value.context);
    value.host_snapshot = resolve(base, value.host_snapshot);
    value.catalog = resolve(base, value.catalog);
    value.policy = value.policy.map(|path| resolve(base, path));
    value.policy_store = value.policy_store.map(|path| resolve(base, path));
    if value.policy_store.is_some() != value.policy_artifact_id.is_some() {
        return Err(CliError::Product("policy.artifact.required"));
    }
    value.plan = resolve(base, value.plan);
    value.request = resolve(base, value.request);
    value.receipt = value.receipt.map(|path| resolve(base, path));
    value.workflow_contract = value.workflow_contract.map(|path| resolve(base, path));
    if value.workflow_contract.is_some() != value.generation.is_some() {
        return Err(CliError::Product("runtime.workflow.generation_required"));
    }
    Ok(value)
}
fn workflow_contract(
    definition: &Definition,
) -> Result<Option<(router_types::WorkflowExecutionContract, u32)>, CliError> {
    definition
        .workflow_contract
        .as_ref()
        .map(|path| {
            Ok((
                phases::read_json(path, DOCUMENT_LIMIT)?,
                definition
                    .generation
                    .ok_or(CliError::Product("runtime.workflow.generation_required"))?,
            ))
        })
        .transpose()
}
fn selection(
    definition: &Definition,
    router: &PhaseRouter,
    context: &ValidatedPhaseContext,
) -> Result<ValidatedPhaseReceipt, CliError> {
    let receipt = match &definition.receipt {
        Some(path) => phases::read_json(path, DOCUMENT_LIMIT)?,
        None => router
            .route(context)
            .map_err(|_| CliError::Product("runtime.route.invalid"))?,
    };
    router
        .validate_receipt(context, &receipt)
        .map_err(|_| CliError::Product("runtime.receipt.invalid"))
}
pub(crate) fn plan(path: &Path) -> Result<WorkflowPlan, CliError> {
    let value: serde_json::Value = phases::read_json(path, DOCUMENT_LIMIT)?;
    let result = if value.get("plan").is_some() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            schema_version: u32,
            plan: WorkflowPlan,
            assumptions: Vec<String>,
        }
        let wrapper: Envelope =
            serde_json::from_value(value).map_err(|_| CliError::RequestJsonInvalid)?;
        if wrapper.schema_version != 1 || wrapper.assumptions.len() > 128 {
            return Err(CliError::Product("workflow.invalid"));
        }
        wrapper.plan
    } else {
        serde_json::from_value(value).map_err(|_| CliError::RequestJsonInvalid)?
    };
    router_workflow::validate_plan(&result).map_err(|_| CliError::Product("workflow.invalid"))?;
    Ok(result)
}
fn context(
    definition: &Definition,
    historical: bool,
) -> Result<(PhaseRouter, ValidatedPhaseContext), CliError> {
    // Historical reconstruction can only feed Runtime::reopen, whose stored
    // immutable definition must match. It is never fresh execution authority.
    let now = if historical {
        let snapshot: HostObservation =
            phases::read_json(&definition.host_snapshot, DOCUMENT_LIMIT)?;
        Some(snapshot.observed_at_unix_seconds)
    } else {
        None
    };
    let pinned = if definition.policy.is_none() {
        if let (Some(path), Some(id)) = (&definition.policy_store, &definition.policy_artifact_id) {
            if !path.join("feedback.sqlite3").is_file() {
                return Err(CliError::Product("policy.store.unavailable"));
            }
            let store = router_feedback::FeedbackStore::open(path)
                .map_err(|_| CliError::Product("policy.store.invalid"))?;
            if !historical
                && store
                    .active_policy()
                    .map_err(|_| CliError::Product("policy.active.invalid"))?
                    .as_ref()
                    .map(|active| &active.artifact_id)
                    != Some(id)
            {
                return Err(CliError::Product("policy.active.changed"));
            }
            Some(
                store
                    .policy(id)
                    .map_err(|_| CliError::Product("policy.artifact.invalid"))?
                    .artifact
                    .policy,
            )
        } else {
            None
        }
    } else {
        None
    };
    phases::load_with_policy(
        &PhaseArgs {
            context: definition.context.clone(),
            host_snapshot: definition.host_snapshot.clone(),
            catalog: definition.catalog.clone(),
            policy: definition.policy.clone(),
            policy_store: None,
            now,
            snapshot_max_age_seconds: if historical { 86400 } else { 300 },
        },
        pinned,
    )
}
fn open_existing(store: &Path, definition: &Definition) -> Result<(Runtime, PendingRun), CliError> {
    // Read first: an artifact/status operation never creates a missing store.
    let existing = router_runtime::observe(store, &definition.run_id).map_err(error)?;
    let (router, context) = context(definition, true)?;
    let proof = selection(definition, &router, &context)?;
    let request = phases::read_json(
        &definition.request,
        router_adapter_api::MAX_EXECUTION_INPUT_BYTES,
    )?;
    let plan = plan(&definition.plan)?;
    let workflow = workflow_contract(definition)?;
    let runtime = Runtime::open(
        store,
        RuntimeLimits {
            output_retention: existing.output_retention,
            ..RuntimeLimits::default()
        },
    )
    .map_err(error)?;
    let pending = runtime
        .reopen_selected(
            &definition.run_id,
            &router,
            &context,
            &proof,
            request,
            plan,
            &definition.node_id,
            workflow
                .as_ref()
                .map(|(contract, generation)| (contract, *generation)),
        )
        .map_err(error)?;
    Ok((runtime, pending))
}
fn adapter(host: &str) -> Result<Box<dyn HarnessAdapter>, CliError> {
    match host {
        #[cfg(feature = "codex")]
        "codex" => Ok(Box::new(adapter_codex::CodexAdapter)),
        #[cfg(feature = "claude")]
        "claude-code" => Ok(Box::new(adapter_claude::ClaudeAdapter)),
        _ => Err(CliError::Product("runtime.host.unavailable")),
    }
}
fn sandbox(path: &Path) -> Result<SandboxConfig, CliError> {
    let config: SandboxConfig = phases::read_json(path, 64 * 1024)?;
    if config.schema_version != 1
        || config.programs.is_empty()
        || config.programs.len() > 16
        || config.read_roots.len() > 32
        || config.environment_names.len() > 64
        || !config.helper.is_absolute()
        || config.bubblewrap.as_ref().is_some_and(|p| !p.is_absolute())
        || config.programs.values().any(|p| !p.is_absolute())
        || config.read_roots.iter().any(|p| !p.is_absolute())
        || config.environment_names.iter().any(|v| {
            v.is_empty()
                || v.len() > 128
                || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(CliError::Product("runtime.sandbox.configuration"));
    }
    Ok(config)
}
fn approval(
    pending: &PendingRun,
    config: &SandboxConfig,
    definition: &Definition,
) -> Result<(String, BTreeMap<PathBuf, String>), CliError> {
    let mut hashes = BTreeMap::new();
    for path in config
        .programs
        .values()
        .chain(std::iter::once(&config.helper))
        .chain(config.bubblewrap.iter())
    {
        let bytes = crate::read_bounded(
            path,
            256 * 1024 * 1024,
            CliError::RequestRead,
            CliError::RequestTooLarge,
        )?;
        hashes.insert(path.clone(), content_digest(&bytes));
    }
    #[cfg(target_os = "macos")]
    {
        let wrapper = PathBuf::from("/usr/bin/sandbox-exec");
        let bytes = crate::read_bounded(
            &wrapper,
            256 * 1024 * 1024,
            CliError::RequestRead,
            CliError::RequestTooLarge,
        )?;
        hashes.insert(wrapper, content_digest(&bytes));
    }
    let bytes = serde_json::to_vec(&(
        "runtime-cli-approval-v1",
        pending.approval_digest(),
        definition,
        config,
        &hashes,
    ))
    .map_err(|_| CliError::Internal)?;
    Ok((content_digest(&bytes), hashes))
}
async fn supervisor(
    store: &Path,
    config: SandboxConfig,
    expected: &BTreeMap<PathBuf, String>,
) -> Result<Box<dyn SandboxSupervisor>, CliError> {
    let environment = config
        .environment_names
        .iter()
        .map(|name| {
            std::env::var(name)
                .map(|value| (name.clone(), value))
                .map_err(|_| CliError::Product("runtime.environment.unavailable"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let private_dir = std::fs::canonicalize(store).map_err(|_| CliError::Product("runtime.io"))?;
    #[cfg(target_os = "macos")]
    {
        if config.bubblewrap.is_some() {
            return Err(CliError::Product("runtime.sandbox.configuration"));
        }
        let native = router_runtime::SeatbeltSandbox::probe_expected(
            &private_dir,
            config.programs,
            config.read_roots,
            environment,
            expected,
        )
        .await
        .map_err(error)?;
        Ok(Box::new(
            ResourceSupervisor::probe_expected(
                &config.helper,
                config.limits.into(),
                native,
                expected,
            )
            .await
            .map_err(error)?,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let wrapper = config
            .bubblewrap
            .ok_or(CliError::Product("runtime.sandbox.bubblewrap.required"))?;
        let native = router_runtime::BubblewrapSandbox::probe_expected(
            &private_dir,
            &wrapper,
            config.programs,
            config.read_roots,
            environment,
            expected,
        )
        .await
        .map_err(error)?;
        Ok(Box::new(
            ResourceSupervisor::probe_expected(
                &config.helper,
                config.limits.into(),
                native,
                expected,
            )
            .await
            .map_err(error)?,
        ))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (private_dir, config, environment);
        Err(CliError::Product("runtime.sandbox.unsupported"))
    }
}

pub(crate) fn run(
    definition: &Path,
    store: &Path,
    config: &Path,
    approve: Option<&str>,
    retry: bool,
    json: bool,
) -> Result<Outcome, CliError> {
    require_json(json)?;
    let definition = load_definition(definition)?;
    let config = sandbox(config)?;
    let feedback_path = config.feedback_store.clone();
    let feedback_consent = crate::execution_feedback::consent(feedback_path.as_deref())?;
    block(async {
        let (router, context) = context(&definition, false)?;
        let proof = selection(&definition, &router, &context)?;
        let workflow = workflow_contract(&definition)?;
        let request: ExecutionRequest = phases::read_json(
            &definition.request,
            router_adapter_api::MAX_EXECUTION_INPUT_BYTES,
        )?;
        let host = request.host.as_str().to_owned();
        let display = serde_json::json!({"host": request.host, "model": request.model, "effort": request.effort,
            "scope": request.scope, "sandbox": request.sandbox, "tools": request.tools,
            "prompt_sha256": content_digest(request.prompt.as_bytes()), "resource_limits": config.limits});
        let runtime = Runtime::open(
            store,
            RuntimeLimits {
                output_retention: config.output_retention,
                ..RuntimeLimits::default()
            },
        )
        .map_err(error)?;
        let pending = runtime
            .review_selected(
                &definition.run_id,
                &router,
                &context,
                &proof,
                request,
                plan(&definition.plan)?,
                &definition.node_id,
                workflow
                    .as_ref()
                    .map(|(contract, generation)| (contract, *generation)),
            )
            .await
            .map_err(error)?;
        let pending = if retry {
            runtime.retry(pending).map_err(error)?
        } else {
            pending
        };
        let (digest, binaries) = approval(&pending, &config, &definition)?;
        let Some(approved) = approve else {
            return json_output(
                &serde_json::json!({"schema_version":1,"run":pending.summary(),"approval_digest":digest,
                "request":display,"sandbox_configuration":config,"binary_hashes":binaries,"executed":false}),
                ExitCode::SUCCESS,
            );
        };
        if approved != digest {
            return Err(CliError::Product("runtime.approval.required_or_changed"));
        }
        if !runtime.dependencies_ready(&pending).map_err(error)? {
            // This command owns one step. Release the store owner so a separate
            // invocation can run its prerequisites; do not wait holding the lock.
            let inner_digest = pending.approval_digest().to_owned();
            if pending.summary().state == router_types::RunState::Queued {
                runtime
                    .resume_approved(pending, &inner_digest)
                    .map_err(error)?;
            } else {
                runtime.approve(pending, &inner_digest).map_err(error)?;
            }
            return json_output(
                &serde_json::json!({"schema_version":1,"run":router_runtime::observe(store,&definition.run_id).map_err(error)?,"executed":false,"dependencies_pending":true,"approval_digest":digest}),
                ExitCode::from(3),
            );
        }
        let adapter = adapter(&host)?;
        let supervisor = supervisor(store, config, &binaries).await?;
        let inner_digest = pending.approval_digest().to_owned();
        let approved = if pending.summary().state == router_types::RunState::Queued {
            runtime
                .resume_approved(pending, &inner_digest)
                .map_err(error)?
        } else {
            runtime.approve(pending, &inner_digest).map_err(error)?
        };
        let cancel = Cancellation::default();
        let interrupt = cancel.clone();
        let listener = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                interrupt.cancel();
            }
        });
        let completed = runtime
            .execute(approved, adapter.as_ref(), supervisor.as_ref(), cancel)
            .await;
        listener.abort();
        let completed = completed.map_err(error)?;
        let feedback = crate::execution_feedback::record(
            feedback_path.as_deref(),
            feedback_consent.as_ref(),
            &proof,
            &context,
            &completed,
        );
        let code = if completed.transport.exit_code == Some(0)
            && completed.transport.termination == router_types::TransportTermination::Exited
            && matches!(
                completed.run.state,
                router_types::RunState::SucceededTransport
                    | router_types::RunState::Verifying
                    | router_types::RunState::Verified
            ) {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(3)
        };
        json_output(
            &serde_json::json!({"schema_version":1,"completion":completed,"feedback":feedback}),
            code,
        )
    })
}
pub(crate) fn status(store: &Path, id: &str, json: bool) -> Result<Outcome, CliError> {
    require_json(json)?;
    json_output(
        &serde_json::json!({"schema_version":1,"run":router_runtime::observe(store,id).map_err(error)?}),
        ExitCode::SUCCESS,
    )
}
pub(crate) fn cancel(store: &Path, id: &str, json: bool) -> Result<Outcome, CliError> {
    require_json(json)?;
    router_runtime::request_cancel(store, id).map_err(error)?;
    json_output(
        &serde_json::json!({"schema_version":1,"run_id":id,"cancellation_requested":true}),
        ExitCode::SUCCESS,
    )
}
pub(crate) fn events(store: &Path, id: &str, json: bool) -> Result<Outcome, CliError> {
    require_json(json)?;
    json_output(
        &serde_json::json!({"schema_version":1,"events":router_runtime::observe_events(store,id).map_err(error)?}),
        ExitCode::SUCCESS,
    )
}
pub(crate) fn source_state(workspace: &Path, json: bool) -> Result<Outcome, CliError> {
    require_json(json)?;
    block(async {
        json_output(
            &serde_json::json!({"schema_version":1,"source_state":router_runtime::source_state(workspace).await.map_err(error)?}),
            ExitCode::SUCCESS,
        )
    })
}
pub(crate) fn workflow(
    path: &Path,
    contract: Option<&Path>,
    seal: bool,
    json: bool,
) -> Result<Outcome, CliError> {
    require_json(json)?;
    let plan = plan(path)?;
    let contract = contract
        .map(|path| {
            let contract = phases::read_json(path, DOCUMENT_LIMIT)?;
            if seal {
                router_workflow::seal_execution_contract(contract, &plan)
                    .map_err(|_| CliError::Product("workflow.contract.invalid"))
            } else {
                router_workflow::validate_execution_contract(&contract, &plan)
                    .map_err(|_| CliError::Product("workflow.contract.invalid"))?;
                Ok(contract)
            }
        })
        .transpose()?;
    json_output(
        &serde_json::json!({"schema_version":1,"plan":plan,"execution_contract":contract,"valid":true,"approved":false}),
        ExitCode::SUCCESS,
    )
}
pub(crate) fn verify(
    definition: &Path,
    store: &Path,
    evidence: &Path,
    json: bool,
) -> Result<Outcome, CliError> {
    require_json(json)?;
    let definition = load_definition(definition)?;
    let record = phases::read_json(evidence, 64 * 1024)?;
    let (runtime, pending) = open_existing(store, &definition)?;
    json_output(
        &serde_json::json!({"schema_version":1,"run":runtime.record_verification(&pending,record).map_err(error)?}),
        ExitCode::SUCCESS,
    )
}
pub(crate) fn artifacts(
    definition: &Path,
    store: &Path,
    command: ArtifactCommand,
    json: bool,
) -> Result<Outcome, CliError> {
    require_json(json)?;
    let definition = load_definition(definition)?;
    let (runtime, pending) = open_existing(store, &definition)?;
    block(async {
        let value = match command {
            ArtifactCommand::Outputs => {
                serde_json::json!({"schema_version":1,"outputs":runtime.output_artifacts(&definition.run_id).map_err(error)?})
            }
            ArtifactCommand::PurgeOutputs { attempt } => {
                serde_json::json!({"schema_version":1,"purged":runtime.purge_output_artifacts(&definition.run_id,attempt).map_err(error)?})
            }
            ArtifactCommand::Preview => {
                serde_json::json!({"schema_version":1,"preview":runtime.integration_preview(&pending).await.map_err(error)?})
            }
            ArtifactCommand::Apply { approve } => {
                serde_json::json!({"schema_version":1,"applied":runtime.apply(&pending,&approve).await.map_err(error)?})
            }
            ArtifactCommand::Discard { approve } => {
                runtime.discard(&pending, &approve).await.map_err(error)?;
                serde_json::json!({"schema_version":1,"discarded":true})
            }
            ArtifactCommand::Keep => {
                serde_json::json!({"schema_version":1,"kept":true,"run":pending.summary()})
            }
        };
        json_output(&value, ExitCode::SUCCESS)
    })
}
