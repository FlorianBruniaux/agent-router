#![forbid(unsafe_code)]

mod execution;
pub use execution::*;
mod registry;
pub use registry::*;

use std::collections::{BTreeMap, BTreeSet};

use router_types::{Capability, CapabilitySupport, HostSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_AGE_SECONDS: u64 = 86_400;
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SNAPSHOT_MODELS: usize = 64;
pub const MAX_EVIDENCE_SOURCES: usize = 64;
pub const ALL_CAPABILITIES: [Capability; 5] = [
    Capability::Tools,
    Capability::Subagents,
    Capability::Vision,
    Capability::Network,
    Capability::WorkspaceWrite,
];

pub trait HostProbe {
    fn probe(&self, observed_at_unix_seconds: u64) -> Result<HostObservation, ProbeError>;
}

/// Evidence kinds describe provenance, not authentication. Callers must establish
/// the authority of an externally supplied document independently of its hash.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    CodexModelCache,
    ClaudeNativeStream,
    HostRuntime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub id: String,
    pub kind: EvidenceKind,
    pub observed_version: Option<String>,
    pub observed_at_unix_seconds: u64,
    pub content_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEvidence {
    pub support: CapabilitySupport,
    pub source_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostObservation {
    pub schema_version: u32,
    pub host: HostSnapshot,
    pub capabilities: BTreeMap<Capability, CapabilityEvidence>,
    pub evidence: Vec<EvidenceSource>,
    pub observed_at_unix_seconds: u64,
    pub snapshot_hash: String,
}

#[derive(Clone, Copy, Debug)]
pub struct SnapshotValidation {
    pub now_unix_seconds: u64,
    pub max_age_seconds: u64,
}

#[derive(Clone, Debug)]
pub struct ValidatedHostObservation {
    observation: HostObservation,
}

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot.too_large")]
    TooLarge,
    #[error("snapshot.json.invalid")]
    InvalidJson,
    #[error("snapshot.schema.unsupported")]
    UnsupportedSchema,
    #[error("snapshot.invalid: {0}")]
    Invalid(&'static str),
    #[error("snapshot.hash.invalid")]
    InvalidHash,
    #[error("snapshot.stale")]
    Stale,
    #[error("snapshot.source.stale")]
    StaleSource,
    #[error("snapshot.timestamp.future")]
    FutureTimestamp,
}

impl SnapshotError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "snapshot.too_large",
            Self::InvalidJson => "snapshot.json.invalid",
            Self::UnsupportedSchema => "snapshot.schema.unsupported",
            Self::Invalid(_) => "snapshot.invalid",
            Self::InvalidHash => "snapshot.hash.invalid",
            Self::Stale => "snapshot.stale",
            Self::StaleSource => "snapshot.source.stale",
            Self::FutureTimestamp => "snapshot.timestamp.future",
        }
    }
}

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("probe.unavailable")]
    Unavailable,
    #[error("probe.read")]
    Read,
    #[error("probe.cache.too_large")]
    CacheTooLarge,
    #[error("probe.cache.invalid: {0}")]
    InvalidCache(&'static str),
    #[error("probe.version.mismatch")]
    VersionMismatch,
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

impl ProbeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => "probe.unavailable",
            Self::Read => "probe.read",
            Self::CacheTooLarge => "probe.cache.too_large",
            Self::InvalidCache(_) => "probe.cache.invalid",
            Self::VersionMismatch => "probe.version.mismatch",
            Self::Snapshot(error) => error.code(),
        }
    }
}

impl HostObservation {
    /// Producer operation. Missing capability evidence is explicitly UNKNOWN.
    /// This computes integrity only; it does not attest to the producer's claims.
    pub fn seal(mut self) -> Result<Self, SnapshotError> {
        self.normalize();
        self.validate_structure()?;
        self.snapshot_hash = self.compute_hash()?;
        if serde_json::to_vec(&self)
            .map_err(|_| SnapshotError::InvalidJson)?
            .len()
            > MAX_SNAPSHOT_BYTES
        {
            return Err(SnapshotError::TooLarge);
        }
        Ok(self)
    }

    fn normalize(&mut self) {
        for capability in ALL_CAPABILITIES {
            self.capabilities
                .entry(capability)
                .or_insert_with(|| CapabilityEvidence {
                    support: CapabilitySupport::Unknown,
                    source_ids: BTreeSet::new(),
                });
        }
        self.evidence.sort_by(|left, right| left.id.cmp(&right.id));
    }

