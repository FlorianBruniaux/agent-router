// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Deterministic advisory findings derived from one verified v4 observation.
//! A pure function of its input: no store access, clock, network or action.
//! Every rule either emits one finding or one named suppression, so an absent
//! finding never silently means "healthy".
use crate::observation::*;
use router_types::{RunState, TransportTermination};
use serde::Serialize;

pub const FINDINGS_RULES_VERSION: u32 = 1;
const REPEATED_FAILURE_THRESHOLD: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingRule {
    RepeatedFailedAttempts,
    CleanupUnconfirmed,
    LateResultAfterCancel,
    AttemptEvidenceMissing,
    /// Declared so the dependency on qualified timing (#117) is visible.
    AttemptDuration,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
}
/// Where the supporting fact comes from. Only the journal is a source here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingBasis {
    RecordedJournal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    BelowThreshold,
    AttemptEvidenceIncomplete,
    CleanupRecorded,
    CleanupMarkerConfirmed,
    RunNotTerminal,
    RunInProgress,
    NoAttemptStarted,
    NoCancelRequest,
    OrdinaryCancellation,
    NoGap,
    TimingNotQualified,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomaticAction {
    None,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FindingEvidence {
    pub sequence: u64,
    pub event_sha256: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunFinding {
    /// Digest of the rule, run key and supporting event hashes. It does not
    /// depend on the journal horizon, so unrelated appends keep it stable.
    pub id: String,
    pub rule: FindingRule,
    pub severity: FindingSeverity,
    pub basis: FindingBasis,
    pub source_snapshot_sha256: String,
    pub fact: &'static str,
    pub hypothesis: &'static str,
    pub affected_attempts: Vec<u32>,
    pub evidence: Vec<FindingEvidence>,
    pub preserve: Vec<&'static str>,
    pub check: &'static str,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuppressedRule {
    pub rule: FindingRule,
    pub reason: SuppressionReason,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunFindings {
    pub rules_version: u32,
    /// The v4 snapshot these findings were derived from. A finding from an
    /// older snapshot is not evidence about a later one.
    pub source_snapshot_sha256: String,
    pub findings: Vec<RunFinding>,
    pub suppressed: Vec<SuppressedRule>,
    pub automatic_action: AutomaticAction,
}
/// Opt-in version 5: the v4 report plus advisory findings from that same report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunObservationV5 {
    #[serde(flatten)]
    pub observation: RunObservationV4,
    pub findings: RunFindings,
}

const PRESERVE_RETRY: &[&str] = &[
    "the original task requirements",
    "a fresh approval before any retry",
];
const PRESERVE_CLEANUP: &[&str] = &[
    "no reuse of the worktree or lease until cleanup is confirmed",
    "no release of any reservation on this evidence alone",
];
const PRESERVE_CANCEL: &[&str] = &[
    "the cancellation request as the authoritative intent",
    "no application of the late result without an explicit approval",
];
const PRESERVE_EVIDENCE: &[&str] = &[
    "the original task requirements",
    "the stored journal unchanged",
];

pub fn derive_findings(report: &RunObservationV4) -> crate::Result<RunFindings> {
    let source = report
        .observation
        .observation
        .summary
        .snapshot_sha256
        .clone();
    let summary = &report.observation.observation.summary;
    let attempts = &report.observation.observation.attempts;
    let timeline = &report.timeline;
    let events = &summary.known.events;
    let reference = |hash: &str| {
        events
            .iter()
            .find(|e| e.event_sha256 == hash)
            .map(|e| FindingEvidence {
                sequence: e.sequence,
                event_sha256: e.event_sha256.clone(),
            })
    };
    let state = summary.known.state;
    let mut findings = Vec::new();
    let mut suppressed = Vec::new();
    let mut emit = |rule: FindingRule,
                    severity: FindingSeverity,
                    fact: &'static str,
                    hypothesis: &'static str,
                    affected: Vec<u32>,
                    mut evidence: Vec<FindingEvidence>,
                    preserve: &[&'static str],
                    check: &'static str|
     -> crate::Result<()> {
        evidence.sort_by_key(|e| e.sequence);
        evidence.dedup();
        let id =
            crate::store::finding_digest(&(rule, &summary.identity.run_key_sha256, &evidence))?;
        findings.push(RunFinding {
            id,
            rule,
            severity,
            basis: FindingBasis::RecordedJournal,
            source_snapshot_sha256: source.clone(),
            fact,
            hypothesis,
            affected_attempts: affected,
            evidence,
            preserve: preserve.to_vec(),
            check,
        });
        Ok(())
    };

    // Rule 1: repeated failed attempts. Only attempts with validated evidence
    // count; a cancelled attempt or an unrecorded exit code is not a failure.
    let failed: Vec<_> = attempts
        .iter()
        .filter(|a| {
            a.transport.as_ref().is_some_and(|t| match t.termination {
                TransportTermination::Exited => t.exit_code.is_some_and(|code| code != 0),
                TransportTermination::TimedOut
                | TransportTermination::Killed
                | TransportTermination::SpawnFailed => true,
                TransportTermination::Cancelled => false,
            })
        })
        .collect();
    if failed.len() >= REPEATED_FAILURE_THRESHOLD {
        emit(
            FindingRule::RepeatedFailedAttempts,
            FindingSeverity::Warning,
            "Two or more attempts of this run ended without a recorded clean exit.",
            "The same cause may repeat across retries; the journal does not show the cause.",
            failed.iter().map(|a| a.ordinal).collect(),
            failed
                .iter()
                .filter_map(|a| {
                    a.sources
                        .finished_event_sha256
                        .as_deref()
                        .and_then(reference)
                })
                .collect(),
            PRESERVE_RETRY,
            "Compare termination and exit code of the listed attempts, change one input, and observe the next approved attempt again.",
        )?;
    } else {
        suppressed.push(SuppressedRule {
            rule: FindingRule::RepeatedFailedAttempts,
            reason: if attempts
                .iter()
                .any(|a| a.evidence == AttemptObservationEvidence::Unknown)
            {
                SuppressionReason::AttemptEvidenceIncomplete
            } else {
                SuppressionReason::BelowThreshold
            },
        });
    }

    // Rule 2: a finished run whose cleanup the journal never confirmed.
    let terminal = state.terminal();
    let cleanup_reason = if !terminal {
        Some(SuppressionReason::RunNotTerminal)
    } else if summary.known.attempts == 0 {
        Some(SuppressionReason::NoAttemptStarted)
    } else if match (
        timeline.cleanup.confirmed_sequence,
        timeline.cleanup.unknown_sequence,
    ) {
        // An older confirmation does not cover a later unknown marker.
        (Some(confirmed), Some(unknown)) => confirmed > unknown,
        (Some(_), None) => true,
        _ => false,
    } {
        Some(SuppressionReason::CleanupMarkerConfirmed)
    } else if summary.known.cleanup != router_types::ProcessCleanupEvidence::Unknown {
        Some(SuppressionReason::CleanupRecorded)
    } else {
        None
    };
    match cleanup_reason {
        Some(reason) => suppressed.push(SuppressedRule {
            rule: FindingRule::CleanupUnconfirmed,
            reason,
        }),
        None => emit(
            FindingRule::CleanupUnconfirmed,
            FindingSeverity::Warning,
            "The run is in a terminal state but its process cleanup is not confirmed in the journal.",
            "A supervised process or lease may remain; the absence of a marker proves neither outcome.",
            attempts.iter().map(|a| a.ordinal).collect(),
            events
                .last()
                .map(|e| FindingEvidence {
                    sequence: e.sequence,
                    event_sha256: e.event_sha256.clone(),
                })
                .into_iter()
                .chain(
                    timeline
                        .cleanup
                        .unknown_sequence
                        .and_then(|s| events.iter().find(|e| e.sequence == s))
                        .map(|e| FindingEvidence {
                            sequence: e.sequence,
                            event_sha256: e.event_sha256.clone(),
                        }),
                )
                .collect(),
            PRESERVE_CLEANUP,
            "Inspect the store with the read-only `agent-router doctor --runtime-root` and confirm no supervised process remains.",
        )?,
    }

    // Rule 3: a result recorded after the cancel request of its own approval
    // cycle. A supervisor that itself reports `cancelled` honoured the request.
    let cancel_seen = timeline.cancellation.requested_sequence.is_some()
        || timeline
            .attempts
            .iter()
            .any(|a| a.cancel_request_sequence.is_some());
    let late: Vec<_> = timeline
        .attempts
        .iter()
        .filter(|a| a.result_after_cancel_request == Some(true))
        .filter(|a| {
            attempts
                .iter()
                .find(|v2| v2.ordinal == a.ordinal)
                .and_then(|v2| v2.transport.as_ref())
                .is_some_and(|t| t.termination != TransportTermination::Cancelled)
        })
        .collect();
    if late.is_empty() {
        let after_request = timeline
            .attempts
            .iter()
            .any(|a| a.result_after_cancel_request == Some(true));
        suppressed.push(SuppressedRule {
            rule: FindingRule::LateResultAfterCancel,
            reason: if !cancel_seen {
                SuppressionReason::NoCancelRequest
            } else if after_request {
                SuppressionReason::OrdinaryCancellation
            } else {
                SuppressionReason::NoGap
            },
        });
    } else {
        let mut evidence = Vec::new();
        for attempt in &late {
            evidence.extend(
                attempt
                    .cancel_request_sequence
                    .and_then(|sequence| events.iter().find(|e| e.sequence == sequence))
                    .map(|e| FindingEvidence {
                        sequence: e.sequence,
                        event_sha256: e.event_sha256.clone(),
                    }),
            );
            evidence.extend(
                attempts
                    .iter()
                    .find(|a| a.ordinal == attempt.ordinal)
                    .and_then(|a| a.sources.finished_event_sha256.as_deref())
                    .and_then(reference),
            );
        }
        emit(
            FindingRule::LateResultAfterCancel,
            FindingSeverity::Info,
            "An attempt's finished marker was recorded after the cancel request and the supervisor did not report a cancellation.",
            "The result was not applied by this view; a recorded result does not mean the cancellation failed or the output is usable.",
            late.iter().map(|a| a.ordinal).collect(),
            evidence,
            PRESERVE_CANCEL,
            "Read the attempt's recorded termination and decide explicitly whether to discard the result; observe again after the owner exits.",
        )?;
    }

    // Rule 4: a started attempt with no durable outcome, once the run has stopped.
    // `observed_attempt` writes attempt.finished, the transition, attempt.evidence.v1
    // and attempt.timing.v1 in one IMMEDIATE transaction (store/escalation.rs:21-67),
    // so a committed journal never shows a finished marker without its evidence.
    // The only window is a run still Preparing or Running, which is suppressed.
    let missing: Vec<_> = attempts
        .iter()
        .filter(|a| a.evidence == AttemptObservationEvidence::Unknown)
        .collect();
    if missing.is_empty() {
        suppressed.push(SuppressedRule {
            rule: FindingRule::AttemptEvidenceMissing,
            reason: SuppressionReason::NoGap,
        });
    } else if matches!(state, RunState::Preparing | RunState::Running) {
        suppressed.push(SuppressedRule {
            rule: FindingRule::AttemptEvidenceMissing,
            reason: SuppressionReason::RunInProgress,
        });
    } else {
        emit(
            FindingRule::AttemptEvidenceMissing,
            FindingSeverity::Info,
            "An attempt started but no durable outcome evidence is bound to it.",
            "It may have been interrupted, recorded by an older format, or never finished; the journal does not say which.",
            missing.iter().map(|a| a.ordinal).collect(),
            missing
                .iter()
                .filter_map(|a| reference(&a.sources.started_event_sha256))
                .collect(),
            PRESERVE_EVIDENCE,
            "Run `agent-router observe` again after the owner has exited and compare the attempt rows; collect nothing from the model.",
        )?;
    }

    // Rule 5: duration. No qualified source exists for it in this contract.
    suppressed.push(SuppressedRule {
        rule: FindingRule::AttemptDuration,
        reason: SuppressionReason::TimingNotQualified,
    });

    Ok(RunFindings {
        rules_version: FINDINGS_RULES_VERSION,
        source_snapshot_sha256: source,
        findings,
        suppressed,
        automatic_action: AutomaticAction::None,
    })
}
