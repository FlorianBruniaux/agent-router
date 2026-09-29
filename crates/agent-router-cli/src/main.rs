#![forbid(unsafe_code)]

mod admission;
mod assets;
mod backend_activation;
mod catalogs;
mod cli;
mod command;
mod coordinator;
mod diagnostics;
mod escalation;
mod evaluation;
mod execution;
mod execution_backend;
mod execution_feedback;
mod feedback;
mod generation;
mod human;
mod inspect;
mod phases;
mod preparation;
mod product;
mod render;
#[cfg(all(feature = "local-api", unix))]
mod serve;
mod session_budget;
mod sidecar;
mod task_guide;

use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};

use clap::{Parser, error::ErrorKind};
use cli::Cli;
use command::Commands;
use router_adapter_api::ValidatedHostObservation;
use router_catalog::Catalog;
use router_policy::Router;
use router_types::{DecisionEnvelope, RouteDecision, RouteRequest};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_DECISION_BYTES: usize = 8 * 1024 * 1024;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                return match error.print() {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(_) => report_error(CliError::Internal),
                };
            }
            _ => return report_error(CliError::InvalidCommand),
        },
    };

    match run(cli.command) {
        Ok(outcome) => {
            match write_stdout(&outcome.output) {
                Ok(()) => outcome.exit_code,
                // A failed write must never report success, nor panic.
                Err(_) => report_error(CliError::Internal),
            }
        }
        Err(error) => report_error(error),
    }
}

struct Outcome {
    output: String,
    exit_code: ExitCode,
}

