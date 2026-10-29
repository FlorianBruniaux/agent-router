// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Redacted projection of one coherent, bounded runtime-store read.
//! These are observation contracts, never executable authority.
use router_types::{ProcessCleanupEvidence, RunFailure, RunState};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservation {
    pub schema_version: u32,
    pub snapshot_sha256: String,
    pub identity: RunObservationIdentity,
    pub horizon: RunObservationHorizon,
    pub known: RunObservationKnown,
    pub unknown: RunObservationUnknowns,
    pub completeness: RunObservationCompleteness,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservationIdentity {
    pub run_key_sha256: String,
    pub definition_sha256: String,
    pub receipt_id_sha256: String,
    pub plan_id_sha256: String,
    pub node_id_sha256: String,
}
/// Horizon of the entire verified journal, including events for other runs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservationHorizon {
    pub sequence: u64,
    pub event_sha256: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservationKnown {
    /// Persisted runtime state, not independent process or objective evidence.
    pub state: RunState,
    pub attempts: u32,
    pub failure: Option<RunFailure>,
    /// Persisted cleanup scope only. No process or lease is probed on read.
    pub cleanup: ProcessCleanupEvidence,
    pub events: Vec<RunEventReference>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunEventReference {
    pub sequence: u64,
    pub state: RunState,
    pub event_sha256: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationUnknown {
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservationUnknowns {
    pub requested_model: ObservationUnknown,
    pub requested_effort: ObservationUnknown,
    pub observed_model: ObservationUnknown,
    pub observed_effort: ObservationUnknown,
    pub transport: ObservationUnknown,
    pub validation: ObservationUnknown,
    pub objective: ObservationUnknown,
    pub duration: ObservationUnknown,
    pub journal_authenticity: ObservationUnknown,
    pub process_liveness: ObservationUnknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationJournalCoverage {
    Complete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationDetailCoverage {
    Redacted,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservationCompleteness {
    /// Complete chain coverage through horizon. Hashes do not prove authenticity.
    pub journal: ObservationJournalCoverage,
    pub external_observations: ObservationUnknown,
    pub detail: ObservationDetailCoverage,
}

/// Opt-in attempt facts, projected in the same passive snapshot as the summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunObservationV2 {
    #[serde(flatten)]
    pub summary: RunObservation,
    pub attempts: Vec<RunAttemptObservation>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunAttemptObservation {
    pub ordinal: u32,
    pub attempt_key_sha256: String,
    pub evidence: AttemptObservationEvidence,
    pub requested: Option<AttemptObservationRequested>,
    pub transport: Option<AttemptObservationTransport>,
    pub sources: AttemptObservationSources,
    pub capture: AttemptObservationCapture,
    pub protocol: ObservationUnknown,
    pub native: AttemptObservationNative,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptObservationEvidence {
    Unknown,
    DurableAttempt,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptObservationRequested {
    pub model: router_types::ModelId,
    pub effort: router_types::EffortLevel,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptObservationTransport {
    pub termination: router_types::TransportTermination,
    pub exit_code: Option<i32>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptObservationSources {
    pub started_event_sha256: String,
    pub attempt_evidence_sha256: Option<String>,
    pub finished_event_sha256: Option<String>,
    pub transition_event_sha256: Option<String>,
    pub output_incomplete_event_sha256: Option<String>,
    /// Legacy protocol events have no ordinal binding and cannot qualify here.
    pub protocol_event_sha256: Option<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptObservationCapture {
    Unknown,
    Incomplete,
    OutputLimit,
    OutputLimitIncomplete,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptObservationNative {
    pub observed_model: ObservationUnknown,
    pub observed_effort: ObservationUnknown,
    pub thread_id: ObservationUnknown,
    pub turn_id: ObservationUnknown,
    pub usage: ObservationUnknown,
    pub notification_delivery: ObservationUnknown,
    pub lost_notifications: ObservationUnknown,
    pub duration: ObservationUnknown,
    pub validation: ObservationUnknown,
    pub objective: ObservationUnknown,
}

/// Explicit opt-in local capture intervals, read with the v2 facts in one snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunObservationV3 {
    #[serde(flatten)]
    pub observation: RunObservationV2,
    pub timings: Vec<crate::AttemptTimingObservation>,
}

/// Maximum number of timeline entries in one projection; earlier ones are counted.
pub const TIMELINE_ENTRY_LIMIT: usize = 64;

/// Explicit version 4: the v3 facts plus a bounded, redacted chronological view
/// of the selected run, read in the same passive snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunObservationV4 {
    #[serde(flatten)]
    pub observation: RunObservationV3,
    pub timeline: RunTimeline,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineOrdering {
    JournalSequenceAscending,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelinePhase {
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
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunTimeline {
    pub ordering: TimelineOrdering,
    pub entry_limit: u32,
    /// Selected-run event references in this snapshot.
    pub run_events: u32,
    pub shown: u32,
    /// Earliest references left out of `entries`; the phase summary still counts them.
    pub omitted_earlier: u32,
    /// Journal events committed after this run's last event (other runs).
    pub later_global_events: u64,
    pub entries: Vec<TimelineEntry>,
    pub phases: Vec<TimelinePhaseSummary>,
    pub approval: TimelineApproval,
    pub cancellation: TimelineCancellation,
    pub cleanup: TimelineCleanup,
    pub verification: TimelineVerification,
    pub attempts: Vec<TimelineAttempt>,
    pub unknown: TimelineUnknowns,
}
/// `kind` comes from a closed allowlist; an unlisted kind is `unclassified`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineEntry {
    pub sequence: u64,
    pub event_sha256: String,
    pub state: RunState,
    pub phase: TimelinePhase,
    pub kind: &'static str,
    /// Position-derived: the latest attempt start at or before this event.
    pub attempt: Option<u32>,
    pub after_cancel_request: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelinePhaseSummary {
    pub phase: TimelinePhase,
    pub events: u32,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineApprovalOutcome {
    Granted,
    NotGranted,
    Pending,
    Unknown,
}
/// State transitions only; it does not authenticate the approval digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineApproval {
    pub outcome: TimelineApprovalOutcome,
    pub awaiting_sequence: Option<u64>,
    pub granted_sequence: Option<u64>,
}
/// Logical cancellation only; physical shutdown is reported under cleanup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineCancellation {
    pub requested_sequence: Option<u64>,
    pub before_start_sequence: Option<u64>,
    pub terminal_cancelled: bool,
    pub events_after_request: u32,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineCleanup {
    pub recorded: ProcessCleanupEvidence,
    pub confirmed_sequence: Option<u64>,
    pub unknown_sequence: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineVerificationState {
    NotReached,
    Required,
    InProgress,
    Verified,
    Failed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineVerification {
    pub state: TimelineVerificationState,
    pub record_sequence: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineAppliedSelection {
    pub model: ObservationUnknown,
    pub effort: ObservationUnknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineUsageStatus {
    Unknown,
    Partial,
    RecordedUnattested,
}
/// Absent values are UNKNOWN, never zero. Native usage is not attested.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineUsage {
    pub tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub status: TimelineUsageStatus,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineAttempt {
    pub ordinal: u32,
    pub requested: Option<AttemptObservationRequested>,
    pub applied: TimelineAppliedSelection,
    pub usage: TimelineUsage,
    pub recorded_cleanup: Option<ProcessCleanupEvidence>,
    /// `None` when no finished marker is bound to this attempt.
    pub result_after_cancel_request: Option<bool>,
    /// The cancel request in force at this attempt's finished marker, within its
    /// own approval cycle. `None` when there is none or no marker is bound.
    pub cancel_request_sequence: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineUnknowns {
    pub exact_revision_acceptance: ObservationUnknown,
    pub notification_delivery: ObservationUnknown,
    pub lost_notifications: ObservationUnknown,
    pub event_timestamps: ObservationUnknown,
    pub physical_cleanup: ObservationUnknown,
    pub objective: ObservationUnknown,
}
