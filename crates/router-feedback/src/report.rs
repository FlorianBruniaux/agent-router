//! `feedback report`: descriptive outcome measurement per model, effort and
//! task kind. Read-only and deterministic: every list is sorted and every
//! figure is an integer or fixed-precision micros. It shows associations, not
//! a gain attributable to the router (that needs the paired A/B protocol).
use std::collections::{BTreeMap, BTreeSet};

use router_eval::{PricingTable, TokenPrice};
use router_types::{
    FeedbackEvent, FeedbackFact, FeedbackFactEvent, FeedbackFactKind, FeedbackOrigin, HostUsage,
    IntegrationOutcome, VerificationOutcome,
};
use serde::Serialize;

use crate::{
    FeedbackError, FeedbackStore, Result,
    beta::jeffreys_interval_micros,
    facts::read_facts,
    hash,
    learning::event_set_hash,
    store::{read_consent, scrub, snapshot_in, timestamp},
};

/// Percentile method: nearest rank on the sorted known values.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Percentiles {
    pub samples: u64,
    pub missing: u64,
    pub p50: Option<u64>,
    pub p90: Option<u64>,
}

fn percentiles(values: impl Iterator<Item = Option<u64>>) -> Percentiles {
    let mut known = Vec::new();
    let mut missing = 0;
    for value in values {
        match value {
            Some(v) => known.push(v),
            None => missing += 1,
        }
    }
    known.sort_unstable();
    let rank =
        |p: usize| (!known.is_empty()).then(|| known[(known.len() * p).div_ceil(100).max(1) - 1]);
    Percentiles {
        samples: known.len() as u64,
        missing,
        p50: rank(50),
        p90: rank(90),
    }
}