#[derive(Clone, Copy)]
enum CliError {
    InvalidCommand,
    JsonOutputRequired,
    RequestRead,
    RequestTooLarge,
    RequestJsonInvalid,
    RequestInvalid,
    CatalogRead,
    CatalogTooLarge,
    CatalogInvalid,
    DecisionRead,
    DecisionTooLarge,
    DecisionJsonInvalid,
    DecisionSchema,
    DecisionRender,
    SnapshotRead,
    SnapshotTooLarge,
    Snapshot(&'static str),
    HostUnknown,
    Probe(&'static str),
    Product(&'static str),
    Internal,
}

impl CliError {
    fn code(self) -> &'static str {
        match self {
            Self::InvalidCommand => "cli.invalid",
            Self::JsonOutputRequired => "cli.json.required",
            Self::RequestRead => "request.read",
            Self::RequestTooLarge => "request.too_large",
            Self::RequestJsonInvalid => "request.json.invalid",
            Self::RequestInvalid => "request.invalid",
            Self::CatalogRead => "catalog.read",
            Self::CatalogTooLarge => "catalog.too_large",
            Self::CatalogInvalid => "catalog.invalid",
            Self::DecisionRead => "decision.read",
            Self::DecisionTooLarge => "decision.too_large",
            Self::DecisionJsonInvalid => "decision.json.invalid",
            Self::DecisionSchema => "decision.schema.unsupported",
            Self::DecisionRender => "decision.render",
            Self::SnapshotRead => "snapshot.read",
            Self::SnapshotTooLarge => "snapshot.too_large",
            Self::Snapshot(code) => code,
            Self::HostUnknown => "host.unknown",
            Self::Probe(code) | Self::Product(code) => code,
            Self::Internal => "route.internal",
        }
    }

    fn exit_code(self) -> ExitCode {
        ExitCode::from(exit_class(self.code()) as u8)
    }
}

/// Exit status of a failed command. Status 0 (success), 3 (valid but not final
/// outcome) and 130 (interruption) are chosen by the command itself, never here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ExitClass {
    /// Invalid input or a policy refusal: change the input before retrying.
    Invalid = 2,
    /// Internal serialization, storage or invariant failure.
    Internal = 4,
    /// Transient contention: the same command may be relaunched unchanged.
    Retryable = 5,
    /// The outcome is unknown: inspect the state before any retry.
    OutcomeUnknown = 6,
    /// A hash chain, digest or stored file no longer matches its record.
    Integrity = 7,
}

/// Codes whose effect may already have happened; retrying blindly could repeat it.
const OUTCOME_UNKNOWN_CODES: [&str; 2] = [
    "backend_store.published_durability_unknown",
    "runtime.cleanup.unknown",
];

/// The single classification of every stable error code into an exit status.
/// It reads only the code, so a new mapper inherits its class from the suffix.
/// `*.io` stays invalid input: a missing or unreadable caller path reaches it.
fn exit_class(code: &str) -> ExitClass {
    if OUTCOME_UNKNOWN_CODES.contains(&code) {
        return ExitClass::OutcomeUnknown;
    }
    match code.rsplit('.').next().unwrap_or(code) {
        "busy" | "locked" | "storage_locked" => ExitClass::Retryable,
        "integrity" => ExitClass::Integrity,
        "internal" | "render" | "serialization" | "json" | "storage" | "storage_database" => {
            ExitClass::Internal
        }
        _ => ExitClass::Invalid,
    }
}

fn report_error(error: CliError) -> ExitCode {
    // A closed stderr must not turn a reported error into a panic.
    let _ = writeln!(io::stderr(), "error: {}", error.code());
    error.exit_code()
}

/// Writes the whole output to stdout, surfacing every failure (EPIPE included)
/// instead of panicking like `print!`.
fn write_stdout(output: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

fn run(mut command: Commands) -> Result<Outcome, CliError> {
    let human_title = command.structured_output();
    let mut result = run_structured(command)?;
    if let Some(title) = human_title {
        result.output = human::render(title, &result.output);
    }
    Ok(result)
}

fn run_structured(command: Commands) -> Result<Outcome, CliError> {
    match command {
        Commands::GeneratePlan {
            manifest,
            now,
            json,
        } => generation::run(&manifest, now, json),
        Commands::Sidecar { command, json } => sidecar::run(command, json),
        Commands::BackendActivation { command, json } => backend_activation::run(command, json),
        Commands::Escalation { command, json } => escalation::run(command, json),
        Commands::Admission { command, json } => admission::run(command, json),
        Commands::Coordinate { command, json } => coordinator::run(command, json),
        Commands::Assets { command } => match command {
            command::AssetCommand::Review {
                manifest,
                inputs,
                skills,
                json,
            } => assets::review(&manifest, &inputs, &skills, json),
        },
        Commands::Prepare {
            manifest,
            output_directory,
            json,
        } => preparation::run(&manifest, output_directory.as_deref(), json),
        Commands::Run {
            definition,
            store,
            sandbox,
            approve,
            retry,
            json,
        } => execution::run(
            &definition,
            &store,
            &sandbox,
            approve.as_deref(),
            retry,
            json,
        ),
        Commands::Status {
            store,
            run_id,
            json,
        } => execution::status(&store, &run_id, json),
        Commands::Cancel {
            store,
            run_id,
            json,
        } => execution::cancel(&store, &run_id, json),
        Commands::Events {
            store,
            run_id,
            json,
        } => execution::events(&store, &run_id, json),
        Commands::SourceState { workspace, json } => execution::source_state(&workspace, json),
        Commands::Workflow {
            plan,
            execution_contract,
            seal_contract,
            json,
        } => execution::workflow(&plan, execution_contract.as_deref(), seal_contract, json),
        Commands::Verify {
            definition,
            store,
            evidence,
            json,
        } => execution::verify(&definition, &store, &evidence, json),
        Commands::Artifacts {
            definition,
            store,
            command,
            json,
        } => execution::artifacts(&definition, &store, command, json),
        Commands::Catalog { command } => catalogs::run(command),
        Commands::Feedback {
            store,
            now,
            json,
            command,
        } => feedback::feedback(&store, now, json, command),
        Commands::Policy {
            store,
            now,
            json,
            command,
        } => feedback::policy(&store, now, json, command),
        Commands::Doctor {
            request,
            runtime_root,
            now,
            deep,
            json,
        } => diagnostics::doctor(request.as_deref(), runtime_root.as_deref(), now, deep, json),
        Commands::Audit { command } => diagnostics::audit(command),
        Commands::Config { layers, json } => product::configuration(&layers, json),
        Commands::RoutePhase { inputs, json } => phases::route(inputs, json),
        Commands::Backend {
            inputs,
            backend,
            expected_backend_artifact,
            endpoint,
            bearer_env,
            allow_redacted_metadata,
            on_error,
            timeout_ms,
            minimum_confidence_micros,
            json,
        } => phases::backend(
            inputs,
            phases::BackendArgs {
                backend,
                expected_backend_artifact,
                endpoint,
                bearer_env,
                allow_redacted_metadata,
                on_error,
                timeout_ms,
                minimum_confidence_micros,
            },
            json,
        ),
        Commands::Replay {
            record,
            policy,
            now,
            snapshot_max_age_seconds,
            json,
        } => evaluation::replay(&record, &policy, now, snapshot_max_age_seconds, json),
        Commands::Eval { command } => evaluation::run(command),
        Commands::Route {
            request,
            catalog,
            json,
            host_snapshot,
            retrieval,
            snapshot_max_age_seconds,
            now,
        } => route(
            &request,
            &catalog,
            json,
            host_snapshot.as_deref(),
            snapshot_max_age_seconds,
            now,
            retrieval.as_deref(),
        ),
        Commands::Inspect {
            host,
            model_cache,
            native_stream,
            observed_at,
            observed_version,
            now,
            json,
        } => inspect::inspect(
            inspect::InspectArgs {
                host,
                model_cache,
                native_stream,
                observed_at,
                observed_version,
                now,
            },
            json,
        ),
        Commands::Search {
            metadata,
            catalog,
            now,
            query,
            context_query,
            semantic_ranking,
            kind,
            top_k,
            json,
        } => product::search(product::SearchArgs {
            metadata: &metadata,
            catalog: catalog.as_deref(),
            now,
            text: query.as_deref(),
            context_query: context_query.as_deref(),
            semantic_ranking: semantic_ranking.as_deref(),
            kind: kind.as_deref(),
            top_k,
            json,
        }),
        Commands::Index {
            metadata,
            signals,
            catalog,
            now,
            json,
        } => product::index(&metadata, signals.as_deref(), &catalog, now, json),
        Commands::Refresh {
            metadata,
            successor,
            expected_source_hash,
            catalog,
            now,
            output,
            json,
        } => product::refresh(
            &metadata,
            &successor,
            &expected_source_hash,
            &catalog,
            now,
            output.as_deref(),
            json,
        ),
        Commands::TaskGuide { args, json } => task_guide::run(args, json),
        Commands::Normalize {
            task,
            risk_context,
            json,
        } => product::normalize(&task, risk_context.as_deref(), json),
        Commands::Plan {
            task,
            risk_context,
            preset,
            max_width,
            max_depth,
            reserve_cost_microusd,
            json,
        } => product::plan(
            &task,
            risk_context.as_deref(),
            &preset,
            max_width,
            max_depth,
            reserve_cost_microusd,
            json,
        ),
        Commands::Explain { decision } => explain(&decision),
        #[cfg(all(feature = "local-api", unix))]
        Commands::Serve {
            state_dir,
            max_connections,
            request_deadline_ms,
        } => serve::run(&state_dir, max_connections, request_deadline_ms),
    }
}

fn route(
    request_path: &Path,
    catalog_path: &Path,
    json: bool,
    snapshot_path: Option<&Path>,
    snapshot_max_age: u64,
    now: Option<u64>,
    retrieval: Option<&Path>,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }

    let mut request = read_request(request_path)?;
    let snapshot = snapshot_path
        .map(|path| inspect::read_snapshot(path, now, snapshot_max_age))
        .transpose()?;
    let (catalog, embedded_snapshot) = catalogs::load(catalog_path, now, snapshot_max_age)?;
    if let Some(path) = retrieval {
        let index = product::read_index(path)?;
        let query = router_retrieval::SearchQuery::new(&request.task.objective);
        request.skill_candidates = if index.catalog_hash().is_some() {
            let hash = catalog
                .content_hash()
                .map_err(|_| CliError::CatalogInvalid)?;
            index.skill_candidates_with_catalog(&query, &hash)
        } else {
            index.skill_candidates(&query)
        }
        .map_err(product::retrieval_error)?;
    }
    decide(request, &catalog, snapshot, embedded_snapshot)
}

/// Shared by `route` and the local API: merges the explicit and embedded host
/// snapshots, routes, and renders the canonical decision JSON.
pub(crate) fn decide(
    mut request: RouteRequest,
    catalog: &Catalog,
    snapshot: Option<ValidatedHostObservation>,
    embedded_snapshot: Option<ValidatedHostObservation>,
) -> Result<Outcome, CliError> {
    let snapshot = match (snapshot, embedded_snapshot) {
        (Some(explicit), Some(embedded)) if explicit.hash() != embedded.hash() => {
            return Err(CliError::Product("catalog.snapshot.mismatch"));
        }
        (Some(explicit), _) => Some(explicit),
        (None, embedded) => embedded,
    };
    if let Some(snapshot) = &snapshot {
        request.host = snapshot.host().clone();
        request.capability_knowledge = snapshot.capabilities();
    }
    let router = Router::stable_v1();
    let decision = match &snapshot {
        Some(snapshot) => router.route_with_snapshot_hash(&request, catalog, snapshot.hash()),
        None => router.route(&request, catalog),
    }
    .map_err(|error| {
        if error.is_invalid_request() {
            CliError::RequestInvalid
        } else {
            CliError::Internal
        }
    })?;
    let exit_code = match decision.decision {
        RouteDecision::Selected { .. } => ExitCode::SUCCESS,
        RouteDecision::NoRoute { .. } => ExitCode::from(3),
    };

    Ok(Outcome {
        output: render::decision_json(&decision).map_err(|_| CliError::DecisionRender)?,
        exit_code,
    })
}

fn explain(decision_path: &Path) -> Result<Outcome, CliError> {
    let bytes = read_bounded(
        decision_path,
        MAX_DECISION_BYTES,
        CliError::DecisionRead,
        CliError::DecisionTooLarge,
    )?;
    let decision = serde_json::from_slice::<DecisionEnvelope>(&bytes)
        .map_err(|_| CliError::DecisionJsonInvalid)?;
    if !decision.has_supported_schema() {
        return Err(CliError::DecisionSchema);
    }
    Ok(Outcome {
        output: render::explain(&decision),
        exit_code: ExitCode::SUCCESS,
    })
}

fn read_request(path: &Path) -> Result<RouteRequest, CliError> {
    let bytes = read_bounded(
        path,
        MAX_REQUEST_BYTES,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )?;
    serde_json::from_slice(&bytes).map_err(|_| CliError::RequestJsonInvalid)
}

fn read_catalog(path: &Path) -> Result<Catalog, CliError> {
    let bytes = read_bounded(
        path,
        MAX_CATALOG_BYTES,
        CliError::CatalogRead,
        CliError::CatalogTooLarge,
    )?;
    Catalog::from_json(&bytes).map_err(|_| CliError::CatalogInvalid)
}

fn read_bounded(
    path: &Path,
    max_bytes: usize,
    read_error: CliError,
    too_large: CliError,
) -> Result<Vec<u8>, CliError> {
    let file = open_regular(path, read_error)?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| read_error)?;
    if bytes.len() > max_bytes {
        return Err(too_large);
    }
    Ok(bytes)
}