    fn compute_hash(&self) -> Result<String, SnapshotError> {
        #[derive(Serialize)]
        struct Payload<'a> {
            schema_version: u32,
            host: &'a HostSnapshot,
            capabilities: &'a BTreeMap<Capability, CapabilityEvidence>,
            evidence: &'a [EvidenceSource],
            observed_at_unix_seconds: u64,
        }
        let bytes = serde_json::to_vec(&Payload {
            schema_version: self.schema_version,
            host: &self.host,
            capabilities: &self.capabilities,
            evidence: &self.evidence,
            observed_at_unix_seconds: self.observed_at_unix_seconds,
        })
        .map_err(|_| SnapshotError::InvalidJson)?;
        let mut hasher = Sha256::new();
        hasher.update(b"agent-router:host-observation:v1\0");
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }

    fn validate_structure(&self) -> Result<(), SnapshotError> {
        if self.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::UnsupportedSchema);
        }
        if !valid_identifier(&self.host.id) || !valid_identifier(self.host.harness.as_str()) {
            return Err(SnapshotError::Invalid("host identity"));
        }
        if self.host.supported_models.len() > MAX_SNAPSHOT_MODELS
            || self.host.supported_efforts.len() > MAX_SNAPSHOT_MODELS
            || self.evidence.is_empty()
            || self.evidence.len() > MAX_EVIDENCE_SOURCES
        {
            return Err(SnapshotError::Invalid("collection bounds"));
        }
        for model in &self.host.supported_models {
            // A model may be observed while its supported efforts remain unknown.
            // Missing/empty efforts never authorize any effort at routing time.
            if !valid_identifier(model.as_str()) {
                return Err(SnapshotError::Invalid("model or effort"));
            }
        }
        if self
            .host
            .supported_efforts
            .keys()
            .any(|model| !self.host.supported_models.contains(model))
        {
            return Err(SnapshotError::Invalid("orphan model effort"));
        }
        let mut sources = BTreeMap::new();
        for source in &self.evidence {
            if !valid_identifier(&source.id)
                || !valid_hash(&source.content_sha256)
                || source.observed_version.as_ref().is_some_and(|value| {
                    value.is_empty()
                        || value.len() > 128
                        || !value.bytes().all(|byte| byte.is_ascii_graphic())
                })
                || sources.insert(&source.id, source).is_some()
            {
                return Err(SnapshotError::Invalid("evidence source"));
            }
            if source.observed_at_unix_seconds > self.observed_at_unix_seconds {
                return Err(SnapshotError::FutureTimestamp);
            }
        }
        for capability in ALL_CAPABILITIES {
            let evidence = self
                .capabilities
                .get(&capability)
                .ok_or(SnapshotError::Invalid("missing capability evidence"))?;
            if evidence.source_ids.len() > MAX_EVIDENCE_SOURCES {
                return Err(SnapshotError::Invalid("capability evidence bounds"));
            }
            let mut has_runtime_source = false;
            for id in &evidence.source_ids {
                let source = sources
                    .get(id)
                    .ok_or(SnapshotError::Invalid("unknown evidence source"))?;
                has_runtime_source |= source.kind == EvidenceKind::HostRuntime;
            }
            if evidence.support != CapabilitySupport::Unknown && !has_runtime_source {
                return Err(SnapshotError::Invalid(
                    "runtime support requires runtime evidence",
                ));
            }
            if self.host.capabilities.contains(&capability)
                != (evidence.support == CapabilitySupport::Supported)
            {
                return Err(SnapshotError::Invalid("contradictory capability evidence"));
            }
        }
        Ok(())
    }
}

