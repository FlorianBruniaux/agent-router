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
    selection: &router_policy::ValidatedPhaseReceipt,
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
        let receipt = selection.receipt();
        if selection.context_hash() != context.context_hash()
            || completion.run.receipt_id != receipt.receipt_id
        {
            return Err(());
        }
        let selected = &selection.selection().candidate;
        let input = feedback_input(context, completion, receipt, selected, now)?;
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
fn feedback_input(
    context: &ValidatedPhaseContext,
    completion: &RunCompletion,
    receipt: &PhaseReceipt,
    selected: &PhaseCandidate,
    now: u64,
) -> Result<FeedbackInput, ()> {
    let failure = match completion.run.failure {
        Some(RunFailure::Authentication) => Some(FeedbackFailure::Authentication),
        Some(RunFailure::RateLimit) => Some(FeedbackFailure::RateLimit),
        Some(RunFailure::Timeout) => Some(FeedbackFailure::Timeout),
        Some(RunFailure::Crash) => Some(FeedbackFailure::Crash),
        Some(RunFailure::Network) => Some(FeedbackFailure::Network),
        Some(RunFailure::Policy) => Some(FeedbackFailure::Policy),
        Some(RunFailure::Verification) => Some(FeedbackFailure::Validation),
        Some(_) => Some(FeedbackFailure::Unknown),
        None if completion.run.state == RunState::Cancelled => Some(FeedbackFailure::Cancelled),
        None => None,
    };
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
            escalations: 0,
        },
        comment_sha256: None,
    })
}
