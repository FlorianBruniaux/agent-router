// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Manual outcomes of advisory recommendations (ADR 0052). A user did a task by
//! hand and says whether the `advise` recommendation was followed and how it
//! went. The record is bound to the saved decision by its digest, never to a
//! supervised run, and holds enums, syntax-checked identifiers and bounded counts.
use std::collections::BTreeMap;

use router_types::{
    AdvisoryAdherence, AdvisoryOutcomeEvent, AdvisoryOutcomeInput, AdvisoryResult,
    AdvisorySelectionBasis, TaskKind,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::{
    FeedbackError, FeedbackStore, MAX_EVENTS, MAX_INPUT_BYTES, MAX_VALUE, Result, hash, identifier,
    store::{ensure_clock, expire, read_consent, scrub, timestamp},
    valid_hash,
};

const MAX_PHASES: usize = 16;

/// Identifier syntax for strings the user can influence. It is stricter than
/// `identifier`: a separator, a leading dot or a tilde would let a path through.
/// This checks syntax, not membership in any catalog.
fn path_free_identifier(value: &str) -> bool {
    identifier(value) && !value.contains(['/', '\\']) && !value.starts_with(['.', '~'])
}
const MAX_SKILLS: usize = 64;

pub(crate) fn validate_outcome(input: &AdvisoryOutcomeInput) -> Result<()> {
    let b = &input.binding;
    if input.schema_version != 1
        || !valid_hash(&b.decision_id)
        || !path_free_identifier(&b.catalog_version)
        || !path_free_identifier(&b.policy_version)
        || b.recommended.is_empty()
        || b.recommended.len() > MAX_PHASES
        || b.recommended.iter().any(|phase| {
            !path_free_identifier(phase.model.as_str())
                || phase.skills.len() > MAX_SKILLS
                || phase
                    .skills
                    .iter()
                    .any(|skill| !path_free_identifier(skill))
        })
    {
        return Err(FeedbackError::Invalid("advisory binding"));
    }
    // Followed states no substitute; an override must say something about it.
    let coherent = match (input.adherence, &input.used) {
        (AdvisoryAdherence::Followed, None) => true,
        (AdvisoryAdherence::Overridden, Some(used)) => {
            (used.model.is_some() || used.effort.is_some() || used.skills.is_some())
                && used
                    .model
                    .as_ref()
                    .is_none_or(|model| path_free_identifier(model.as_str()))
                && used.skills.as_ref().is_none_or(|skills| {
                    skills.len() <= MAX_SKILLS
                        && skills.iter().all(|skill| path_free_identifier(skill))
                })
                // Stating the whole recommendation back is not an override.
                && !restates(&b.recommended, used)
        }
        _ => false,
    };
    if !coherent {
        return Err(FeedbackError::Invalid("advisory adherence"));
    }
    let m = &input.measurements;
    if [m.cost_microusd, m.tokens]
        .into_iter()
        .flatten()
        .any(|v| v > MAX_VALUE)
    {
        return Err(FeedbackError::Invalid("advisory measurements"));
    }
    if serde_json::to_vec(input)?.len() > MAX_INPUT_BYTES {
        return Err(FeedbackError::Limit);
    }
    Ok(())
}

/// Every field stated and equal to every recommended phase.
fn restates(
    recommended: &[router_types::AdvisoryPhase],
    used: &router_types::AdvisoryUsed,
) -> bool {
    let (Some(model), Some(effort), Some(skills)) = (&used.model, used.effort, &used.skills) else {
        return false;
    };
    recommended
        .iter()
        .all(|phase| &phase.model == model && phase.effort == effort && &phase.skills == skills)
}

pub(crate) fn outcome_hash(event: &AdvisoryOutcomeEvent) -> Result<String> {
    hash(
        "advisory-outcome-v1",
        &(
            event.sequence,
            event.attempt,
            event.consent_revision,
            event.recorded_at_unix_seconds,
            &event.input,
        ),
    )
}

/// The same outcome observed at another second is the same outcome.
fn same_outcome(left: &AdvisoryOutcomeInput, right: &AdvisoryOutcomeInput) -> bool {
    let mut left = left.clone();
    left.observed_at_unix_seconds = right.observed_at_unix_seconds;
    left == *right
}

impl FeedbackStore {
    /// Same consent, clock and retention rules as `record`. Repeating the
    /// latest outcome of a decision is idempotent; a different outcome for the
    /// same decision is stored as its next attempt, and the report counts the
    /// latest attempt only. Nothing is rewritten, so a correction keeps history.
    pub fn record_advisory_outcome(
        &self,
        input: AdvisoryOutcomeInput,
        now: u64,
    ) -> Result<AdvisoryOutcomeEvent> {
        validate_outcome(&input)?;
        timestamp(now)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let consent = read_consent(&tx)?;
        if !consent.preferences.collection_enabled {
            return Err(FeedbackError::Consent);
        }
        ensure_clock(&tx, now)?;
        if now < consent.updated_at_unix_seconds
            || input.observed_at_unix_seconds > now
            || input.observed_at_unix_seconds < consent.updated_at_unix_seconds
            || now - input.observed_at_unix_seconds > consent.preferences.retention_seconds
        {
            return Err(FeedbackError::Invalid("observation time"));
        }
        let expired = expire(&tx, &consent, now)?;
        let latest = tx
            .query_row(
                "SELECT event FROM advisory_outcomes WHERE decision_id=?1 ORDER BY sequence DESC LIMIT 1",
                [&input.binding.decision_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|text| serde_json::from_str::<AdvisoryOutcomeEvent>(&text))
            .transpose()?;
        if let Some(existing) = &latest {
            if existing.outcome_hash != outcome_hash(existing)? {
                return Err(FeedbackError::Integrity("advisory outcome"));
            }
            // An exact repeat of the latest record changes nothing.
            if same_outcome(&existing.input, &input) {
                tx.commit()?;
                if expired > 0 {
                    scrub(&db)?;
                }
                return Ok(existing.clone());
            }
        }
        let attempt = match &latest {
            Some(existing) => existing
                .attempt
                .checked_add(1)
                .ok_or(FeedbackError::Limit)?,
            None => 1,
        };
        let count: usize =
            tx.query_row("SELECT COUNT(*) FROM advisory_outcomes", [], |r| r.get(0))?;
        if count >= MAX_EVENTS {
            return Err(FeedbackError::Limit);
        }
        let sequence: u64 = tx.query_row(
            "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name='advisory_outcomes'),0)+1",
            [],
            |r| r.get(0),
        )?;
        let mut event = AdvisoryOutcomeEvent {
            sequence,
            attempt,
            consent_revision: consent.revision,
            recorded_at_unix_seconds: now,
            input,
            outcome_hash: String::new(),
        };
        event.outcome_hash = outcome_hash(&event)?;
        tx.execute(
            "INSERT INTO advisory_outcomes(sequence,decision_id,attempt,observed_at,event) VALUES(?1,?2,?3,?4,?5)",
            params![
                sequence,
                event.input.binding.decision_id,
                event.attempt,
                event.input.observed_at_unix_seconds,
                serde_json::to_string(&event)?
            ],
        )?;
        tx.commit()?;
        if expired > 0 {
            scrub(&db)?;
        }
        Ok(event)
    }

    pub fn advisory_outcomes(&self, now: u64) -> Result<Vec<AdvisoryOutcomeEvent>> {
        timestamp(now)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let consent = read_consent(&tx)?;
        ensure_clock(&tx, now)?;
        if now < consent.updated_at_unix_seconds {
            return Err(FeedbackError::Invalid("clock moved backwards"));
        }
        let removed = expire(&tx, &consent, now)?;
        let outcomes = read_outcomes(&tx)?;
        tx.commit()?;
        if removed > 0 {
            scrub(&db)?;
        }
        Ok(outcomes)
    }

    /// Followed versus overridden recommendations per task kind. Needs
    /// collection consent. Same read effects as `report`.
    pub fn advisory_report(&self, now: u64) -> Result<AdvisoryOutcomeReport> {
        timestamp(now)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        let consent = read_consent(&tx)?;
        if !consent.preferences.collection_enabled {
            return Err(FeedbackError::Consent);
        }
        ensure_clock(&tx, now)?;
        if now < consent.updated_at_unix_seconds {
            return Err(FeedbackError::Invalid("clock moved backwards"));
        }
        let removed = expire(&tx, &consent, now)?;
        let outcomes = read_outcomes(&tx)?;
        tx.commit()?;
        if removed > 0 {
            scrub(&db)?;
        }
        build_report(&outcomes, now)
    }
}

/// Re-validates every row, like `read_events`.
pub(crate) fn read_outcomes(db: &Connection) -> Result<Vec<AdvisoryOutcomeEvent>> {
    let count: usize = db.query_row("SELECT COUNT(*) FROM advisory_outcomes", [], |r| r.get(0))?;
    if count > MAX_EVENTS {
        return Err(FeedbackError::Limit);
    }
    let mut query = db.prepare(
        "SELECT sequence,decision_id,attempt,observed_at,event FROM advisory_outcomes ORDER BY sequence",
    )?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
                r.get::<_, u64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(sequence, decision, attempt, observed, text)| {
            if text.len() > MAX_INPUT_BYTES + 1024 {
                return Err(FeedbackError::Limit);
            }
            let e: AdvisoryOutcomeEvent = serde_json::from_str(&text)?;
            validate_outcome(&e.input)?;
            if e.outcome_hash != outcome_hash(&e)?
                || sequence != e.sequence
                || e.attempt == 0
                || attempt != e.attempt
                || decision != e.input.binding.decision_id
                || observed != e.input.observed_at_unix_seconds
                || e.recorded_at_unix_seconds < observed
            {
                return Err(FeedbackError::Integrity("advisory outcome"));
            }
            Ok(e)
        })
        .collect()
}

/// Latest recording time, for the clock watermark.
pub(crate) fn latest_recorded_at(db: &Connection) -> Result<Option<u64>> {
    let text: Option<String> = db
        .query_row(
            "SELECT event FROM advisory_outcomes ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    text.map(|text| {
        serde_json::from_str::<AdvisoryOutcomeEvent>(&text)
            .map(|e| e.recorded_at_unix_seconds)
            .map_err(FeedbackError::from)
    })
    .transpose()
}

