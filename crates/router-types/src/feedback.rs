//! Redacted local feedback. No prompt, output, path or free-text comment field.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    EffortLevel, HarnessId, ModelId, PhaseAbstention, RiskLevel, TaskKind, VerificationOutcome,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackPreferences {
    pub schema_version: u32,
    pub collection_enabled: bool,
    pub learning_enabled: bool,
    pub retention_seconds: u64,
}

impl Default for FeedbackPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            collection_enabled: false,
            learning_enabled: false,
            retention_seconds: 30 * 86400,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackConsent {
    pub preferences: FeedbackPreferences,
    pub revision: u64,
    pub updated_at_unix_seconds: u64,
}

/// Only immutable identities, never historical receipt bodies.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackBinding {
    pub task_hash: String,
    pub run_hash: String,
    pub receipt_hash: String,
    pub policy_hash: String,
    pub catalog_hash: String,
    pub snapshot_hash: String,
    pub producer_version: String,
    pub provenance_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackOrigin {
    Human,
    Automated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackFailure {
    Authentication,
    RateLimit,
    Timeout,
    Crash,
    Network,
    Policy,
    Validation,
    Objective,
    Budget,
    Cancelled,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceivedCost {
    Low,
    Acceptable,
    High,
    Unknown,
}

/// Discrete context and registered asset identities. No task text or embedding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackContext {
    pub task_kind: TaskKind,
    pub risk: RiskLevel,
    pub model: ModelId,
    pub effort: EffortLevel,
    pub harness: HarnessId,
    pub transport: String,
    pub agent: Option<String>,
    pub skills: BTreeSet<String>,
    pub plan: String,
    /// Actual selection probability in millionths; None means UNKNOWN.
    pub propensity_micros: Option<u32>,
    /// Route facts from the receipt. Absent in v1 events; None means UNKNOWN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<FeedbackRoute>,
}

/// Discrete facts about how the executed candidate was ranked. No text: the
/// only string is a registered candidate identifier, validated on record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRoute {
    pub candidate_id: String,
    /// One-based rank among the receipt's ranked candidates; None is UNKNOWN
    /// (the executed candidate is not in the receipt).
    pub rank: Option<u32>,
    /// Utility of this candidate minus the next ranked one, in micros. Signed:
    /// the receipt ranks the preferred model first, so it can be negative.
    /// None when no candidate follows or the candidate is not ranked.
    pub margin_micros: Option<i64>,
    /// Ranked candidates other than the executed one, as kept by the receipt.
    pub alternatives: u32,
    /// The baseline receipt's abstention, when the execution overrode it.
    pub abstention: Option<PhaseAbstention>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackMeasurements {
    pub cost_microusd: Option<u64>,
    pub latency_ms: Option<u64>,
    pub tokens: Option<u64>,
    pub attempts: u32,
    pub escalations: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanVerdict {
    pub success: VerificationOutcome,
    /// Quality on an explicit 0..=100 scale.
    pub quality: Option<u8>,
    pub perceived_cost: PerceivedCost,
    pub correction_required: Option<bool>,
    pub reviewer_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackInput {
    pub schema_version: u32,
    pub observed_at_unix_seconds: u64,
    pub binding: FeedbackBinding,
    pub context: FeedbackContext,
    pub origin: FeedbackOrigin,
    pub transport: VerificationOutcome,
    pub validation: VerificationOutcome,
    pub objective: VerificationOutcome,
    /// Required for a claimed validation or objective result, not authenticity proof.
    pub verification_evidence_sha256: Option<String>,
    pub human: Option<HumanVerdict>,
    pub failure: Option<FeedbackFailure>,
    pub measurements: FeedbackMeasurements,
    /// Explicitly redacted comment digest. The comment itself is never persisted.
    pub comment_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackEvent {
    pub sequence: u64,
    pub consent_revision: u64,
    pub recorded_at_unix_seconds: u64,
    pub input: FeedbackInput,
    pub event_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackExport {
    pub schema_version: u32,
    pub exported_at_unix_seconds: u64,
    pub consent: FeedbackConsent,
    pub events: Vec<FeedbackEvent>,
    pub content_hash: String,
}
