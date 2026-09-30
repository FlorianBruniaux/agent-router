//! Explicit opt-in runtime feedback. No prompt or raw output crosses this boundary.
use crate::CliError;
use router_adapter_api::content_digest;
use router_feedback::FeedbackStore;
use router_policy::ValidatedPhaseContext;
use router_types::*;
use std::path::Path;

pub(crate) fn consent(path: Option<&Path>) -> Result<Option<FeedbackConsent>, CliError> {
    path.map(|path| {
        if !path.join("feedback.sqlite3").is_file() {
            return Err(CliError::Product("feedback.store.unavailable"));
        }
        FeedbackStore::open(path)
            .and_then(|store| store.consent())
            .map_err(|_| CliError::Product("feedback.store.invalid"))
    })
    .transpose()
}

pub(crate) fn record(
    path: Option<&Path>,
    consent: Option<&FeedbackConsent>,
    selection: &router_execution_decision::ExecutionDecision,
    context: &ValidatedPhaseContext,
    completion: &RunCompletion,
) -> serde_json::Value {
    let (Some(path), Some(consent)) = (path, consent) else {
        return serde_json::json!({"recorded":false,"reason":"not_requested"});
    };
    if !consent.preferences.collection_enabled {
        return serde_json::json!({"recorded":false,"reason":"collection_disabled"});
    }
    let result = (|| {
        let now = crate::inspect::current_time(None).map_err(|_| ())?;
        let receipt = selection.baseline();
        if receipt.context_hash != context.context_hash()
            || completion.run.receipt_id != selection.authorization_id()
        {
            return Err(());
        }
        let selected = &selection.selection().candidate;
        let route = route_facts(receipt, selection.selection());
        // One at most: escalation sources are limited to deterministic or
        // activated runs (`escalation.rs` `source_in`), so a chain stops at 1.
        let escalations = u32::from(selection.origin() == ExecutionDecisionOrigin::Escalated);
        let mut input = feedback_input(
            context,
            completion,
            receipt,
            selected,
            route,
            escalations,
            now,
        )?;
        input.binding.receipt_hash = selection.authorization_id().into();
        if let Some(binding) = selection.binding() {
            input.binding.provenance_sha256 = digest(&(completion, receipt, binding))?;
        }
        let store = FeedbackStore::open(path).map_err(|_| ())?;
        if store.consent().map_err(|_| ())? != *consent {
            return Err(());
        }
        store.record(input, now).map_err(|_| ())
    })();
    match result {
        Ok(event) => serde_json::json!({"recorded":true,"event_hash":event.event_hash}),
        // Transport completion remains observable even if consent changed or
        // persistence failed. Never encourage a replay of the native operation.
        Err(()) => {
            serde_json::json!({"recorded":false,"reason":"consent_changed_or_storage_error"})
        }
    }
}