pub(crate) fn expire_outcomes(tx: &Transaction<'_>, cutoff: u64) -> Result<usize> {
    Ok(tx.execute(
        "DELETE FROM advisory_outcomes WHERE observed_at < ?1",
        [cutoff],
    )?)
}

/// A count over a sample, with its denominator. `rate_micros` is None when the
/// sample is empty: an empty sample has no rate, not a zero rate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryRate {
    pub numerator: u64,
    pub denominator: u64,
    pub rate_micros: Option<u32>,
}

impl AdvisoryRate {
    fn new(numerator: u64, denominator: u64) -> Self {
        // Round half up, like the other ratios.
        let rate_micros = (denominator > 0).then(|| {
            ((u128::from(numerator) * 1_000_000 + u128::from(denominator) / 2)
                / u128::from(denominator)) as u32
        });
        Self {
            numerator,
            denominator,
            rate_micros,
        }
    }
}

/// What the sample does not state. `used_*` apply to overridden outcomes only
/// and are null for followed ones. The result is a required field, so it is
/// never UNKNOWN and has no count.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AdvisoryUnknownCounts {
    pub cost: u64,
    pub tokens: u64,
    pub used_model: Option<u64>,
    pub used_effort: Option<u64>,
    pub used_skills: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AdvisoryBasisCounts {
    pub catalog_policy_prior: u64,
    pub partly_unrated: u64,
    pub unrated_tie_break: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryAdherenceStats {
    /// Decisions, each counted once with its latest record.
    pub sample_size: u64,
    /// All records of those decisions, corrections included.
    pub records: u64,
    pub accepted_as_is: AdvisoryRate,
    pub accepted_after_rework: AdvisoryRate,
    pub abandoned: AdvisoryRate,
    pub unknown: AdvisoryUnknownCounts,
    /// How the advice chose, for the recommendations in this sample.
    pub selection_basis: AdvisoryBasisCounts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryAdherencePair {
    pub followed: AdvisoryAdherenceStats,
    pub overridden: AdvisoryAdherenceStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryTaskKindReport {
    pub task_kind: TaskKind,
    #[serde(flatten)]
    pub adherence: AdvisoryAdherencePair,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryOutcomeReport {
    pub schema_version: u32,
    pub generated_at_unix_seconds: u64,
    pub outcome_set_hash: String,
    /// Decisions in the sample: one per `decision_id`, with its latest record.
    pub decisions: u64,
    /// Every retained record; `records - decisions` were superseded by a later attempt.
    pub records: u64,
    pub overall: AdvisoryAdherencePair,
    pub task_kinds: Vec<AdvisoryTaskKindReport>,
    pub attribution: &'static str,
    pub interpretation: &'static str,
    pub content_hash: String,
}

type Retained<'a> = BTreeMap<&'a str, u64>;

fn stats(
    outcomes: &[&AdvisoryOutcomeEvent],
    overridden: bool,
    retained: &Retained<'_>,
) -> AdvisoryAdherenceStats {
    let n = outcomes.len() as u64;
    let count = |result| outcomes.iter().filter(|e| e.input.result == result).count() as u64;
    let unknown = |missing: fn(&AdvisoryOutcomeEvent) -> bool| {
        overridden.then(|| outcomes.iter().filter(|e| missing(e)).count() as u64)
    };
    let basis = |basis| {
        outcomes
            .iter()
            .filter(|e| e.input.binding.selection_basis == basis)
            .count() as u64
    };
    AdvisoryAdherenceStats {
        sample_size: n,
        records: outcomes
            .iter()
            .map(|e| retained[e.input.binding.decision_id.as_str()])
            .sum(),
        accepted_as_is: AdvisoryRate::new(count(AdvisoryResult::AcceptedAsIs), n),
        accepted_after_rework: AdvisoryRate::new(count(AdvisoryResult::AcceptedAfterRework), n),
        abandoned: AdvisoryRate::new(count(AdvisoryResult::Abandoned), n),
        unknown: AdvisoryUnknownCounts {
            cost: outcomes
                .iter()
                .filter(|e| e.input.measurements.cost_microusd.is_none())
                .count() as u64,
            tokens: outcomes
                .iter()
                .filter(|e| e.input.measurements.tokens.is_none())
                .count() as u64,
            used_model: unknown(|e| e.input.used.as_ref().is_none_or(|u| u.model.is_none())),
            used_effort: unknown(|e| e.input.used.as_ref().is_none_or(|u| u.effort.is_none())),
            used_skills: unknown(|e| e.input.used.as_ref().is_none_or(|u| u.skills.is_none())),
        },
        selection_basis: AdvisoryBasisCounts {
            catalog_policy_prior: basis(AdvisorySelectionBasis::CatalogPolicyPrior),
            partly_unrated: basis(AdvisorySelectionBasis::PartlyUnrated),
            unrated_tie_break: basis(AdvisorySelectionBasis::UnratedTieBreak),
        },
    }
}

fn pair(outcomes: &[&AdvisoryOutcomeEvent], retained: &Retained<'_>) -> AdvisoryAdherencePair {
    let split = |adherence| {
        outcomes
            .iter()
            .copied()
            .filter(|e| e.input.adherence == adherence)
            .collect::<Vec<_>>()
    };
    AdvisoryAdherencePair {
        followed: stats(&split(AdvisoryAdherence::Followed), false, retained),
        overridden: stats(&split(AdvisoryAdherence::Overridden), true, retained),
    }
}

fn build_report(outcomes: &[AdvisoryOutcomeEvent], now: u64) -> Result<AdvisoryOutcomeReport> {
    // Records are in sequence order, so the last one of a decision is its latest.
    let mut latest = BTreeMap::<&str, &AdvisoryOutcomeEvent>::new();
    let mut retained = Retained::new();
    for event in outcomes {
        *retained
            .entry(&event.input.binding.decision_id)
            .or_default() += 1;
        latest.insert(&event.input.binding.decision_id, event);
    }
    let mut latest: Vec<_> = latest.into_values().collect();
    latest.sort_by_key(|event| event.sequence);
    let mut by_kind = BTreeMap::<TaskKind, Vec<&AdvisoryOutcomeEvent>>::new();
    for event in latest.iter().copied() {
        by_kind
            .entry(event.input.binding.task_kind)
            .or_default()
            .push(event);
    }
    let all = latest.clone();
    let mut report = AdvisoryOutcomeReport {
        schema_version: 1,
        generated_at_unix_seconds: now,
        outcome_set_hash: hash(
            "advisory-outcome-set-v1",
            &outcomes.iter().map(|e| &e.outcome_hash).collect::<Vec<_>>(),
        )?,
        decisions: latest.len() as u64,
        records: outcomes.len() as u64,
        overall: pair(&all, &retained),
        task_kinds: by_kind
            .into_iter()
            .map(|(task_kind, events)| AdvisoryTaskKindReport {
                task_kind,
                adherence: pair(&events, &retained),
            })
            .collect(),
        attribution: "not_established",
        interpretation: "personal_self_selected_sample_not_evidence_of_a_general_gain",
        content_hash: String::new(),
    };
    report.content_hash = hash("advisory-outcome-report-v1", &report)?;
    Ok(report)
}
