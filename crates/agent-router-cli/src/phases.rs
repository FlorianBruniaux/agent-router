use crate::{
    CliError, Outcome, catalogs, command::PhaseArgs, inspect, product::json_output, read_bounded,
};
use router_decision::{
    BackendOptions, BoundedDecisionContext, DecisionBackend, DeterministicBackend, decide_shadow,
};
use router_policy::{PhaseRouteError, PhaseRouter, ValidatedPhaseContext};
use router_types::{BackendDataPolicy, BackendFailurePolicy};
use std::{net::SocketAddr, path::Path, process::ExitCode, time::Duration};

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    limit: usize,
) -> Result<T, CliError> {
    let bytes = read_bounded(
        path,
        limit,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )?;
    let value =
        router_adapter_api::strict_json(&bytes).map_err(|_| CliError::RequestJsonInvalid)?;
    serde_json::from_value(value).map_err(|_| CliError::RequestJsonInvalid)
}
pub(crate) fn error(error: PhaseRouteError) -> CliError {
    CliError::Product(match error {
        PhaseRouteError::Invalid(_) => "phase.invalid",
        PhaseRouteError::Limit(_) => "phase.limit",
        PhaseRouteError::Serialization => "phase.serialization",
        PhaseRouteError::Catalog => "phase.catalog.integrity",
        PhaseRouteError::OutsidePool => "phase.selection.outside_pool",
        PhaseRouteError::Ineligible => "phase.selection.ineligible",
        PhaseRouteError::PolicyMismatch => "phase.policy.mismatch",
    })
}
pub(crate) fn load(inputs: &PhaseArgs) -> Result<(PhaseRouter, ValidatedPhaseContext), CliError> {
    let request = read_json(&inputs.context, router_policy::MAX_PHASE_REQUEST_BYTES)?;
    let snapshot = inspect::read_snapshot(
        &inputs.host_snapshot,
        inputs.now,
        inputs.snapshot_max_age_seconds,
    )?;
    let (catalog, embedded) =
        catalogs::load(&inputs.catalog, inputs.now, inputs.snapshot_max_age_seconds)?;
    if embedded.is_some_and(|value| value.hash() != snapshot.hash()) {
        return Err(CliError::Product("catalog.snapshot.mismatch"));
    }
    let router = if let Some(path) = &inputs.policy {
        PhaseRouter::new(read_json(path, 64 * 1024)?).map_err(error)?
    } else {
        PhaseRouter::stable_v2()
    };
    let context = router
        .context(request, &snapshot, &catalog)
        .map_err(error)?;
    Ok((router, context))
}
pub(crate) fn route(inputs: PhaseArgs, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let (router, context) = load(&inputs)?;
    let receipt = router.route(&context).map_err(error)?;
    let code = if receipt.selected.is_some() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    };
    json_output(&receipt, code)
}
pub(crate) struct BackendArgs {
    pub backend: String,
    pub endpoint: Option<SocketAddr>,
    pub bearer_env: Option<String>,
    pub allow_redacted_metadata: bool,
    pub on_error: String,
    pub timeout_ms: u64,
    pub minimum_confidence_micros: u32,
}
fn adapter(args: &BackendArgs) -> Result<Box<dyn DecisionBackend>, CliError> {
    if args.backend == "deterministic" {
        if args.endpoint.is_some() || args.bearer_env.is_some() {
            return Err(CliError::InvalidCommand);
        }
        return Ok(Box::new(DeterministicBackend));
    }
    #[cfg(feature = "local-sidecar")]
    {
        let endpoint = args
            .endpoint
            .ok_or(CliError::Product("backend.endpoint.required"))?;
        let bearer = args
            .bearer_env
            .as_ref()
            .map(|name| {
                if name.is_empty()
                    || name.len() > 64
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(CliError::InvalidCommand);
                }
                std::env::var(name)
                    .map_err(|_| CliError::Product("backend.authentication.unavailable"))
            })
            .transpose()?;
        let backend: Box<dyn DecisionBackend> = match args.backend.as_str() {
            "laya" => Box::new(
                router_decision::LayaLocalBackend::new(endpoint, bearer)
                    .map_err(|e| CliError::Product(e.code()))?,
            ),
            "jev" => Box::new(
                router_decision::JevSidecarBackend::new(endpoint, bearer)
                    .map_err(|e| CliError::Product(e.code()))?,
            ),
            _ => return Err(CliError::InvalidCommand),
        };
        Ok(backend)
    }
    #[cfg(not(feature = "local-sidecar"))]
    Err(CliError::Product("backend.feature.disabled"))
}
pub(crate) fn backend(
    inputs: PhaseArgs,
    args: BackendArgs,
    json: bool,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let (router, context) = load(&inputs)?;
    let context = BoundedDecisionContext::from_phase(&router, &context)
        .map_err(|e| CliError::Product(e.code()))?;
    let backend = adapter(&args)?;
    let result = decide_shadow(
        &context,
        backend.as_ref(),
        BackendOptions {
            data_policy: if args.allow_redacted_metadata {
                BackendDataPolicy::RedactedMetadataToLoopback
            } else {
                BackendDataPolicy::Deny
            },
            failure_policy: if args.on_error == "abstain" {
                BackendFailurePolicy::Abstain
            } else {
                BackendFailurePolicy::DeterministicFallback
            },
            timeout: Duration::from_millis(args.timeout_ms),
            minimum_confidence_micros: args.minimum_confidence_micros,
        },
    )
    .map_err(|e| CliError::Product(e.code()))?;
    let code = if result.active_candidate_id.is_some() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    };
    json_output(&result, code)
}