fn digest(value: &impl serde::Serialize) -> Result<String, ()> {
    serde_json::to_vec(value)
        .map(|bytes| content_digest(&bytes))
        .map_err(|_| ())
}
fn feedback_failure(failure: Option<RunFailure>, state: RunState) -> Option<FeedbackFailure> {
    match failure {
        Some(RunFailure::Authentication) => Some(FeedbackFailure::Authentication),
        Some(RunFailure::RateLimit) => Some(FeedbackFailure::RateLimit),
        Some(RunFailure::Timeout) => Some(FeedbackFailure::Timeout),
        Some(RunFailure::Crash) => Some(FeedbackFailure::Crash),
        Some(RunFailure::Network) => Some(FeedbackFailure::Network),
        Some(RunFailure::Policy) => Some(FeedbackFailure::Policy),
        Some(RunFailure::Verification) => Some(FeedbackFailure::Validation),
        // An unclassified outcome says nothing about the candidate model.
        Some(RunFailure::Unclassified) => Some(FeedbackFailure::Unknown),
        Some(_) => Some(FeedbackFailure::Unknown),
        None if state == RunState::Cancelled => Some(FeedbackFailure::Cancelled),
        None => None,
    }
}
/// Rank and margin come from the receipt's own ordering: the selected
/// candidate first, then its alternatives. Nothing here is recomputed.
fn route_facts(receipt: &PhaseReceipt, executed: &PhaseSelection) -> FeedbackRoute {
    let ranked: Vec<&PhaseSelection> = receipt
        .selected
        .iter()
        .chain(&receipt.alternatives)
        .collect();
    let position = ranked
        .iter()
        .position(|s| s.candidate.id == executed.candidate.id);
    let count = |others: usize| u32::try_from(others).unwrap_or(u32::MAX);
    FeedbackRoute {
        candidate_id: executed.candidate.id.clone(),
        rank: position.map(|i| count(i + 1)),
        margin_micros: position.and_then(|i| {
            let next = ranked.get(i + 1)?;
            ranked[i]
                .score
                .utility_micros
                .checked_sub(next.score.utility_micros)
        }),
        alternatives: count(ranked.len().saturating_sub(usize::from(position.is_some()))),
        abstention: receipt.abstention,
    }
}
fn feedback_input(
    context: &ValidatedPhaseContext,
    completion: &RunCompletion,
    receipt: &PhaseReceipt,
    selected: &PhaseCandidate,
    route: FeedbackRoute,
    escalations: u32,
    now: u64,
) -> Result<FeedbackInput, ()> {
    let failure = feedback_failure(completion.run.failure, completion.run.state);
    Ok(FeedbackInput {
        schema_version: 1,
        observed_at_unix_seconds: now,
        binding: FeedbackBinding {
            task_hash: digest(&context.request().task)?,
            run_hash: digest(&completion.run)?,
            receipt_hash: receipt.receipt_id.clone(),
            policy_hash: receipt.policy_hash.clone(),
            catalog_hash: receipt.catalog_hash.clone(),
            snapshot_hash: receipt.snapshot_hash.clone(),
            producer_version: env!("CARGO_PKG_VERSION").into(),
            provenance_sha256: digest(completion)?,
        },
        context: FeedbackContext {
            task_kind: context.request().task.kind,
            risk: context.request().task.risk,
            model: selected.model.clone(),
            effort: selected.effort,
            harness: context.snapshot().host.harness.clone(),
            transport: "subprocess-jsonl".into(),
            agent: selected.agent.clone(),
            skills: selected.skills.iter().map(ToString::to_string).collect(),
            plan: completion.run.plan_id.clone(),
            propensity_micros: None,
            route: Some(route),
        },
        origin: FeedbackOrigin::Automated,
        transport: if completion.transport.exit_code == Some(0)
            && completion.transport.termination == TransportTermination::Exited
            && completion.run.failure.is_none()
        {
            VerificationOutcome::Passed
        } else {
            VerificationOutcome::Failed
        },
        // A successful process says nothing about validation or the objective.
        validation: VerificationOutcome::Unknown,
        objective: VerificationOutcome::Unknown,
        verification_evidence_sha256: None,
        human: None,
        failure,
        measurements: FeedbackMeasurements {
            cost_microusd: None,
            latency_ms: Some(completion.transport.elapsed_ms),
            tokens: None,
            attempts: completion.run.attempts,
            escalations,
        },
        comment_sha256: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn selection(id: &str, utility_micros: i64) -> PhaseSelection {
        PhaseSelection {
            candidate: PhaseCandidate {
                id: id.into(),
                model: ModelId::parse("fixture-model").unwrap(),
                effort: EffortLevel::Medium,
                agent: None,
                skills: BTreeSet::new(),
                tools: BTreeSet::new(),
                tool_policy: PhaseToolPolicy::Restricted,
                sandbox: PhaseSandbox::ReadOnly,
                verification: PhaseVerification::Required,
                required_capabilities: BTreeSet::new(),
                required_mutations: BTreeSet::new(),
                estimated_cost_microusd: 1,
                estimated_tokens: 1,
                estimated_elapsed_ms: 1,
                relevance_micros: 0,
            },
            score: PhaseScore {
                quality_micros: 0,
                relevance_micros: 0,
                cost_penalty_micros: 0,
                latency_penalty_micros: 0,
                utility_micros,
            },
            assets: Vec::new(),
            reasons: Vec::new(),
        }
    }
    fn receipt(
        selected: Option<PhaseSelection>,
        alternatives: Vec<PhaseSelection>,
    ) -> PhaseReceipt {
        PhaseReceipt {
            schema_version: 2,
            receipt_id: "r".into(),
            phase: StageKind::Execute,
            context_hash: String::new(),
            snapshot_hash: String::new(),
            catalog_hash: String::new(),
            policy_hash: String::new(),
            pool_hash: String::new(),
            registry_hash: String::new(),
            abstention: selected.is_none().then_some(PhaseAbstention::Ambiguous),
            selected,
            alternatives,
            exclusions: Vec::new(),
            unavailable: BTreeSet::new(),
        }
    }

    #[test]
    fn route_facts_match_the_receipt_ranking() {
        let receipt = receipt(
            Some(selection("a", 900_000)),
            vec![selection("b", 700_000), selection("c", 650_000)],
        );
        let top = route_facts(&receipt, &selection("a", 900_000));
        assert_eq!(
            (
                top.rank,
                top.margin_micros,
                top.alternatives,
                top.abstention
            ),
            (Some(1), Some(200_000), 2, None)
        );
        let last = route_facts(&receipt, &selection("c", 650_000));
        assert_eq!(
            (last.rank, last.margin_micros, last.alternatives),
            (Some(3), None, 2)
        );
    }

    #[test]
    fn a_preferred_model_ranked_first_gives_a_negative_margin() {
        let receipt = receipt(
            Some(selection("preferred", 500_000)),
            vec![selection("better", 700_000)],
        );
        let facts = route_facts(&receipt, &selection("preferred", 500_000));
        assert_eq!((facts.rank, facts.margin_micros), (Some(1), Some(-200_000)));
    }

    #[test]
    fn route_facts_of_an_overridden_abstention_are_counted_and_unranked_is_unknown() {
        let receipt = receipt(None, vec![selection("b", 700_000), selection("c", 690_000)]);
        let facts = route_facts(&receipt, &selection("b", 700_000));
        assert_eq!(
            (facts.rank, facts.margin_micros, facts.abstention),
            (Some(1), Some(10_000), Some(PhaseAbstention::Ambiguous))
        );
        let outside = route_facts(&receipt, &selection("z", 1));
        assert_eq!(
            (outside.rank, outside.margin_micros, outside.alternatives),
            (None, None, 2)
        );
    }

    #[test]
    fn unclassified_failure_is_unknown_feedback_never_a_model_fault() {
        assert!(matches!(
            feedback_failure(Some(RunFailure::Unclassified), RunState::Failed),
            Some(FeedbackFailure::Unknown)
        ));
        assert!(matches!(
            feedback_failure(Some(RunFailure::Verification), RunState::VerificationFailed),
            Some(FeedbackFailure::Validation)
        ));
    }
}