impl ValidatedHostObservation {
    pub fn from_json(bytes: &[u8], validation: SnapshotValidation) -> Result<Self, SnapshotError> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(SnapshotError::TooLarge);
        }
        let value = strict_json(bytes)?;
        if let Some(host) = value.get("host").and_then(serde_json::Value::as_object) {
            let fields = [
                "id",
                "harness",
                "supported_models",
                "supported_efforts",
                "capabilities",
            ];
            if host.keys().any(|key| !fields.contains(&key.as_str())) {
                return Err(SnapshotError::InvalidJson);
            }
        }
        reject_duplicate_sets(&value)?;
        let observation = serde_json::from_value(value).map_err(|_| SnapshotError::InvalidJson)?;
        Self::from_observation(observation, validation)
    }

    pub fn from_observation(
        mut observation: HostObservation,
        validation: SnapshotValidation,
    ) -> Result<Self, SnapshotError> {
        observation.normalize();
        observation.validate_structure()?;
        if !valid_hash(&observation.snapshot_hash)
            || observation.compute_hash()? != observation.snapshot_hash
        {
            return Err(SnapshotError::InvalidHash);
        }
        if observation.observed_at_unix_seconds > validation.now_unix_seconds {
            return Err(SnapshotError::FutureTimestamp);
        }
        if validation.now_unix_seconds - observation.observed_at_unix_seconds
            > validation.max_age_seconds
        {
            return Err(SnapshotError::Stale);
        }
        for source in &observation.evidence {
            if validation.now_unix_seconds - source.observed_at_unix_seconds
                > validation.max_age_seconds
            {
                return Err(SnapshotError::StaleSource);
            }
        }
        if serde_json::to_vec(&observation)
            .map_err(|_| SnapshotError::InvalidJson)?
            .len()
            > MAX_SNAPSHOT_BYTES
        {
            return Err(SnapshotError::TooLarge);
        }
        Ok(Self { observation })
    }

    pub fn host(&self) -> &HostSnapshot {
        &self.observation.host
    }

    /// Complete replacement for request evidence, never merge caller assertions.
    pub fn capabilities(&self) -> BTreeMap<Capability, CapabilitySupport> {
        self.observation
            .capabilities
            .iter()
            .map(|(capability, evidence)| (*capability, evidence.support))
            .collect()
    }

    pub fn hash(&self) -> &str {
        &self.observation.snapshot_hash
    }

    pub fn observation(&self) -> &HostObservation {
        &self.observation
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reject_duplicate_sets(value: &serde_json::Value) -> Result<(), SnapshotError> {
    match value {
        serde_json::Value::Array(items) => {
            // Arrays of strings in this schema represent sets. Reject before
            // serde's BTreeSet conversion can silently discard duplicates.
            let mut seen = BTreeSet::new();
            for item in items {
                if let Some(text) = item.as_str() {
                    if !seen.insert(text) {
                        return Err(SnapshotError::Invalid("duplicate set entry"));
                    }
                }
                reject_duplicate_sets(item)?;
            }
        }
        serde_json::Value::Object(fields) => {
            for child in fields.values() {
                reject_duplicate_sets(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Strict JSON also rejects repeated object keys, including inside typed maps.
pub fn strict_json(bytes: &[u8]) -> Result<serde_json::Value, SnapshotError> {
    strict_json_limited(bytes, MAX_SNAPSHOT_BYTES)
}

/// Strict JSON with a caller-chosen byte bound. `strict_json` applies
/// `MAX_SNAPSHOT_BYTES`; callers whose documented bound is larger pass it here.
pub fn strict_json_limited(bytes: &[u8], limit: usize) -> Result<serde_json::Value, SnapshotError> {
    if bytes.len() > limit {
        return Err(SnapshotError::TooLarge);
    }
    use serde::de::{MapAccess, SeqAccess, Visitor};
    struct StrictValue(serde_json::Value);
    struct StrictVisitor;
    impl<'de> Visitor<'de> for StrictVisitor {
        type Value = StrictValue;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("JSON with unique object keys")
        }
        fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
            Ok(StrictValue(value.into()))
        }
        fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(StrictValue(value.into()))
        }
        fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
            Ok(StrictValue(value.into()))
        }
        fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
            serde_json::Number::from_f64(value)
                .map(|number| StrictValue(number.into()))
                .ok_or_else(|| E::custom("non-finite number"))
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(StrictValue(value.into()))
        }
        fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
            Ok(StrictValue(value.into()))
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(StrictValue(serde_json::Value::Null))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(StrictValue(value)) = seq.next_element()? {
                items.push(value);
            }
            Ok(StrictValue(items.into()))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut fields = serde_json::Map::new();
            while let Some((key, StrictValue(value))) = map.next_entry::<String, StrictValue>()? {
                if fields.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate object key"));
                }
            }
            Ok(StrictValue(fields.into()))
        }
    }
    impl<'de> Deserialize<'de> for StrictValue {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(StrictVisitor)
        }
    }
    serde_json::from_slice::<StrictValue>(bytes)
        .map(|value| value.0)
        .map_err(|_| SnapshotError::InvalidJson)
}

#[cfg(test)]
mod strict_json_limited_tests {
    use super::*;

    #[test]
    fn honors_the_caller_limit_in_both_directions() {
        let padded = format!("{{}}{}", " ".repeat(MAX_SNAPSHOT_BYTES));
        assert!(matches!(
            strict_json(padded.as_bytes()),
            Err(SnapshotError::TooLarge)
        ));
        assert!(strict_json_limited(padded.as_bytes(), MAX_SNAPSHOT_BYTES + 2).is_ok());
        assert!(matches!(
            strict_json_limited(b"{\"a\":1}", 6),
            Err(SnapshotError::TooLarge)
        ));
        assert!(strict_json_limited(b"{\"a\":1}", 7).is_ok());
    }

    #[test]
    fn still_rejects_duplicate_keys_under_a_larger_limit() {
        assert!(matches!(
            strict_json_limited(b"{\"a\":1,\"a\":2}", 1024),
            Err(SnapshotError::InvalidJson)
        ));
    }
}
