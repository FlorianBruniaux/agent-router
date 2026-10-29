//! Opt-in v4: a bounded chronological view built from the same passive read
//! transaction as v3. Only a closed allowlist of kinds and numeric facts is
//! published; event detail, identifiers and paths never leave the store.
use super::observation::project_v3;
use super::*;
use crate::observation::*;
use router_types::AttemptEvidence;

use TimelinePhase::{
    Approval, Attempt, Cancellation, Cleanup, Other, Plan, Preparation, Terminal, Transport,
    Verification,
};

const PHASES: [TimelinePhase; 10] = [
    Plan,
    Approval,
    Preparation,
    Attempt,
    Transport,
    Verification,
    Cancellation,
    Cleanup,
    Terminal,
    Other,
];

/// Journal kind, published label, phase. `transition` is classified by state.
const KINDS: &[(&str, &str, TimelinePhase)] = &[
    ("planned", "planned", Plan),
    ("artifact.declared", "artifact_declared", Plan),
    ("capability.evidence", "capability_evidence", Preparation),
    (
        "native.version_validated",
        "native_version_validated",
        Preparation,
    ),
    ("assets.final_gate", "assets_final_gate", Preparation),
    ("preparation.cycle.v1", "preparation_cycle", Preparation),
    (
        "preparation.git_returned",
        "preparation_git_returned",
        Preparation,
    ),
    ("admission.intent.v1", "admission_intent", Preparation),
    ("queued", "queued", Preparation),
    (
        "retry.awaiting_fresh_approval",
        "retry_awaiting_fresh_approval",
        Approval,
    ),
    ("attempt.started", "attempt_started", Attempt),
    ("attempt.evidence.v1", "attempt_evidence", Attempt),
    ("attempt.bound.v1", "attempt_bound", Attempt),
    ("attempt.finished", "attempt_finished", Transport),
    ("attempt.timing.v1", "attempt_timing", Transport),
    ("output.incomplete", "output_incomplete", Transport),
    ("protocol.observed", "protocol_observed", Transport),
    (
        "failure.native_classified",
        "failure_native_classified",
        Transport,
    ),
    (
        "failure.classification_error",
        "failure_classification_error",
        Transport,
    ),
    (
        "verification.recorded",
        "verification_recorded",
        Verification,
    ),
    (
        "verification.attested.v1",
        "verification_attested",
        Verification,
    ),
    (
        "verification.legacy_snapshot.v1",
        "verification_legacy_snapshot",
        Verification,
    ),
    (
        "recovery.verification_required",
        "recovery_verification_required",
        Verification,
    ),
    ("control.cancel_requested", "cancel_requested", Cancellation),
    (
        "control.cancelled_before_start",
        "cancelled_before_start",
        Cancellation,
    ),
    ("process.cleanup_confirmed", "cleanup_confirmed", Cleanup),
    ("process.cleanup_unknown", "cleanup_unknown", Cleanup),
    (
        "preparation.cleanup_unknown",
        "preparation_cleanup_unknown",
        Cleanup,
    ),
    (
        "admission.release_failed",
        "admission_release_failed",
        Cleanup,
    ),
    ("finite_tools.stopped.v1", "finite_tools_stopped", Cleanup),
    (
        "finite_subscription.stopped.v1",
        "finite_subscription_stopped",
        Cleanup,
    ),
    ("artifact", "artifact", Other),
    ("artifacts.readonly_view", "artifacts_readonly_view", Other),
    ("integration.intent", "integration_intent", Other),
    ("integration.context.v1", "integration_context", Other),
    (
        "integration.resume_intent.v1",
        "integration_resume_intent",
        Other,
    ),
    ("integration.discarded", "integration_discarded", Other),
    ("integration.applied", "integration_applied", Other),
    ("integration.published", "integration_published", Other),
    ("escalation.claim.v1", "escalation_claim", Other),
    ("integrity.invalid_transition", "invalid_transition", Other),
];

/// Every published `kind` label, for contract tests that pin the JSON Schema.
pub fn timeline_kind_labels() -> Vec<&'static str> {
    let mut labels: Vec<_> = KINDS.iter().map(|(_, label, _)| *label).collect();
    labels.extend(["transition", "unclassified"]);
    labels
}

