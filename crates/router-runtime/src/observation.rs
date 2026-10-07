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
