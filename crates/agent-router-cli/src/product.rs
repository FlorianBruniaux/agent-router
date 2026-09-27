use crate::{CliError, Outcome, read_bounded};
use router_retrieval::{Bm25Index, RetrievalError, SearchQuery};
use router_types::{TaskInput, WorkflowPreset};
use router_workflow::{PlanLimits, WorkflowError};
use std::{path::Path, process::ExitCode};

pub(crate) fn json_output(
    value: &impl serde::Serialize,
    exit_code: ExitCode,
) -> Result<Outcome, CliError> {
    Ok(Outcome {
        output: format!(
            "{}\n",
            serde_json::to_string_pretty(value).map_err(|_| CliError::Internal)?
        ),
        exit_code,
    })
}
pub(crate) fn retrieval_error(error: RetrievalError) -> CliError {
    CliError::Product(match error {
        RetrievalError::Invalid(_) => "retrieval.invalid",
        RetrievalError::Limit(_) => "retrieval.limit",
        RetrievalError::Stale => "retrieval.stale",
        RetrievalError::Selection => "retrieval.selection",
        RetrievalError::UnsafePath => "retrieval.path",
        RetrievalError::BodyHashMismatch => "retrieval.integrity",
        RetrievalError::Io { .. } => "retrieval.io",
    })
}
fn workflow_error(error: WorkflowError) -> CliError {
    CliError::Product(match error {
        WorkflowError::Invalid(_) => "workflow.invalid",
        WorkflowError::Limit(_) => "workflow.limit",
        WorkflowError::Authority(_) => "workflow.authority",
        WorkflowError::Integrity => "workflow.integrity",
        WorkflowError::Cost { .. } => "workflow.cost.exceeded",
    })
}
pub(crate) fn configuration(path: &Path, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let layers = crate::phases::read_json(path, 256 * 1024)?;
    let result = router_workflow::resolve_configuration(&layers).map_err(workflow_error)?;
    json_output(&result, ExitCode::SUCCESS)
}
pub(crate) fn read_index(path: &Path) -> Result<Bm25Index, CliError> {
    let bytes = read_bounded(
        path,
        router_retrieval::MAX_DOCUMENT_BYTES,
        CliError::Product("retrieval.read"),
        CliError::Product("retrieval.limit"),
    )?;
    Bm25Index::from_json(&bytes).map_err(retrieval_error)
}
pub(crate) fn search(
    path: &Path,
    text: &str,
    kind: Option<&str>,
    top_k: usize,
    json: bool,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let mut query = SearchQuery::new(text);
    query.top_k = top_k;
    query.kind = kind
        .map(|kind| serde_json::from_value(serde_json::json!(kind)))
        .transpose()
        .map_err(|_| CliError::InvalidCommand)?;
    let result = read_index(path)?.search(&query).map_err(retrieval_error)?;
    json_output(
        &result,
        if result.abstention.is_some() {
            ExitCode::from(3)
        } else {
            ExitCode::SUCCESS
        },
    )
}
fn read_task(path: &Path) -> Result<TaskInput, CliError> {
    let bytes = read_bounded(
        path,
        1024 * 1024,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )?;
    let value =
        router_adapter_api::strict_json(&bytes).map_err(|_| CliError::RequestJsonInvalid)?;
    serde_json::from_value(value).map_err(|_| CliError::RequestJsonInvalid)
}
pub(crate) fn normalize(path: &Path, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let result = router_workflow::normalize(read_task(path)?).map_err(workflow_error)?;
    json_output(
        &result,
        if result.task.is_some() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(3)
        },
    )
}
pub(crate) fn plan(
    path: &Path,
    preset: &str,
    max_width: usize,
    max_depth: usize,
    reserve_cost_microusd: u64,
    json: bool,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let result = router_workflow::normalize(read_task(path)?).map_err(workflow_error)?;
    let Some(task) = &result.task else {
        return json_output(&result, ExitCode::from(3));
    };
    let preset: WorkflowPreset =
        serde_json::from_value(serde_json::json!(preset)).map_err(|_| CliError::InvalidCommand)?;
    let limits = PlanLimits {
        max_width,
        max_depth,
        reserve_cost_microusd,
        ..PlanLimits::default()
    };
    match router_workflow::build_plan(task, preset, limits) {
        Ok(plan) => json_output(
            &serde_json::json!({"schema_version":1,"plan":plan,"assumptions":result.assumptions}),
            ExitCode::SUCCESS,
        ),
        Err(WorkflowError::Cost { suggested }) => json_output(
            &serde_json::json!({"schema_version":1,"error":"workflow.cost.exceeded","suggested_preset":suggested}),
            ExitCode::from(3),
        ),
        Err(error) => Err(workflow_error(error)),
    }
}
