// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `feedback advisory-record`: a manual outcome for a saved `advise` report
//! (ADR 0052). The report is read, never rewritten. Only discrete facts, the
//! syntax-checked identifiers it names and the decision digest are stored; no task
//! text, no path and no comment.
use crate::{CliError, phases::read_json};
use clap::{ArgGroup, Args, ValueEnum};
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
#[command(group(
    ArgGroup::new("adherence_choice")
        .required(true)
        .args(["adherence", "followed", "overridden"])
))]
pub struct AdvisoryRecordArgs {
    /// `advice.json` that `advise --output-directory` saved, or that directory.
    /// For `advise --compare-policies`, the directory or its `compare.json`.
    #[arg(long)]
    pub advice: PathBuf,
    /// Which saved decision of a comparison this outcome is about (`stable-v2`,
    /// ...): the plan you followed, or the plan you departed from with
    /// `--overridden`. Required for a comparison, refused for a single advice.
    #[arg(long, value_name = "NAME")]
    policy: Option<String>,
    /// Whether you followed the recommendation.
    #[arg(long, value_enum)]
    adherence: Option<AdherenceArg>,
    /// Shorthand for `--adherence followed`.
    #[arg(long)]
    followed: bool,
    /// Shorthand for `--adherence overridden`.
    #[arg(long)]
    overridden: bool,
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

/// The part of a saved `compare.json` this command relies on.
#[derive(Deserialize)]
struct SavedComparison {
    schema_version: u32,
    mode: String,
    executed: bool,
    arms: Vec<SavedArm>,
}

#[derive(Deserialize)]
struct SavedArm {
    policy: String,
    decision_id: String,
    directory: Option<String>,
}

const COMPARISON_MARKER: &str = "compare.json";

/// The `advice.json` to read and, for a comparison, the decision id its arm
/// promises. A single advice has no arms, so naming one is refused.
fn resolve(
    advice: &std::path::Path,
    policy: Option<&str>,
) -> std::result::Result<(PathBuf, Option<String>), CliError> {
    let comparison = if advice.is_dir() {
        let marker = advice.join(COMPARISON_MARKER);
        (!advice.join("advice.json").exists() && marker.exists()).then_some(marker)
    } else {
        (advice
            .file_name()
            .is_some_and(|name| name == COMPARISON_MARKER))
        .then(|| advice.to_owned())
    };
    let Some(marker) = comparison else {
        if policy.is_some() {
            return Err(invalid("advisory.outcome.policy_unexpected"));
        }
        let path = if advice.is_dir() {
            advice.join("advice.json")
        } else {
            advice.to_owned()
        };
        return Ok((path, None));
    };
    let saved: SavedComparison = read_json(&marker, MAX_ADVICE)?;
    if saved.schema_version != 1 || saved.mode != "advisory_comparison" || saved.executed {
        return Err(invalid("advisory.outcome.advice_invalid"));
    }
    let policy = policy.ok_or_else(|| invalid("advisory.outcome.policy_required"))?;
    // The name is matched against the arms the comparison lists, never joined
    // to a path as typed.
    let arm = saved
        .arms
        .iter()
        .find(|arm| arm.policy == policy)
        .ok_or_else(|| invalid("advisory.outcome.policy_unknown"))?;
    let directory = arm
        .directory
        .as_deref()
        .filter(|name| !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.'))
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))?;
    let root = marker
        .parent()
        .ok_or_else(|| invalid("advisory.outcome.advice_invalid"))?;
    Ok((
        root.join(directory).join("advice.json"),
        Some(arm.decision_id.clone()),
    ))
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
    // The advice directory that `advise --output-directory` created is accepted
    // in place of its `advice.json` marker.
    let (advice_path, promised) = resolve(&args.advice, args.policy.as_deref())?;
    let advice: SavedAdvice = read_json(&advice_path, MAX_ADVICE)?;
    let binding = binding(&advice)?;
    if promised.is_some_and(|id| id != binding.decision_id) {
        return Err(invalid("advisory.outcome.advice_invalid"));
    }
    let invalid_flag = || invalid("feedback.invalid");
    let adherence = match (args.adherence, args.followed, args.overridden) {
        (Some(value), false, false) => value,
        (None, true, false) => AdherenceArg::Followed,
        (None, false, true) => AdherenceArg::Overridden,
        _ => return Err(invalid_flag()),
    };
    let used = match adherence {
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
        adherence: match adherence {
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