/// `numerator / denominator` in micros, rounded half up.
fn ratio_micros(numerator: u64, denominator: u64) -> Option<u64> {
    (denominator > 0).then(|| {
        ((u128::from(numerator) * 1_000_000 + u128::from(denominator) / 2)
            / u128::from(denominator)) as u64
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptedRate {
    pub accepted: u64,
    pub rejected: u64,
    pub point_micros: u64,
    pub lower_micros: u64,
    pub upper_micros: u64,
    pub level_micros: u32,
    pub prior: &'static str,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct HumanCounts {
    pub passed: u64,
    pub failed: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct IntegrationCounts {
    pub applied: u64,
    pub discarded: u64,
    /// Runs with no integration fact.
    pub unknown: u64,
    /// `applied / (applied + discarded)`; null without a decided run.
    pub rate_micros: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct GroupCost {
    /// Runs priced from a stored usage split and a table entry.
    pub priced_runs: u64,
    /// Runs without complete usage, or whose model is not in the table.
    pub cost_unknown: u64,
    /// Priced runs with a declared cache write count above zero. Cache writes
    /// are priced at the input rate because the table has no write price.
    pub runs_with_cache_writes_priced_as_input: u64,
    /// Sum over priced runs only.
    pub total_microusd: Option<u64>,
    /// Total over all runs divided by accepted tasks; null unless every run is priced.
    pub per_accepted_microusd: Option<u64>,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GroupReport {
    pub model: String,
    pub effort: String,
    pub task_kind: String,
    pub runs: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub unknown: u64,
    /// Runs without a run key; they are counted in `unknown`.
    pub unlinked: u64,
    pub human: HumanCounts,
    pub accepted_rate: Option<AcceptedRate>,
    pub integration: IntegrationCounts,
    pub tokens: Percentiles,
    pub latency_ms: Percentiles,
    pub attempts: Percentiles,
    /// Accepted runs only: applied fact time minus run start, in seconds.
    pub time_to_acceptance_seconds: Percentiles,
    pub cost: GroupCost,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PricingCitation {
    pub version: String,
    pub source: String,
    pub observed_at_unix_seconds: u64,
}

/// No value of this type exists: the figure it types is always null.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Never {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NullFigure {
    pub value: Option<Never>,
    pub reason: &'static str,
}

/// Never a policy change. `suggestion` has no constructible value: report data
/// alone cannot express a sound weight delta (see docs/feedback.md).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Advisory {
    Disabled {
        advisory: Option<bool>,
        reason: &'static str,
    },
    Enabled {
        advisory: bool,
        suggestion: Option<Never>,
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FeedbackReport {
    pub schema_version: u32,
    pub generated_at_unix_seconds: u64,
    pub event_set_hash: String,
    pub fact_set_hash: String,
    pub automated_events: u64,
    pub human_events: u64,
    pub facts: u64,
    /// Automated events without a run key: no late fact can refer to them.
    pub unlinked: u64,
    /// Facts whose run key matches no retained automated event.
    pub orphan_facts: u64,
    /// Automated events left out because a later event holds the same run key.
    /// `automated_events` and the groups count each run key once.
    pub superseded_events: u64,
    pub pricing: Option<PricingCitation>,
    pub groups: Vec<GroupReport>,
    pub catalog_estimate_ratio: NullFigure,
    pub advisory: Advisory,
    pub interpretation: &'static str,
    pub content_hash: String,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Verdict {
    Accepted,
    Rejected,
    Unknown,
}

#[derive(Default)]
struct RunFacts<'a> {
    verification: Option<&'a FeedbackFact>,
    integration: Option<&'a FeedbackFact>,
}

impl RunFacts<'_> {
    fn integration_outcome(&self) -> Option<IntegrationOutcome> {
        match self.integration.map(|f| &f.kind) {
            Some(FeedbackFactKind::Integration { outcome }) => Some(*outcome),
            _ => None,
        }
    }
    fn objective(&self) -> Option<VerificationOutcome> {
        match self.verification.map(|f| &f.kind) {
            Some(FeedbackFactKind::Verification { objective, .. }) => Some(*objective),
            _ => None,
        }
    }
    /// A rejection signal wins over acceptance. Anything else is unknown,
    /// never a failure.
    fn verdict(&self) -> Verdict {
        let integration = self.integration_outcome();
        let objective = self.objective();
        if integration == Some(IntegrationOutcome::Discarded)
            || objective == Some(VerificationOutcome::Failed)
        {
            Verdict::Rejected
        } else if integration == Some(IntegrationOutcome::Applied)
            && objective == Some(VerificationOutcome::Passed)
        {
            Verdict::Accepted
        } else {
            Verdict::Unknown
        }
    }
}

fn label<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        _ => Err(FeedbackError::Integrity("label")),
    }
}

fn valid_pricing(pricing: &PricingTable) -> bool {
    let text = |v: &str| !v.trim().is_empty() && v.len() <= 512 && !v.chars().any(char::is_control);
    pricing.schema_version == 1
        && text(&pricing.version)
        && text(&pricing.source)
        && pricing.models.len() <= 64
}

/// Same formula as `router_eval::price_usage`: uncached input, cached input and
/// output, rounded up to a micro-dollar per run. Reasoning tokens are part of
/// output and cache writes part of input, so neither is priced twice; the table
/// has no cache write price, so writes are priced as input. A field the host
/// did not declare makes the run unknown, never zero.
fn run_cost(usage: &HostUsage, price: &TokenPrice) -> Option<u128> {
    let input = usage.input_tokens.reported()?;
    let cached = usage.cached_input_tokens.reported()?;
    let output = usage.output_tokens.reported()?;
    if cached > input {
        return None;
    }
    let numerator = u128::from(input - cached) * u128::from(price.input_microusd_per_million)
        + u128::from(cached) * u128::from(price.cached_input_microusd_per_million)
        + u128::from(output) * u128::from(price.output_microusd_per_million);
    Some(numerator.div_ceil(1_000_000))
}

#[derive(Default)]
struct Accumulator<'a> {
    events: Vec<&'a FeedbackEvent>,
    accepted: u64,
    rejected: u64,
    unknown: u64,
    unlinked: u64,
    human: HumanCounts,
    integration: IntegrationCounts,
    acceptance_seconds: Vec<Option<u64>>,
    priced: u64,
    cost_unknown: u64,
    cache_write_runs: u64,
    cost_total: u128,
}

pub(crate) fn build_report(
    events: &[FeedbackEvent],
    fact_events: &[FeedbackFactEvent],
    pricing: Option<&PricingTable>,
    learning_enabled: bool,
    now: u64,
) -> Result<FeedbackReport> {
    if pricing.is_some_and(|p| !valid_pricing(p)) {
        return Err(FeedbackError::Invalid("pricing"));
    }
    let mut by_key: BTreeMap<&str, RunFacts<'_>> = BTreeMap::new();
    for event in fact_events {
        let entry = by_key.entry(&event.fact.run_key_sha256).or_default();
        match event.fact.kind {
            FeedbackFactKind::Verification { .. } => entry.verification = Some(&event.fact),
            FeedbackFactKind::Integration { .. } => entry.integration = Some(&event.fact),
        }
    }
    let mut groups: BTreeMap<(String, String, String), Accumulator<'_>> = BTreeMap::new();
    let mut automated_keys = BTreeSet::new();
    let (mut automated, mut human, mut unlinked) = (0u64, 0u64, 0u64);
    let none = RunFacts::default();
    // `run_hash` covers state and attempts, so one run can hold several
    // automated events. Late facts belong to the run, not to an event: count
    // each run key once, on its latest event.
    let mut latest: BTreeMap<&str, u64> = BTreeMap::new();
    for event in events {
        if let (FeedbackOrigin::Automated, Some(run_key)) =
            (event.input.origin, event.input.run_key_sha256.as_deref())
        {
            let entry = latest.entry(run_key).or_default();
            *entry = (*entry).max(event.sequence);
        }
    }
    let mut superseded = 0u64;
    for event in events {
        let i = &event.input;
        if i.origin == FeedbackOrigin::Automated
            && i.run_key_sha256
                .as_deref()
                .is_some_and(|run_key| latest.get(run_key) != Some(&event.sequence))
        {
            superseded += 1;
            continue;
        }
        let key = (
            i.context.model.to_string(),
            label(&i.context.effort)?,
            label(&i.context.task_kind)?,
        );
        let group = groups.entry(key).or_default();
        match i.origin {
            FeedbackOrigin::Human => {
                human += 1;
                match i.human.as_ref().map(|h| h.success) {
                    Some(VerificationOutcome::Passed) => group.human.passed += 1,
                    Some(VerificationOutcome::Failed) => group.human.failed += 1,
                    _ => group.human.unknown += 1,
                }
            }
            FeedbackOrigin::Automated => {
                automated += 1;
                group.events.push(event);
                let facts = match i.run_key_sha256.as_deref() {
                    Some(run_key) => {
                        automated_keys.insert(run_key);
                        by_key.get(run_key).unwrap_or(&none)
                    }
                    None => {
                        unlinked += 1;
                        group.unlinked += 1;
                        &none
                    }
                };
                match facts.verdict() {
                    Verdict::Accepted => {
                        group.accepted += 1;
                        let applied = facts.integration.map(|f| f.observed_at_unix_seconds);
                        group.acceptance_seconds.push(
                            i.started_at_unix_seconds
                                .zip(applied)
                                .and_then(|(start, end)| end.checked_sub(start)),
                        );
                    }
                    Verdict::Rejected => group.rejected += 1,
                    Verdict::Unknown => group.unknown += 1,
                }
                match facts.integration_outcome() {
                    Some(IntegrationOutcome::Applied) => group.integration.applied += 1,
                    Some(IntegrationOutcome::Discarded) => group.integration.discarded += 1,
                    None => group.integration.unknown += 1,
                }
                let cost = pricing
                    .and_then(|p| p.models.get(&i.context.model))
                    .zip(i.measurements.usage.as_ref())
                    .and_then(|(price, usage)| run_cost(usage, price));
                match cost {
                    Some(cost) => {
                        group.priced += 1;
                        group.cache_write_runs += u64::from(
                            i.measurements
                                .usage
                                .as_ref()
                                .and_then(|u| u.cache_write_input_tokens.reported())
                                .is_some_and(|w| w > 0),
                        );
                        group.cost_total += cost;
                    }
                    None => group.cost_unknown += 1,
                }
            }
        }
    }
    let orphan_facts = fact_events
        .iter()
        .filter(|f| !automated_keys.contains(f.fact.run_key_sha256.as_str()))
        .count() as u64;
    let groups = groups
        .into_iter()
        .map(|((model, effort, task_kind), a)| {
            let decided = a.accepted + a.rejected;
            let (lower, upper) = jeffreys_interval_micros(a.accepted, a.rejected);
            let mut integration = a.integration.clone();
            integration.rate_micros = ratio_micros(
                integration.applied,
                integration.applied + integration.discarded,
            );
            let cost = group_cost(pricing, &a);
            GroupReport {
                model,
                effort,
                task_kind,
                runs: a.events.len() as u64,
                accepted: a.accepted,
                rejected: a.rejected,
                unknown: a.unknown,
                unlinked: a.unlinked,
                human: a.human,
                accepted_rate: (decided > 0).then(|| AcceptedRate {
                    accepted: a.accepted,
                    rejected: a.rejected,
                    point_micros: ratio_micros(a.accepted, decided).unwrap_or(0),
                    lower_micros: lower,
                    upper_micros: upper,
                    level_micros: 950_000,
                    prior: "jeffreys",
                }),
                integration,
                tokens: percentiles(a.events.iter().map(|e| e.input.measurements.tokens)),
                latency_ms: percentiles(a.events.iter().map(|e| e.input.measurements.latency_ms)),
                attempts: percentiles(
                    a.events
                        .iter()
                        .map(|e| Some(u64::from(e.input.measurements.attempts))),
                ),
                time_to_acceptance_seconds: percentiles(a.acceptance_seconds.into_iter()),
                cost,
            }
        })
        .collect();
    let mut report = FeedbackReport {
        schema_version: 1,
        generated_at_unix_seconds: now,
        event_set_hash: event_set_hash(events)?,
        fact_set_hash: hash(
            "feedback-fact-set-v1",
            &fact_events.iter().map(|f| &f.fact_hash).collect::<Vec<_>>(),
        )?,
        automated_events: automated,
        human_events: human,
        facts: fact_events.len() as u64,
        unlinked,
        orphan_facts,
        superseded_events: superseded,
        pricing: pricing.map(|p| PricingCitation {
            version: p.version.clone(),
            source: p.source.clone(),
            observed_at_unix_seconds: p.observed_at_unix_seconds,
        }),
        groups,
        catalog_estimate_ratio: NullFigure {
            value: None,
            reason: "no_catalog_estimate_stored_in_feedback",
        },
        advisory: if learning_enabled {
            Advisory::Enabled {
                advisory: true,
                suggestion: None,
                reason: "no_sound_weight_suggestion_from_report_data",
            }
        } else {
            Advisory::Disabled {
                advisory: None,
                reason: "learning_disabled",
            }
        },
        interpretation: "descriptive_associations_not_causal_or_router_gain",
        content_hash: String::new(),
    };
    report.content_hash = hash("feedback-report-v1", &report)?;
    Ok(report)
}

fn group_cost(pricing: Option<&PricingTable>, a: &Accumulator<'_>) -> GroupCost {
    let mut cost = GroupCost {
        priced_runs: a.priced,
        cost_unknown: a.cost_unknown,
        runs_with_cache_writes_priced_as_input: a.cache_write_runs,
        ..GroupCost::default()
    };
    if pricing.is_none() {
        cost.reason = Some("no_pricing_table");
        return cost;
    }
    if a.priced == 0 {
        cost.reason = Some("no_priced_runs");
        return cost;
    }
    let Ok(total) = u64::try_from(a.cost_total) else {
        cost.reason = Some("cost_overflow");
        return cost;
    };
    cost.total_microusd = Some(total);
    if a.cost_unknown > 0 {
        cost.reason = Some("cost_unknown_runs");
    } else if a.accepted == 0 {
        cost.reason = Some("no_accepted_task");
    } else {
        // Round half up, like the other ratios.
        cost.per_accepted_microusd = Some(
            ((u128::from(total) + u128::from(a.accepted) / 2) / u128::from(a.accepted)) as u64,
        );
    }
    cost
}

impl FeedbackStore {
    /// Needs collection consent; a table is optional. Reads like the other
    /// readers: it may expire retained rows and advance the clock watermark,
    /// and writes nothing else. The advisory part needs learning consent.
    pub fn report(&self, pricing: Option<&PricingTable>, now: u64) -> Result<FeedbackReport> {
        timestamp(now)?;
        let mut db = self.lock()?;
        let tx = db.transaction()?;
        if !read_consent(&tx)?.preferences.collection_enabled {
            return Err(FeedbackError::Consent);
        }
        let (events, consent, removed) = snapshot_in(&tx, now, false)?;
        let facts = read_facts(&tx)?;
        tx.commit()?;
        if removed > 0 {
            scrub(&db)?;
        }
        build_report(
            &events,
            &facts,
            pricing,
            consent.preferences.learning_enabled,
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank_and_count_missing_values() {
        let ten = (1..=10).map(|v| Some(v * 10));
        let p = percentiles(ten.chain([None, None]));
        assert_eq!(
            p,
            Percentiles {
                samples: 10,
                missing: 2,
                p50: Some(50),
                p90: Some(90)
            }
        );
        let one = percentiles([Some(7)].into_iter());
        assert_eq!((one.p50, one.p90), (Some(7), Some(7)));
        // Order of arrival does not matter.
        let shuffled = percentiles([Some(30), Some(10), Some(20)].into_iter());
        assert_eq!((shuffled.p50, shuffled.p90), (Some(20), Some(30)));
        assert_eq!(percentiles([None].into_iter()).p50, None);
    }

    #[test]
    fn ratios_round_half_up_in_micros() {
        assert_eq!(ratio_micros(1, 3), Some(333_333));
        assert_eq!(ratio_micros(2, 3), Some(666_667));
        assert_eq!(ratio_micros(1, 0), None);
    }
}