/// Opens a regular file without following a final symlink or blocking on a FIFO.
fn open_regular(path: &Path, read_error: CliError) -> Result<std::fs::File, CliError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| read_error)?;
    if !file.metadata().map_err(|_| read_error)?.is_file() {
        return Err(read_error);
    }
    Ok(file)
}

#[cfg(test)]
mod exit_class_tests {
    use super::*;

    #[test]
    fn every_exit_class_is_reached_by_real_error_codes() {
        use ExitClass::*;
        for (code, class) in [
            ("cli.invalid", Invalid),
            ("request.json.invalid", Invalid),
            ("decision.schema.unsupported", Invalid),
            ("policy.composition.denied", Invalid),
            ("runtime.approval.required_or_changed", Invalid),
            ("runtime.sandbox.unknown_or_unsafe", Invalid),
            ("runtime.io", Invalid),
            ("runtime.not_found", Invalid),
            ("session_budget.accounting.unknown", Invalid),
            ("snapshot.json.invalid", Invalid),
            ("route.internal", Internal),
            ("decision.render", Internal),
            ("phase.serialization", Internal),
            ("feedback.serialization", Internal),
            ("runtime.json", Internal),
            ("coordinator.json", Internal),
            ("runtime.storage", Internal),
            ("coordinator.storage", Internal),
            ("feedback.storage_database", Internal),
            ("runtime.busy", Retryable),
            ("coordinator.busy", Retryable),
            ("backend_store.busy", Retryable),
            ("runtime.locked", Retryable),
            ("coordinator.locked", Retryable),
            ("feedback.storage_locked", Retryable),
            ("backend_store.published_durability_unknown", OutcomeUnknown),
            ("runtime.cleanup.unknown", OutcomeUnknown),
            ("runtime.integrity", Integrity),
            ("coordinator.integrity", Integrity),
            ("backend_store.integrity", Integrity),
            ("feedback.integrity", Integrity),
            ("retrieval.integrity", Integrity),
            ("workflow.integrity", Integrity),
            ("eval.integrity", Integrity),
            ("phase.catalog.integrity", Integrity),
            ("doctor.audit.integrity", Integrity),
        ] {
            assert_eq!(exit_class(code), class, "{code}");
        }
    }

    #[test]
    fn fixed_cli_errors_keep_their_documented_status() {
        for (error, status) in [
            (CliError::InvalidCommand, 2),
            (CliError::RequestTooLarge, 2),
            (CliError::DecisionSchema, 2),
            (CliError::Internal, 4),
            (CliError::DecisionRender, 4),
            (CliError::Product("runtime.busy"), 5),
            (CliError::Product("runtime.cleanup.unknown"), 6),
            (CliError::Product("runtime.integrity"), 7),
        ] {
            assert_eq!(exit_class(error.code()) as u8, status, "{}", error.code());
        }
    }

    #[test]
    fn the_outcome_unknown_list_is_not_matched_by_suffix() {
        // Only listed codes are unknown outcomes; other `unknown` suffixes are refusals.
        for code in ["task.budget.unknown", "risk.environment.unknown"] {
            assert_eq!(exit_class(code), ExitClass::Invalid, "{code}");
        }
    }
}
