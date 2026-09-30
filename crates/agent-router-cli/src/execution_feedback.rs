//! Explicit opt-in runtime feedback. No prompt or raw output crosses this boundary.
use crate::CliError;
use router_adapter_api::content_digest;
use router_feedback::{FeedbackError, FeedbackStore};
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
    started_at: Option<u64>,
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
        input.started_at_unix_seconds = started_at.filter(|started| *started <= now);
        input.run_key_sha256 = Some(run_key(&completion.run)?);
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

/// Stable across state changes: `run_hash` covers `state` and `attempts`, so a
/// later verification or integration cannot recompute it. The writer and the
/// late-fact paths both key the run through this one function.
pub(crate) fn run_key(run: &RunSummary) -> Result<String, ()> {
    digest(&(&run.run_id, &run.definition_hash, &run.approval_digest))
}

/// Records one late fact for a run, or says why not. Nothing here can fail the
/// command that already succeeded: a failure is only reported in the value.
pub(crate) fn record_fact(
    path: Option<&Path>,
    run: &RunSummary,
    kind: FeedbackFactKind,
) -> serde_json::Value {
    record_fact_at(path, run, kind, None)
}

/// A verification fact only once the runtime left `Verifying`: it can replace an
/// inconclusive record, and a stored fact is never rewritten. `resulting` is the
/// summary `record_verification` returned; `identity` gives the run key.
pub(crate) fn record_verification_fact(
    path: Option<&Path>,
    identity: &RunSummary,
    record: &VerificationRecord,
    resulting: &RunSummary,
) -> serde_json::Value {
    verification_fact_at(path, identity, record, resulting, None)
}
fn verification_fact_at(
    path: Option<&Path>,
    identity: &RunSummary,
    record: &VerificationRecord,
    resulting: &RunSummary,
    now: Option<u64>,
) -> serde_json::Value {
    if path.is_none() {
        return serde_json::json!({"recorded":false,"reason":"not_requested"});
    }
    if !matches!(
        resulting.state,
        RunState::Verified | RunState::VerificationFailed
    ) {
        return serde_json::json!({"recorded":false,"reason":"inconclusive"});
    }
    match verification_fact(record) {
        Ok(kind) => record_fact_at(path, identity, kind, now),
        Err(()) => {
            serde_json::json!({"recorded":false,"reason":"consent_changed_or_storage_error"})
        }
    }
}

/// `now` overrides the clock, for tests: `observed_at` is the recording time, so
/// only a repeat at the same second is the identical, idempotent fact.
fn record_fact_at(
    path: Option<&Path>,
    run: &RunSummary,
    kind: FeedbackFactKind,
    now: Option<u64>,
) -> serde_json::Value {
    let Some(path) = path else {
        return serde_json::json!({"recorded":false,"reason":"not_requested"});
    };
    if matches!(
        &kind,
        FeedbackFactKind::Verification {
            validation: VerificationOutcome::Unknown,
            objective: VerificationOutcome::Unknown,
            ..
        }
    ) {
        return serde_json::json!({"recorded":false,"reason":"inconclusive"});
    }
    let result = (|| {
        if !path.join("feedback.sqlite3").is_file() {
            return Err(false);
        }
        let store = FeedbackStore::open(path).map_err(|_| false)?;
        if !store
            .consent()
            .map_err(|_| false)?
            .preferences
            .collection_enabled
        {
            return Ok(None);
        }
        let now = crate::inspect::current_time(now).map_err(|_| false)?;
        let fact = FeedbackFact {
            schema_version: 1,
            run_key_sha256: run_key(run).map_err(|()| false)?,
            observed_at_unix_seconds: now,
            kind,
        };
        store.record_fact(fact, now).map(Some).map_err(|error| {
            matches!(
                error,
                FeedbackError::Invalid("fact binding" | "verification fact")
            )
        })
    })();
    match result {
        Ok(Some(event)) => serde_json::json!({"recorded":true,"fact_hash":event.fact_hash}),
        Ok(None) => serde_json::json!({"recorded":false,"reason":"collection_disabled"}),
        // The fact breaks the contract, for example a passed objective with a
        // failed validation. It is not a storage or consent problem.
        Err(true) => serde_json::json!({"recorded":false,"reason":"invalid_fact"}),
        Err(false) => {
            serde_json::json!({"recorded":false,"reason":"consent_changed_or_storage_error"})
        }
    }
}

