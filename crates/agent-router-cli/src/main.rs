#![forbid(unsafe_code)]

mod cli;
mod command;
mod render;

use std::{fs::File, io::Read, path::Path, process::ExitCode};

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
        Commands::Route {
            request,
            catalog,
            json,
        } => route(&request, &catalog, json),
        Commands::Explain { decision } => explain(&decision),
    }
}

fn route(request_path: &Path, catalog_path: &Path, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }

    let request = read_request(request_path)?;
    let catalog = read_catalog(catalog_path)?;
    let decision = Router::stable_v1()
        .route(&request, &catalog)
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
    let file = File::open(path).map_err(|_| read_error)?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| read_error)?;
    if bytes.len() > max_bytes {
        return Err(too_large);
    }
    Ok(bytes)
}