fn classify(event: &RuntimeEvent) -> (&'static str, TimelinePhase) {
    if event.kind == "transition" {
        let phase = match event.state {
            RunState::Planned => Plan,
            RunState::AwaitingApproval | RunState::Queued => Approval,
            RunState::Preparing => Preparation,
            RunState::Running => Attempt,
            RunState::SucceededTransport => Transport,
            RunState::Verifying | RunState::Verified | RunState::VerificationFailed => Verification,
            RunState::Failed | RunState::TimedOut | RunState::Blocked => Terminal,
            RunState::Cancelled => Cancellation,
        };
        return ("transition", phase);
    }
    KINDS
        .iter()
        .find(|(kind, _, _)| *kind == event.kind)
        .map_or(("unclassified", Other), |(_, label, phase)| (label, *phase))
}

/// Explicit version 4. Same passive contract as v3: no ownership, recovery,
/// migration, artifact read, retry or network access.
pub fn observe_run_v4(root: &Path, id: &str) -> Result<RunObservationV4> {
    if id.is_empty() || id.len() > 1024 {
        return Err(RuntimeError::Invalid("observation run id"));
    }
    let mut db = control_connection(root, false)?;
    let tx = db.transaction()?;
    let report = project_v4(&tx, id).map_err(|error| match error {
        RuntimeError::Json(_) => RuntimeError::Integrity("observation malformed timeline"),
        other => other,
    })?;
    tx.commit()?;
    Ok(report)
}

fn project_v4(db: &Connection, id: &str) -> Result<RunObservationV4> {
    // project_v3 admits every byte and verifies the whole chain first.
    let mut observation = project_v3(db, id)?;
    let mut query = db.prepare(&format!(
        "{EVENT_COLUMNS} WHERE run_id=?1 ORDER BY sequence"
    ))?;
    let mut rows = query.query([id])?;
    let mut events = Vec::new();
    while let Some(row) = rows.next()? {
        events.push(event_from_row(row)?);
    }
    let timeline = build(&observation, &events)?;
    observation.observation.summary.schema_version = 4;
    observation.observation.summary.snapshot_sha256.clear();
    let mut report = RunObservationV4 {
        observation,
        timeline,
    };
    report.observation.observation.summary.snapshot_sha256 = digest("run-observation-v4", &report)?;
    Ok(report)
}

