use crate::{
    CliError, Outcome, command::EvalCommand, inspect, phases::read_json, product::json_output,
    read_bounded,
};
use router_adapter_api::SnapshotValidation;
use router_eval::{
    CandidateCorpus, EvalError, PairedTrace, ReplayRecord, compare_shadow, replay_deterministic,
};
use std::{path::Path, process::ExitCode};

fn error(error: EvalError) -> CliError {
    CliError::Product(match error {
        EvalError::Invalid(_) => "eval.invalid",
        EvalError::Limit(_) => "eval.limit",
        EvalError::Integrity => "eval.integrity",
        EvalError::UnknownModel => "eval.model.unknown",
        EvalError::UnknownPricing => "eval.pricing.unknown",
        EvalError::CostOverflow => "eval.pricing.overflow",
        EvalError::UnknownPolicy => "eval.policy.unknown",
        EvalError::Snapshot(_) => "eval.snapshot.invalid",
        EvalError::Catalog => "eval.catalog.invalid",
        EvalError::Backend => "eval.backend.failed",
    })
}
fn bytes(path: &Path) -> Result<Vec<u8>, CliError> {
    read_bounded(
        path,
        router_eval::MAX_REPLAY_BYTES,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )
}
fn validation(now: Option<u64>, max_age_seconds: u64) -> Result<SnapshotValidation, CliError> {
    Ok(SnapshotValidation {
        now_unix_seconds: inspect::current_time(now)?,
        max_age_seconds,
    })
}
pub(crate) fn replay(
    path: &Path,
    policy: &str,
    now: Option<u64>,
    max_age: u64,
    json: bool,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let record = ReplayRecord::from_json(&bytes(path)?).map_err(error)?;
    let trace = replay_deterministic(&record, policy, validation(now, max_age)?).map_err(error)?;
    json_output(&trace, ExitCode::SUCCESS)
}
pub(crate) fn run(command: EvalCommand) -> Result<Outcome, CliError> {
    match command {
        EvalCommand::Seal { input, json } => {
            if !json {
                return Err(CliError::JsonOutputRequired);
            }
            let input = read_json(&input, router_eval::MAX_REPLAY_BYTES)?;
            json_output(
                &ReplayRecord::seal(input).map_err(error)?,
                ExitCode::SUCCESS,
            )
        }
        EvalCommand::Corpus { corpus, json } => {
            if !json {
                return Err(CliError::JsonOutputRequired);
            }
            let corpus = CandidateCorpus::from_json(&bytes(&corpus)?).map_err(error)?;
            json_output(
                &serde_json::json!({"schema_version":1,"valid":true,"content_hash":corpus.content_hash().map_err(error)?,"corpus":corpus}),
                ExitCode::SUCCESS,
            )
        }
        EvalCommand::Compare {
            records,
            baseline,
            candidate,
            criteria,
            now,
            snapshot_max_age_seconds,
            json,
        } => {
            if !json {
                return Err(CliError::JsonOutputRequired);
            }
            if records.len() > router_eval::MAX_EVALUATION_CASES {
                return Err(CliError::Product("eval.limit"));
            }
            let validation = validation(now, snapshot_max_age_seconds)?;
            let criteria = criteria
                .as_ref()
                .map(|path| read_json(path, 64 * 1024))
                .transpose()?;
            let mut pairs = Vec::new();
            for path in records {
                let record = ReplayRecord::from_json(&bytes(&path)?).map_err(error)?;
                pairs.push(PairedTrace {
                    baseline: replay_deterministic(&record, &baseline, validation)
                        .map_err(error)?,
                    semantic: replay_deterministic(&record, &candidate, validation)
                        .map_err(error)?,
                });
            }
            let report = compare_shadow(&pairs, criteria.as_ref()).map_err(error)?;
            let code = if report.promotion_blockers.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            };
            json_output(&report, code)
        }
    }
}
