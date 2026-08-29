#![forbid(unsafe_code)]

mod cli;
mod command;
mod render;

use std::{fs, path::Path, process::ExitCode};

use clap::Parser;
use cli::Cli;
use command::Commands;
use router_catalog::Catalog;
use router_policy::Router;
use router_types::{DecisionEnvelope, RouteDecision, RouteRequest};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(_) => return report_error(CliError::InvalidCommand),
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
    RequestJsonInvalid,
    CatalogRead,
    CatalogInvalid,
    DecisionRead,
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
            Self::RequestJsonInvalid => "request.json.invalid",
            Self::CatalogRead => "catalog.read",
            Self::CatalogInvalid => "catalog.invalid",
            Self::DecisionRead => "decision.read",
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
        .map_err(|_| CliError::Internal)?;
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
    let bytes = fs::read(decision_path).map_err(|_| CliError::DecisionRead)?;
    let decision = serde_json::from_slice::<DecisionEnvelope>(&bytes)
        .map_err(|_| CliError::DecisionJsonInvalid)?;
    Ok(Outcome {
        output: render::explain(&decision),
        exit_code: ExitCode::SUCCESS,
    })
}

fn read_request(path: &Path) -> Result<RouteRequest, CliError> {
    let bytes = fs::read(path).map_err(|_| CliError::RequestRead)?;
    serde_json::from_slice(&bytes).map_err(|_| CliError::RequestJsonInvalid)
}

fn read_catalog(path: &Path) -> Result<Catalog, CliError> {
    let bytes = fs::read(path).map_err(|_| CliError::CatalogRead)?;
    Catalog::from_json(&bytes).map_err(|_| CliError::CatalogInvalid)
}
