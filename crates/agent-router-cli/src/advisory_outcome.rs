// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `feedback advisory-record`: a manual outcome for a saved `advise` report
//! (ADR 0052). The report is read, never rewritten. Only discrete facts, the
//! syntax-checked identifiers it names and the decision digest are stored; no task
//! text, no path and no comment.
use crate::{CliError, phases::read_json};
use clap::{Args, ValueEnum};
use router_types::{
    AdvisoryAdherence, AdvisoryBinding, AdvisoryMeasurements, AdvisoryOutcomeInput, AdvisoryPhase,
    AdvisoryResult, AdvisorySelectionBasis, AdvisoryUsed, DecisionEnvelope, EffortLevel, ModelId,
    RiskLevel, TaskKind,
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};

/// `advise` refuses to publish a document over this size, newline included
/// (`private_output::publish`), so any `advice.json` it wrote fits.
const MAX_ADVICE: usize = crate::private_output::MAX_DOCUMENT;

#[derive(Clone, Copy, ValueEnum)]
pub enum AdherenceArg {
    Followed,
    Overridden,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ResultArg {
    AcceptedAsIs,
    AcceptedAfterRework,
    Abandoned,
}

#[derive(Args)]
pub struct AdvisoryRecordArgs {
    /// `advice.json` that `advise --output-directory` saved.
    #[arg(long)]
    pub advice: PathBuf,
    /// Whether you followed the recommendation.
    #[arg(long, value_enum)]
    adherence: AdherenceArg,
    /// How the work ended, in your judgment.
    #[arg(long, value_enum)]
    result: ResultArg,
    /// Model used instead of the recommendation. Omitted means UNKNOWN.
    #[arg(long)]
    used_model: Option<String>,
    /// Effort used instead (`low`, `medium`, `high`, ...). Omitted means UNKNOWN.
    #[arg(long)]
    used_effort: Option<String>,
    /// Skill used instead; repeatable. Omitted means UNKNOWN, not "no skill".
    #[arg(
        long = "used-skill",
        value_name = "ID",
        conflicts_with = "used_no_skill"
    )]
    used_skills: Vec<String>,
    /// State that no skill was used instead.
    #[arg(long)]
    used_no_skill: bool,
    /// Cost the host reported, in micro-USD. Omitted means UNKNOWN, never zero.
    #[arg(long)]
    cost_microusd: Option<u64>,
    /// Total tokens the host reported. Omitted means UNKNOWN, never zero.
    #[arg(long)]
    tokens: Option<u64>,
}

/// The part of the saved report this command relies on. Other fields may
/// appear in later report versions and are ignored; the ones read are checked.
#[derive(Deserialize)]
struct SavedAdvice {
    schema_version: u32,
    mode: String,
    executed: bool,
    status: String,
    mapping: Vec<SavedMapping>,
    decision: Option<DecisionEnvelope>,
    next_manual_action: SavedAction,
}

#[derive(Deserialize)]
struct SavedMapping {
    field: String,
    value: Value,
}

#[derive(Deserialize)]
struct SavedAction {
    selection_basis: Option<AdvisorySelectionBasis>,
}

fn invalid(code: &'static str) -> CliError {
    CliError::Product(code)
}

fn mapped<T: serde::de::DeserializeOwned>(
    advice: &SavedAdvice,
    field: &str,
    key: Option<&str>,
) -> std::result::Result<T, CliError> {
    advice
        .mapping
        .iter()
        .find(|row| row.field == field)
        .and_then(|row| match key {
            Some(key) => row.value.get(key),
            None => Some(&row.value),
        })
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))
}

fn binding(advice: &SavedAdvice) -> std::result::Result<AdvisoryBinding, CliError> {
    if advice.schema_version != 1 || advice.mode != "advisory" || advice.executed {
        return Err(invalid("advisory.outcome.advice_invalid"));
    }
    if advice.status != "recommended" {
        return Err(invalid("advisory.outcome.not_recommended"));
    }
    let decision = advice
        .decision
        .as_ref()
        .filter(|decision| decision.has_supported_schema())
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))?;
    let plan = decision
        .selected_plan()
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))?;
    let task_kind: TaskKind = mapped(advice, "kind", None)?;
    let risk: RiskLevel = mapped(advice, "risk", Some("effective"))?;
    let selection_basis = advice
        .next_manual_action
        .selection_basis
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))?;
    Ok(AdvisoryBinding {
        decision_id: decision.decision_id.clone(),
        catalog_version: decision.catalog_version.clone(),
        policy_version: decision.policy_version.clone(),
        task_kind,
        risk,
        selection_basis,
        recommended: plan
            .stages
            .iter()
            .map(|stage| AdvisoryPhase {
                stage: stage.kind,
                model: stage.model.clone(),
                effort: stage.effort,
                skills: stage
                    .skills
                    .iter()
                    .map(|skill| skill.as_str().to_owned())
                    .collect(),
            })
            .collect(),
    })
}

pub(crate) fn input(
    args: AdvisoryRecordArgs,
    now: u64,
) -> std::result::Result<AdvisoryOutcomeInput, CliError> {
    let advice: SavedAdvice = read_json(&args.advice, MAX_ADVICE)?;
    let binding = binding(&advice)?;
    let invalid_flag = || invalid("feedback.invalid");
    let used = match args.adherence {
        AdherenceArg::Followed => {
            if args.used_model.is_some()
                || args.used_effort.is_some()
                || !args.used_skills.is_empty()
                || args.used_no_skill
            {
                return Err(invalid_flag());
            }
            None
        }
        AdherenceArg::Overridden => Some(AdvisoryUsed {
            model: args
                .used_model
                .map(|model| ModelId::parse(model).map_err(|_| invalid_flag()))
                .transpose()?,
            effort: args
                .used_effort
                .map(|effort| {
                    serde_json::from_value::<EffortLevel>(Value::String(effort))
                        .map_err(|_| invalid_flag())
                })
                .transpose()?,
            skills: (args.used_no_skill || !args.used_skills.is_empty())
                .then(|| args.used_skills.into_iter().collect::<BTreeSet<_>>()),
        }),
    };
    Ok(AdvisoryOutcomeInput {
        schema_version: 1,
        observed_at_unix_seconds: now,
        binding,
        adherence: match args.adherence {
            AdherenceArg::Followed => AdvisoryAdherence::Followed,
            AdherenceArg::Overridden => AdvisoryAdherence::Overridden,
        },
        used,
        result: match args.result {
            ResultArg::AcceptedAsIs => AdvisoryResult::AcceptedAsIs,
            ResultArg::AcceptedAfterRework => AdvisoryResult::AcceptedAfterRework,
            ResultArg::Abandoned => AdvisoryResult::Abandoned,
        },
        measurements: AdvisoryMeasurements {
            cost_microusd: args.cost_microusd,
            tokens: args.tokens,
        },
    })
}
