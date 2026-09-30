#![forbid(unsafe_code)]

mod learning;
mod policy;
mod sidecar_export;
mod store;
pub use sidecar_export::{SidecarExportGuard, SidecarExportProjection};

pub use learning::*;
pub use policy::*;
pub use store::FeedbackStore;

use router_types::{FeedbackInput, FeedbackOrigin, FeedbackPreferences, VerificationOutcome};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MAX_EVENTS: usize = 10_000;
pub const MAX_INPUT_BYTES: usize = 32 * 1024;
pub const MAX_RETENTION_SECONDS: u64 = 365 * 86400;
pub const MAX_VALUE: u64 = 1_000_000_000_000;

#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    #[error("feedback.invalid: {0}")]
    Invalid(&'static str),
    #[error("feedback.integrity: {0}")]
    Integrity(&'static str),
    #[error("feedback.consent_required")]
    Consent,
    #[error("feedback.learning_disabled")]
    LearningDisabled,
    #[error("feedback.limit")]
    Limit,
    #[error("feedback.storage_locked")]
    Locked,
    #[error("feedback.not_found")]
    NotFound,
    #[error("feedback.policy.gate: {0}")]
    Gate(&'static str),
    #[error("feedback.policy.signature")]
    Signature,
    #[error("feedback.storage_io")]
    Io(#[from] std::io::Error),
    #[error("feedback.storage_database")]
    Database(#[from] rusqlite::Error),
    #[error("feedback.serialization")]
    Json(#[from] serde_json::Error),
    #[error("feedback.evaluation: {0}")]
    Evaluation(#[from] router_eval::EvalError),
}
pub type Result<T> = std::result::Result<T, FeedbackError>;

/// Use for task/run IDs and explicitly submitted comments. A digest is an
/// integrity/correlation identifier, not anonymization or verified evidence.
pub fn redacted_digest(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > MAX_INPUT_BYTES {
        return Err(FeedbackError::Limit);
    }
    hash("feedback-redacted-v1", &value)
}
pub fn comment_digest(comment: &str) -> Result<String> {
    redacted_digest(comment)
}

pub fn parse_feedback(bytes: &[u8]) -> Result<FeedbackInput> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(FeedbackError::Limit);
    }
    let value =
        router_adapter_api::strict_json(bytes).map_err(|_| FeedbackError::Invalid("JSON"))?;
    if let Some(skills) = value
        .get("context")
        .and_then(|v| v.get("skills"))
        .and_then(|v| v.as_array())
    {
        let unique: std::collections::BTreeSet<_> = skills.iter().map(|s| s.as_str()).collect();
        if unique.len() != skills.len() {
            return Err(FeedbackError::Invalid("duplicate skill"));
        }
    }
    let input = serde_json::from_value(value)?;
    validate_input(&input)?;
    Ok(input)
}

pub(crate) fn hash<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(value)?);
    Ok(hex::encode(hasher.finalize()))
}
pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
/// Candidate ids follow the policy bound (128), wider than other identifiers.
fn candidate_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
pub(crate) fn validate_preferences(value: &FeedbackPreferences) -> Result<()> {
    if value.schema_version != 1
        || value.learning_enabled && !value.collection_enabled
        || value.retention_seconds == 0
        || value.retention_seconds > MAX_RETENTION_SECONDS
    {
        return Err(FeedbackError::Invalid("preferences"));
    }
    Ok(())
}
pub(crate) fn validate_input(input: &FeedbackInput) -> Result<()> {
    let b = &input.binding;
    if input.schema_version != 1
        || !identifier(&b.producer_version)
        || [
            &b.task_hash,
            &b.run_hash,
            &b.receipt_hash,
            &b.policy_hash,
            &b.catalog_hash,
            &b.snapshot_hash,
            &b.provenance_sha256,
        ]
        .iter()
        .any(|s| !valid_hash(s))
    {
        return Err(FeedbackError::Invalid("binding"));
    }
    let c = &input.context;
    if !identifier(c.model.as_str())
        || !identifier(c.harness.as_str())
        || !identifier(&c.transport)
        || !identifier(&c.plan)
        || c.agent.as_ref().is_some_and(|s| !identifier(s))
        || c.skills.len() > 64
        || c.skills.iter().any(|s| !identifier(s))
        || c.propensity_micros.is_some_and(|v| v == 0 || v > 1_000_000)
    {
        return Err(FeedbackError::Invalid("context"));
    }
    if c.route.as_ref().is_some_and(|r| {
        !candidate_identifier(&r.candidate_id)
            || r.alternatives > 64
            || r.rank.is_some_and(|v| v == 0 || v > r.alternatives + 1)
    }) {
        return Err(FeedbackError::Invalid("route"));
    }
    if input.origin == FeedbackOrigin::Human && input.human.is_none()
        || input.origin == FeedbackOrigin::Automated && input.human.is_some()
        || input
            .human
            .as_ref()
            .is_some_and(|h| h.quality.is_some_and(|v| v > 100) || !valid_hash(&h.reviewer_sha256))
    {
        return Err(FeedbackError::Invalid("human verdict"));
    }
    if input
        .verification_evidence_sha256
        .as_ref()
        .is_some_and(|s| !valid_hash(s))
        || input
            .comment_sha256
            .as_ref()
            .is_some_and(|s| !valid_hash(s))
        || (input.validation != VerificationOutcome::Unknown
            || input.objective != VerificationOutcome::Unknown)
            && input.verification_evidence_sha256.is_none()
        || input.objective == VerificationOutcome::Passed
            && input.validation != VerificationOutcome::Passed
    {
        return Err(FeedbackError::Invalid("verification evidence"));
    }
    let m = &input.measurements;
    if [m.cost_microusd, m.latency_ms, m.tokens]
        .into_iter()
        .flatten()
        .any(|v| v > MAX_VALUE)
        || m.attempts == 0
        || m.attempts > 100
        || m.escalations > 99
        // An escalated run is itself the escalation while its own attempt
        // counter starts at one.
        || m.escalations > m.attempts
    {
        return Err(FeedbackError::Invalid("measurements"));
    }
    if serde_json::to_vec(input)?.len() > MAX_INPUT_BYTES {
        return Err(FeedbackError::Limit);
    }
    Ok(())
}