fn build(observation: &RunObservationV3, events: &[RuntimeEvent]) -> Result<RunTimeline> {
    let summary = &observation.observation.summary;
    let classified: Vec<_> = events.iter().map(classify).collect();
    let seq_of = |kind: &str| events.iter().find(|e| e.kind == kind).map(|e| e.sequence);
    let last_of = |kinds: &[&str]| {
        events
            .iter()
            .rev()
            .find(|e| kinds.contains(&e.kind.as_str()))
            .map(|e| e.sequence)
    };
    let cancel_requested = seq_of("control.cancel_requested");

    let mut started = 0u32;
    let mut entries = Vec::with_capacity(events.len());
    for (event, (label, phase)) in events.iter().zip(&classified) {
        started += u32::from(event.kind == "attempt.started");
        entries.push(TimelineEntry {
            sequence: event.sequence,
            event_sha256: event.event_hash.clone(),
            state: event.state,
            phase: *phase,
            kind: label,
            attempt: (started > 0 && matches!(phase, Attempt | Transport)).then_some(started),
            after_cancel_request: cancel_requested.is_some_and(|c| event.sequence > c),
        });
    }
    let run_events = entries.len();
    let omitted = run_events.saturating_sub(TIMELINE_ENTRY_LIMIT);

    let phases = PHASES
        .iter()
        .map(|phase| {
            let mut own = entries.iter().filter(|e| e.phase == *phase);
            let first = own.next().map(|e| e.sequence);
            TimelinePhaseSummary {
                phase: *phase,
                events: entries.iter().filter(|e| e.phase == *phase).count() as u32,
                first_sequence: first,
                last_sequence: own.last().map(|e| e.sequence).or(first),
            }
        })
        .collect();

    // Only the latest approval cycle counts: a retry returns the run to
    // AwaitingApproval, so earlier grants no longer describe the current state.
    let cycle_start = events
        .iter()
        .enumerate()
        .rev()
        .find(|(index, e)| {
            e.state == RunState::AwaitingApproval
                && (*index == 0 || events[index - 1].state != RunState::AwaitingApproval)
        })
        .map(|(index, _)| index);
    let awaiting = cycle_start.map(|index| events[index].sequence);
    let granted = events[cycle_start.unwrap_or(0)..]
        .iter()
        .find(|e| {
            !matches!(
                e.state,
                RunState::Planned | RunState::AwaitingApproval | RunState::Cancelled
            )
        })
        .map(|e| e.sequence);
    let outcome = if granted.is_some() {
        TimelineApprovalOutcome::Granted
    } else if summary.known.state == RunState::Cancelled && awaiting.is_some() {
        TimelineApprovalOutcome::NotGranted
    } else if summary.known.state == RunState::AwaitingApproval {
        TimelineApprovalOutcome::Pending
    } else {
        TimelineApprovalOutcome::Unknown
    };

    let attempts = observation
        .observation
        .attempts
        .iter()
        .map(|attempt| {
            let evidence = attempt
                .sources
                .attempt_evidence_sha256
                .as_ref()
                .and_then(|hash| events.iter().find(|e| &e.event_hash == hash))
                .map(|e| serde_json::from_str::<AttemptEvidence>(&e.detail))
                .transpose()?;
            let (tokens, cost) = evidence.as_ref().map_or((None, None), |e| {
                (e.observed_tokens, e.observed_cost_microusd)
            });
            let finished = attempt
                .sources
                .finished_event_sha256
                .as_ref()
                .and_then(|hash| events.iter().find(|e| &e.event_hash == hash));
            Ok(TimelineAttempt {
                ordinal: attempt.ordinal,
                requested: attempt.requested.clone(),
                applied: TimelineAppliedSelection {
                    model: ObservationUnknown::Unknown,
                    effort: ObservationUnknown::Unknown,
                },
                usage: TimelineUsage {
                    tokens,
                    cost_microusd: cost,
                    status: match (tokens, cost) {
                        (Some(_), Some(_)) => TimelineUsageStatus::RecordedUnattested,
                        (None, None) => TimelineUsageStatus::Unknown,
                        _ => TimelineUsageStatus::Partial,
                    },
                },
                recorded_cleanup: evidence.as_ref().map(|e| e.cleanup),
                result_after_cancel_request: finished
                    .map(|f| cancel_requested.is_some_and(|c| f.sequence > c)),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let last_sequence = events.last().map_or(0, |e| e.sequence);
    let unknown = ObservationUnknown::Unknown;
    Ok(RunTimeline {
        ordering: TimelineOrdering::JournalSequenceAscending,
        entry_limit: TIMELINE_ENTRY_LIMIT as u32,
        run_events: run_events as u32,
        shown: (run_events - omitted) as u32,
        omitted_earlier: omitted as u32,
        later_global_events: summary.horizon.sequence.saturating_sub(last_sequence),
        entries: entries.split_off(omitted),
        phases,
        approval: TimelineApproval {
            outcome,
            awaiting_sequence: awaiting,
            granted_sequence: granted,
        },
        cancellation: TimelineCancellation {
            requested_sequence: cancel_requested,
            before_start_sequence: seq_of("control.cancelled_before_start"),
            terminal_cancelled: summary.known.state == RunState::Cancelled,
            events_after_request: cancel_requested.map_or(0, |c| {
                events.iter().filter(|e| e.sequence > c).count() as u32
            }),
        },
        cleanup: TimelineCleanup {
            recorded: summary.known.cleanup,
            confirmed_sequence: last_of(&["process.cleanup_confirmed"]),
            unknown_sequence: last_of(&["process.cleanup_unknown"]),
        },
        verification: TimelineVerification {
            state: match summary.known.state {
                RunState::SucceededTransport => TimelineVerificationState::Required,
                RunState::Verifying => TimelineVerificationState::InProgress,
                RunState::Verified => TimelineVerificationState::Verified,
                RunState::VerificationFailed => TimelineVerificationState::Failed,
                _ => TimelineVerificationState::NotReached,
            },
            record_sequence: last_of(&[
                "verification.recorded",
                "verification.attested.v1",
                "verification.legacy_snapshot.v1",
            ]),
        },
        attempts,
        unknown: TimelineUnknowns {
            exact_revision_acceptance: unknown,
            notification_delivery: unknown,
            lost_notifications: unknown,
            event_timestamps: unknown,
            physical_cleanup: unknown,
            objective: unknown,
        },
    })
}
