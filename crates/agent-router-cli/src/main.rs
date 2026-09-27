#![forbid(unsafe_code)]

mod catalogs;
mod cli;
mod command;
mod diagnostics;
mod evaluation;
mod feedback;
mod inspect;
mod phases;
mod product;
mod render;

use std::{fs::OpenOptions, io::Read, path::Path, process::ExitCode};

use clap::{Parser, error::ErrorKind};
use cli::Cli;
use command::Commands;
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
            print!("{}", outcome.output);
            outcome.exit_code
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
        match self {
            Self::Internal | Self::DecisionRender => ExitCode::from(4),
            _ => ExitCode::from(2),
        }
    }
}

fn report_error(error: CliError) -> ExitCode {
    eprintln!("error: {}", error.code());
    error.exit_code()
}

fn run(command: Commands) -> Result<Outcome, CliError> {
    match command {
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
        Commands::Doctor { request, now, json } => diagnostics::doctor(&request, now, json),
        Commands::Audit { command } => diagnostics::audit(command),
        Commands::Config { layers, json } => product::configuration(&layers, json),
        Commands::RoutePhase { inputs, json } => phases::route(inputs, json),
        Commands::Backend {
            inputs,
            backend,
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
            query,
            kind,
            top_k,
            json,
        } => product::search(&metadata, &query, kind.as_deref(), top_k, json),
        Commands::Normalize { task, json } => product::normalize(&task, json),
        Commands::Plan {
            task,
            preset,
            max_width,
            max_depth,
            reserve_cost_microusd,
            json,
        } => product::plan(
            &task,
            &preset,
            max_width,
            max_depth,
            reserve_cost_microusd,
            json,
        ),
        Commands::Explain { decision } => explain(&decision),
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
    if let Some(path) = retrieval {
        request.skill_candidates = product::read_index(path)?
            .skill_candidates(&router_retrieval::SearchQuery::new(&request.task.objective))
            .map_err(product::retrieval_error)?;
    }
    let (catalog, embedded_snapshot) = catalogs::load(catalog_path, now, snapshot_max_age)?;
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
        Some(snapshot) => router.route_with_snapshot_hash(&request, &catalog, snapshot.hash()),
        None => router.route(&request, &catalog),
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
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| read_error)?;
    if bytes.len() > max_bytes {
        return Err(too_large);
    }
    Ok(bytes)
}