/// The evidence digest covers the whole stored record, verifier included.
fn verification_fact(record: &VerificationRecord) -> Result<FeedbackFactKind, ()> {
    Ok(FeedbackFactKind::Verification {
        validation: record.validation,
        objective: record.objective,
        evidence_sha256: digest(record)?,
    })
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
/// Host-declared usage is a report input only, never an admission input.
fn measurements(completion: &RunCompletion, escalations: u32) -> FeedbackMeasurements {
    let tokens = completion
        .runtime_evidence
        .as_ref()
        .and_then(|evidence| evidence.host_usage.as_ref())
        .and_then(HostUsage::total_tokens);
    FeedbackMeasurements {
        cost_microusd: None,
        latency_ms: Some(completion.transport.elapsed_ms),
        tokens,
        attempts: completion.run.attempts,
        escalations,
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
        started_at_unix_seconds: None,
        run_key_sha256: None,
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
        measurements: measurements(completion, escalations),
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

    fn completion(host_usage: Option<HostUsage>) -> RunCompletion {
        RunCompletion {
            run: RunSummary {
                run_id: "run".into(),
                definition_hash: "d".into(),
                approval_digest: "a".into(),
                receipt_id: "r".into(),
                plan_id: "p".into(),
                node_id: "n".into(),
                state: RunState::Verifying,
                attempts: 1,
                failure: None,
                worktree: None,
                artifact_sha256: None,
                output_retention: Default::default(),
                workflow: None,
                artifact_contract_sha256: None,
            },
            transport: TransportOutcome {
                termination: TransportTermination::Exited,
                exit_code: Some(0),
                elapsed_ms: 5,
            },
            stdout_sha256: String::new(),
            stderr_sha256: String::new(),
            stdout_bytes: 0,
            stderr_bytes: 0,
            validation: VerificationOutcome::Unknown,
            objective: VerificationOutcome::Unknown,
            memory_enforcement: MemoryEnforcement::NotRequested,
            runtime_evidence: Some(RuntimeEvidence {
                schema_version: 1,
                transport: TransportOutcome {
                    termination: TransportTermination::Exited,
                    exit_code: Some(0),
                    elapsed_ms: 5,
                },
                transport_success: true,
                stream_complete: true,
                validation: VerificationOutcome::Unknown,
                objective: VerificationOutcome::Unknown,
                executed_models: vec![],
                executed_effort: None,
                events: vec![],
                missing_evidence: vec![],
                host_usage,
            }),
            output_artifacts: None,
        }
    }

    #[test]
    fn host_usage_becomes_total_tokens_and_absence_stays_unknown() {
        let usage = HostUsage {
            input_tokens: UsageField::host_reported(100),
            cached_input_tokens: UsageField::host_reported(40),
            cache_write_input_tokens: UsageField::UNKNOWN,
            output_tokens: UsageField::host_reported(20),
            reasoning_output_tokens: UsageField::host_reported(5),
        };
        let m = measurements(&completion(Some(usage)), 0);
        assert_eq!(m.tokens, Some(120));
        for absent in [
            None,
            Some(HostUsage {
                output_tokens: UsageField::UNKNOWN,
                ..usage
            }),
        ] {
            let m = measurements(&completion(absent), 0);
            assert_eq!(m.tokens, None);
        }
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
    fn run(state: RunState, attempts: u32) -> RunSummary {
        RunSummary {
            run_id: "run-1".into(),
            definition_hash: "d".repeat(64),
            approval_digest: "a".repeat(64),
            receipt_id: "receipt".into(),
            plan_id: "plan".into(),
            node_id: "node".into(),
            state,
            attempts,
            failure: None,
            worktree: None,
            artifact_sha256: None,
            output_retention: OutputRetentionPolicy::default(),
            workflow: None,
            artifact_contract_sha256: None,
        }
    }

    #[test]
    fn run_key_ignores_state_and_attempts_but_run_hash_does_not() {
        let at_completion = run(RunState::Verifying, 1);
        let later = run(RunState::Verified, 2);
        assert_eq!(run_key(&at_completion), run_key(&later));
        assert_ne!(digest(&at_completion), digest(&later));
        let mut other = run(RunState::Verifying, 1);
        other.approval_digest = "b".repeat(64);
        assert_ne!(run_key(&at_completion), run_key(&other));
        other = run(RunState::Verifying, 1);
        other.run_id = "run-2".into();
        assert_ne!(run_key(&at_completion), run_key(&other));
        other = run(RunState::Verifying, 1);
        other.definition_hash = "e".repeat(64);
        assert_ne!(run_key(&at_completion), run_key(&other));
    }

    const CONFIGURED: u64 = 1_000;
    const FACT_TIME: Option<u64> = Some(1_500);
    fn store(collection_enabled: bool) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("feedback");
        let store = FeedbackStore::open(&path).unwrap();
        store
            .configure(
                FeedbackPreferences {
                    collection_enabled,
                    ..FeedbackPreferences::default()
                },
                CONFIGURED,
            )
            .unwrap();
        (temp, path)
    }
    fn applied() -> FeedbackFactKind {
        FeedbackFactKind::Integration {
            outcome: IntegrationOutcome::Applied,
        }
    }

    #[test]
    fn late_fact_is_recorded_under_the_writer_run_key() {
        let (_temp, path) = store(true);
        let value = record_fact_at(
            Some(&path),
            &run(RunState::Verified, 2),
            applied(),
            FACT_TIME,
        );
        assert_eq!(value["recorded"], true);
        let facts = FeedbackStore::open(&path).unwrap().facts(1_500).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0].fact.run_key_sha256,
            run_key(&run(RunState::Verifying, 1)).unwrap()
        );
        assert_eq!(value["fact_hash"], facts[0].fact_hash);
        // A repeat is idempotent and reports the same fact.
        let again = record_fact_at(
            Some(&path),
            &run(RunState::Verified, 2),
            applied(),
            FACT_TIME,
        );
        assert_eq!(again, value);
        // The identity includes `observed_at`: the same outcome a second later is
        // a different fact, refused rather than rewritten.
        let later = record_fact_at(
            Some(&path),
            &run(RunState::Verified, 2),
            applied(),
            Some(1_501),
        );
        assert_eq!(later["reason"], "consent_changed_or_storage_error");
    }

    #[test]
    fn late_fact_reports_why_it_was_not_recorded() {
        let reason = |value: serde_json::Value| {
            assert_eq!(value["recorded"], false);
            value["reason"].as_str().unwrap().to_owned()
        };
        let summary = run(RunState::Verified, 1);
        assert_eq!(
            reason(record_fact_at(None, &summary, applied(), FACT_TIME)),
            "not_requested"
        );
        let (_temp, disabled) = store(false);
        assert_eq!(
            reason(record_fact_at(
                Some(&disabled),
                &summary,
                applied(),
                FACT_TIME
            )),
            "collection_disabled"
        );
        let missing = disabled.join("absent");
        assert_eq!(
            reason(record_fact_at(
                Some(&missing),
                &summary,
                applied(),
                FACT_TIME
            )),
            "consent_changed_or_storage_error"
        );
        assert!(!missing.exists());
        let (_temp, enabled) = store(true);
        let inconclusive = FeedbackFactKind::Verification {
            validation: VerificationOutcome::Unknown,
            objective: VerificationOutcome::Unknown,
            evidence_sha256: "9".repeat(64),
        };
        assert_eq!(
            reason(record_fact_at(
                Some(&enabled),
                &summary,
                inconclusive,
                FACT_TIME
            )),
            "inconclusive"
        );
        // A conflicting outcome is a storage refusal, never a command failure.
        record_fact_at(Some(&enabled), &summary, applied(), FACT_TIME);
        let conflict = FeedbackFactKind::Integration {
            outcome: IntegrationOutcome::Discarded,
        };
        assert_eq!(
            reason(record_fact_at(
                Some(&enabled),
                &summary,
                conflict,
                FACT_TIME
            )),
            "consent_changed_or_storage_error"
        );
    }

    #[test]
    fn verification_fact_carries_the_stored_outcomes_and_a_record_digest() {
        let record = VerificationRecord {
            verifier: "checker".into(),
            evidence_sha256: "1".repeat(64),
            artifact_sha256: None,
            validation: VerificationOutcome::Passed,
            objective: VerificationOutcome::Failed,
        };
        let FeedbackFactKind::Verification {
            validation,
            objective,
            evidence_sha256,
        } = verification_fact(&record).unwrap()
        else {
            panic!("verification fact");
        };
        assert_eq!(
            (validation, objective),
            (VerificationOutcome::Passed, VerificationOutcome::Failed)
        );
        assert_eq!(evidence_sha256, digest(&record).unwrap());
    }

    fn record(
        validation: VerificationOutcome,
        objective: VerificationOutcome,
    ) -> VerificationRecord {
        VerificationRecord {
            verifier: "checker".into(),
            evidence_sha256: "1".repeat(64),
            artifact_sha256: None,
            validation,
            objective,
        }
    }

    #[test]
    fn a_verification_fact_waits_for_a_conclusive_runtime_state() {
        use VerificationOutcome::{Failed, Passed, Unknown};
        let (_temp, path) = store(true);
        let identity = run(RunState::Verifying, 1);
        // The runtime keeps the run in `Verifying` and may replace this record.
        let first = verification_fact_at(
            Some(&path),
            &identity,
            &record(Passed, Unknown),
            &run(RunState::Verifying, 1),
            FACT_TIME,
        );
        assert_eq!(first["reason"], "inconclusive");
        let stored = || FeedbackStore::open(&path).unwrap().facts(1_500).unwrap();
        assert!(stored().is_empty());
        let second = verification_fact_at(
            Some(&path),
            &identity,
            &record(Passed, Passed),
            &run(RunState::Verified, 1),
            FACT_TIME,
        );
        assert_eq!(second["recorded"], true);
        let facts = stored();
        assert_eq!(facts.len(), 1);
        assert!(matches!(
            facts[0].fact.kind,
            FeedbackFactKind::Verification {
                validation: Passed,
                objective: Passed,
                ..
            }
        ));
        // A failure is conclusive too.
        let (_temp, path) = store(true);
        let failed = verification_fact_at(
            Some(&path),
            &identity,
            &record(Failed, Unknown),
            &run(RunState::VerificationFailed, 1),
            FACT_TIME,
        );
        assert_eq!(failed["recorded"], true);
    }

    #[test]
    fn a_contract_violating_verification_is_invalid_not_a_storage_error() {
        use VerificationOutcome::{Failed, Passed};
        let (_temp, path) = store(true);
        // Conclusive for the runtime (a failed validation), refused by the contract.
        let value = verification_fact_at(
            Some(&path),
            &run(RunState::Verifying, 1),
            &record(Failed, Passed),
            &run(RunState::VerificationFailed, 1),
            FACT_TIME,
        );
        assert_eq!(value["recorded"], false);
        assert_eq!(value["reason"], "invalid_fact");
        assert!(
            FeedbackStore::open(&path)
                .unwrap()
                .facts(1_500)
                .unwrap()
                .is_empty()
        );
    }
}
