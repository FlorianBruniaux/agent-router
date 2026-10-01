//! Authenticated capability evidence. Hashes alone establish no authority.
use std::collections::BTreeSet;

use hmac::{Hmac, Mac};
use router_types::{Capability, HostSnapshot};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

use crate::{
    DEFAULT_MAX_AGE_SECONDS, EvidenceSource, content_digest, valid_hash, valid_identifier,
};

pub const CAPABILITY_PROOF_SCHEMA_VERSION: u32 = 1;
pub const MAX_CAPABILITY_PROOF_BYTES: usize = 32 * 1024;
pub const MAX_CAPABILITY_PROOFS: usize = 4096;
const PROOF_KEY_INFO: &[u8] = b"agent-router/capability-proof/v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProofTier {
    Proven,
    Attested,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAttemptOutcome {
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityNetworkScope {
    LoopbackFetch,
}

/// Execution identity, derived by the consumer from its actual configuration.
/// A digest declared by a snapshot is never a substitute for this comparison.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProofBinding {
    pub harness_id: String,
    pub host_snapshot_id: String,
    pub binary_sha256: String,
    pub observed_version: String,
    pub adapter_id: String,
    pub argument_template_sha256: String,
    pub sandbox_helper_sha256: String,
    pub sandbox_wrapper_sha256: String,
    pub sandbox_profile_sha256: String,
    pub write_roots: BTreeSet<String>,
    pub network: bool,
    pub os: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProofRecord {
    pub schema_version: u32,
    pub proof_id: String,
    pub sequence: u64,
    pub store_id: String,
    pub capability: Capability,
    pub tier: CapabilityProofTier,
    pub network_scope: Option<CapabilityNetworkScope>,
    pub binding: CapabilityProofBinding,
    pub model_id: String,
    pub nonce_sha256: String,
    pub expected_effect_sha256: String,
    pub started_at_unix_seconds: u64,
    pub ended_at_unix_seconds: u64,
    pub max_age_seconds: u64,
    pub outcome: CapabilityAttemptOutcome,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedCapabilityProof {
    pub record: CapabilityProofRecord,
    pub mac_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedCapabilityProof {
    pub sequence: u64,
    pub proof_id: String,
    pub proof_sha256: String,
    pub capability: Capability,
    pub tier: CapabilityProofTier,
    pub observed_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub binding: CapabilityProofBinding,
}

#[derive(Debug, Error)]
pub enum CapabilityProofError {
    #[error("capability.evidence.unauthenticated")]
    Unauthenticated,
    #[error("capability.evidence.invalid")]
    Invalid,
    #[error("capability.evidence.binding_mismatch")]
    BindingMismatch,
    #[error("capability.evidence.timestamp_future")]
    Future,
    #[error("capability.evidence.expired")]
    Expired,
    #[error("capability.evidence.revoked")]
    Revoked,
    #[error("capability.evidence.bounds")]
    Bounds,
}

impl CapabilityProofError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "capability.evidence.unauthenticated",
            Self::Invalid => "capability.evidence.invalid",
            Self::BindingMismatch => "capability.evidence.binding_mismatch",
            Self::Future => "capability.evidence.timestamp_future",
            Self::Expired => "capability.evidence.expired",
            Self::Revoked => "capability.evidence.revoked",
            Self::Bounds => "capability.evidence.bounds",
        }
    }
}

/// Trusted verifier supplied by the runtime store, or an explicit test fixture.
pub trait CapabilityProofVerifier: Send + Sync {
    fn verify(
        &self,
        source: &EvidenceSource,
        capability: Capability,
        host: &HostSnapshot,
        now_unix_seconds: u64,
    ) -> Result<VerifiedCapabilityProof, CapabilityProofError>;
}

/// Domain-separated key. It is deliberately neither Debug nor Serialize.
pub struct CapabilityProofKey([u8; 32]);

impl CapabilityProofKey {
    pub fn derive(store_secret: &[u8; 32], store_id: &str) -> Result<Self, CapabilityProofError> {
        if !valid_hash(store_id) {
            return Err(CapabilityProofError::Invalid);
        }
        let prk = hkdf_extract(store_id.as_bytes(), store_secret)?;
        let bytes = hkdf_expand(&prk, PROOF_KEY_INFO, 32)?;
        Ok(Self(
            bytes
                .try_into()
                .map_err(|_| CapabilityProofError::Invalid)?,
        ))
    }

    /// Low-level authentication only. Issuers must separately establish the
    /// supervised effect; no production canary issuer is provided here.
    pub fn authenticate(
        &self,
        record: CapabilityProofRecord,
    ) -> Result<AuthenticatedCapabilityProof, CapabilityProofError> {
        record.validate()?;
        let bytes = record.bytes()?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).map_err(|_| CapabilityProofError::Invalid)?;
        mac.update(&bytes);
        Ok(AuthenticatedCapabilityProof {
            record,
            mac_sha256: hex::encode(mac.finalize().into_bytes()),
        })
    }

    pub fn verify(&self, proof: &AuthenticatedCapabilityProof) -> Result<(), CapabilityProofError> {
        proof.record.validate()?;
        let supplied =
            hex::decode(&proof.mac_sha256).map_err(|_| CapabilityProofError::Unauthenticated)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).map_err(|_| CapabilityProofError::Invalid)?;
        mac.update(&proof.record.bytes()?);
        mac.verify_slice(&supplied)
            .map_err(|_| CapabilityProofError::Unauthenticated)
    }
}

fn hkdf_extract(salt: &[u8], input: &[u8]) -> Result<[u8; 32], CapabilityProofError> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(salt).map_err(|_| CapabilityProofError::Invalid)?;
    mac.update(input);
    Ok(mac.finalize().into_bytes().into())
}

// RFC 5869 section 2.3. Only 32 output bytes are used by the production key.
fn hkdf_expand(prk: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>, CapabilityProofError> {
    if len > 255 * 32 {
        return Err(CapabilityProofError::Bounds);
    }
    let mut output = Vec::with_capacity(len);
    let mut previous = Vec::new();
    for counter in 1..=len.div_ceil(32) {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(prk).map_err(|_| CapabilityProofError::Invalid)?;
        mac.update(&previous);
        mac.update(info);
        mac.update(&[counter as u8]);
        previous = mac.finalize().into_bytes().to_vec();
        output.extend_from_slice(&previous);
    }
    output.truncate(len);
    Ok(output)
}

impl CapabilityProofBinding {
    pub fn digest(&self) -> Result<String, CapabilityProofError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map(|bytes| content_digest(&bytes))
            .map_err(|_| CapabilityProofError::Invalid)
    }

    fn validate(&self) -> Result<(), CapabilityProofError> {
        if ![
            &self.harness_id,
            &self.host_snapshot_id,
            &self.adapter_id,
            &self.os,
        ]
        .iter()
        .all(|s| valid_identifier(s))
            || self.observed_version.is_empty()
            || self.observed_version.len() > 128
            || !self.observed_version.bytes().all(|b| b.is_ascii_graphic())
            || ![
                &self.binary_sha256,
                &self.argument_template_sha256,
                &self.sandbox_helper_sha256,
                &self.sandbox_wrapper_sha256,
                &self.sandbox_profile_sha256,
            ]
            .iter()
            .all(|s| valid_hash(s))
            || self.write_roots.len() > 64
            || self.write_roots.iter().any(|root| {
                root.is_empty()
                    || root.len() > 4096
                    || root.starts_with('/')
                    || root.split('/').any(|p| p == "..")
                    || root.bytes().any(|b| b < 32)
            })
        {
            return Err(CapabilityProofError::Invalid);
        }
        if serde_json::to_vec(self)
            .map_err(|_| CapabilityProofError::Invalid)?
            .len()
            > MAX_CAPABILITY_PROOF_BYTES
        {
            return Err(CapabilityProofError::Bounds);
        }
        Ok(())
    }
}

impl CapabilityProofRecord {
    fn bytes(&self) -> Result<Vec<u8>, CapabilityProofError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CapabilityProofError::Invalid)?;
        if bytes.len() > MAX_CAPABILITY_PROOF_BYTES {
            return Err(CapabilityProofError::Bounds);
        }
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), CapabilityProofError> {
        self.binding.validate()?;
        if self.schema_version != CAPABILITY_PROOF_SCHEMA_VERSION
            || self.sequence == 0
            || !valid_identifier(&self.proof_id)
            || !valid_hash(&self.store_id)
            || !valid_identifier(&self.model_id)
            || !valid_hash(&self.nonce_sha256)
            || !valid_hash(&self.expected_effect_sha256)
            || self.started_at_unix_seconds > self.ended_at_unix_seconds
            || self.max_age_seconds == 0
            || self.max_age_seconds > DEFAULT_MAX_AGE_SECONDS
            || self
                .ended_at_unix_seconds
                .checked_add(self.max_age_seconds)
                .is_none()
            || (self.capability == Capability::Network) != self.network_scope.is_some()
            || (self.capability == Capability::Network && !self.binding.network)
        {
            return Err(CapabilityProofError::Invalid);
        }
        self.bytes()?;
        Ok(())
    }
}

impl AuthenticatedCapabilityProof {
    pub fn from_json(bytes: &[u8]) -> Result<Self, CapabilityProofError> {
        if bytes.len() > MAX_CAPABILITY_PROOF_BYTES + 256 {
            return Err(CapabilityProofError::Bounds);
        }
        let value = crate::strict_json(bytes).map_err(|_| CapabilityProofError::Invalid)?;
        let proof: Self =
            serde_json::from_value(value).map_err(|_| CapabilityProofError::Invalid)?;
        proof.record.validate()?;
        if !valid_hash(&proof.mac_sha256) {
            return Err(CapabilityProofError::Unauthenticated);
        }
        Ok(proof)
    }

    pub fn digest(&self) -> Result<String, CapabilityProofError> {
        serde_json::to_vec(self)
            .map(|bytes| content_digest(&bytes))
            .map_err(|_| CapabilityProofError::Invalid)
    }
}

/// An authenticated bounded store view and consumer-derived expected bindings.
/// Historical callers supply the run clock; newer rows are not past authority.
pub struct CapabilityProofSet {
    records: Vec<AuthenticatedCapabilityProof>,
    expected: Vec<CapabilityProofBinding>,
}

impl CapabilityProofSet {
    pub fn authenticate(
        store_id: &str,
        key: &CapabilityProofKey,
        records: Vec<AuthenticatedCapabilityProof>,
        expected: Vec<CapabilityProofBinding>,
    ) -> Result<Self, CapabilityProofError> {
        if records.len() > MAX_CAPABILITY_PROOFS || expected.len() > MAX_CAPABILITY_PROOFS {
            return Err(CapabilityProofError::Bounds);
        }
        let mut sequence = 0;
        let mut ids = BTreeSet::new();
        for proof in &records {
            key.verify(proof)?;
            if proof.record.store_id != store_id
                || proof.record.sequence <= sequence
                || !ids.insert(&proof.record.proof_id)
            {
                return Err(CapabilityProofError::Unauthenticated);
            }
            sequence = proof.record.sequence;
        }
        for binding in &expected {
            binding.validate()?;
        }
        Ok(Self { records, expected })
    }

    /// Historical reconstruction only: retain exactly the previously observed
    /// sequence horizon, including attempts sharing the run's second.
    pub fn through_sequence(mut self, sequence: u64) -> Self {
        self.records
            .retain(|proof| proof.record.sequence <= sequence);
        self
    }

    /// Sequence horizon of exact references in an already authenticated log.
    /// For historical proposal reconstruction only, after the proposal's source
    /// digest is bound by its caller. This does not validate freshness, deciding
    /// outcome or execution authority; the historical verifier must do so next.
    pub fn referenced_sequence(
        &self,
        host: &HostSnapshot,
        sources: &[EvidenceSource],
    ) -> Result<u64, CapabilityProofError> {
        if sources.len() > crate::MAX_EVIDENCE_SOURCES {
            return Err(CapabilityProofError::Bounds);
        }
        let mut seen = BTreeSet::new();
        let mut sequence = 0;
        for source in sources {
            if source.kind != crate::EvidenceKind::HostRuntime || !seen.insert(&source.id) {
                return Err(CapabilityProofError::Unauthenticated);
            }
            let proof = self
                .records
                .iter()
                .find(|proof| proof.record.proof_id == source.id)
                .ok_or(CapabilityProofError::Unauthenticated)?;
            if proof.record.tier != CapabilityProofTier::Proven
                || proof.record.binding.host_snapshot_id != host.id
                || proof.record.binding.harness_id != host.harness.as_str()
                || proof.digest()? != source.content_sha256
                || Some(proof.record.binding.observed_version.as_str())
                    != source.observed_version.as_deref()
                || proof.record.ended_at_unix_seconds != source.observed_at_unix_seconds
            {
                return Err(CapabilityProofError::BindingMismatch);
            }
            sequence = sequence.max(proof.record.sequence);
        }
        Ok(sequence)
    }

    pub fn authenticated_bindings(&self) -> Vec<CapabilityProofBinding> {
        let mut bindings = Vec::new();
        for proof in &self.records {
            if !bindings.contains(&proof.record.binding) {
                bindings.push(proof.record.binding.clone());
            }
        }
        bindings
    }

    pub fn deciding_proofs(
        &self,
        host: &HostSnapshot,
        now: u64,
    ) -> Vec<(&AuthenticatedCapabilityProof, VerifiedCapabilityProof)> {
        self.records
            .iter()
            .filter_map(|proof| {
                let source = EvidenceSource {
                    id: proof.record.proof_id.clone(),
                    kind: crate::EvidenceKind::HostRuntime,
                    observed_version: Some(proof.record.binding.observed_version.clone()),
                    observed_at_unix_seconds: proof.record.ended_at_unix_seconds,
                    content_sha256: proof.digest().ok()?,
                };
                self.verify(&source, proof.record.capability, host, now)
                    .ok()
                    .map(|verified| (proof, verified))
            })
            .collect()
    }
}

impl CapabilityProofVerifier for CapabilityProofSet {
    fn verify(
        &self,
        source: &EvidenceSource,
        capability: Capability,
        host: &HostSnapshot,
        now: u64,
    ) -> Result<VerifiedCapabilityProof, CapabilityProofError> {
        let proof = self
            .records
            .iter()
            .find(|p| p.record.proof_id == source.id)
            .ok_or(CapabilityProofError::Unauthenticated)?;
        let record = &proof.record;
        let digest = proof.digest()?;
        if source.kind != crate::EvidenceKind::HostRuntime
            || record.capability != capability
            || record.tier != CapabilityProofTier::Proven
            || record.binding.harness_id != host.harness.as_str()
            || record.binding.host_snapshot_id != host.id
            || !self.expected.contains(&record.binding)
            || source.content_sha256 != digest
            || source.observed_version.as_deref() != Some(&record.binding.observed_version)
            || source.observed_at_unix_seconds != record.ended_at_unix_seconds
        {
            return Err(CapabilityProofError::BindingMismatch);
        }
        if record.started_at_unix_seconds > now || record.ended_at_unix_seconds > now {
            return Err(CapabilityProofError::Future);
        }
        let expiry = record.ended_at_unix_seconds + record.max_age_seconds;
        if now > expiry {
            return Err(CapabilityProofError::Expired);
        }
        let deciding = self.records.iter().rev().find(|p| {
            p.record.capability == capability
                && p.record.tier == record.tier
                && p.record.binding == record.binding
        });
        // Fresh verification sees the latest authenticated attempt even if the
        // wall clock moved backwards. Historical readers must bound sequence.
        if deciding.is_some_and(|proof| proof.record.ended_at_unix_seconds > now) {
            return Err(CapabilityProofError::Future);
        }
        if record.outcome != CapabilityAttemptOutcome::Succeeded
            || deciding.is_none_or(|p| p.record.sequence != record.sequence)
        {
            return Err(CapabilityProofError::Revoked);
        }
        Ok(VerifiedCapabilityProof {
            sequence: record.sequence,
            proof_id: record.proof_id.clone(),
            proof_sha256: digest,
            capability,
            tier: record.tier,
            observed_at_unix_seconds: record.ended_at_unix_seconds,
            expires_at_unix_seconds: expiry,
            binding: record.binding.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5869_sha256_case_one_extract_and_expand() {
        let salt = hex::decode("000102030405060708090a0b0c").unwrap();
        let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap();
        let prk = hkdf_extract(&salt, &[0x0b; 22]).unwrap();
        assert_eq!(
            hex::encode(prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );
        assert_eq!(
            hex::encode(hkdf_expand(&prk, &info, 42).unwrap()),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
        assert!(hkdf_expand(&prk, &info, 8161).is_err());
    }
    fn record() -> CapabilityProofRecord {
        CapabilityProofRecord {
            schema_version: 1,
            proof_id: "fixture-proof-1".into(),
            sequence: 1,
            store_id: "1".repeat(64),
            capability: Capability::WorkspaceWrite,
            tier: CapabilityProofTier::Proven,
            network_scope: None,
            binding: CapabilityProofBinding {
                harness_id: "fixture-host".into(),
                host_snapshot_id: "fixture-snapshot".into(),
                binary_sha256: "2".repeat(64),
                observed_version: "fixture-1.0".into(),
                adapter_id: "fixture-adapter".into(),
                argument_template_sha256: "3".repeat(64),
                sandbox_helper_sha256: "4".repeat(64),
                sandbox_wrapper_sha256: "5".repeat(64),
                sandbox_profile_sha256: "6".repeat(64),
                write_roots: BTreeSet::from(["src".into()]),
                network: false,
                os: "fixture-os".into(),
            },
            model_id: "fixture-model".into(),
            nonce_sha256: "7".repeat(64),
            expected_effect_sha256: "8".repeat(64),
            started_at_unix_seconds: 100,
            ended_at_unix_seconds: 110,
            max_age_seconds: 1000,
            outcome: CapabilityAttemptOutcome::Succeeded,
        }
    }
    fn host() -> HostSnapshot {
        HostSnapshot {
            id: "fixture-snapshot".into(),
            harness: router_types::HarnessId::parse("fixture-host").unwrap(),
            supported_models: BTreeSet::new(),
            supported_efforts: Default::default(),
            capabilities: BTreeSet::new(),
        }
    }
    fn key() -> CapabilityProofKey {
        CapabilityProofKey::derive(&[42; 32], &"1".repeat(64)).unwrap()
    }
    fn source(proof: &AuthenticatedCapabilityProof) -> EvidenceSource {
        EvidenceSource {
            id: proof.record.proof_id.clone(),
            kind: crate::EvidenceKind::HostRuntime,
            observed_version: Some(proof.record.binding.observed_version.clone()),
            observed_at_unix_seconds: proof.record.ended_at_unix_seconds,
            content_sha256: proof.digest().unwrap(),
        }
    }
    fn set(
        records: Vec<AuthenticatedCapabilityProof>,
        expected: CapabilityProofBinding,
    ) -> CapabilityProofSet {
        CapabilityProofSet::authenticate(&"1".repeat(64), &key(), records, vec![expected]).unwrap()
    }

    #[test]
    fn authenticated_proof_checks_every_source_and_consumer_binding() {
        let record = record();
        let proof = key().authenticate(record.clone()).unwrap();
        let proofs = set(vec![proof.clone()], record.binding.clone());
        let source = source(&proof);
        let verify = |s: &EvidenceSource, cap, h: &HostSnapshot, now| proofs.verify(s, cap, h, now);
        assert!(verify(&source, record.capability, &host(), 120).is_ok());
        assert!(verify(&source, Capability::Network, &host(), 120).is_err());
        let mut other_host = host();
        other_host.id = "other".into();
        assert!(verify(&source, record.capability, &other_host, 120).is_err());
        other_host = host();
        other_host.harness = router_types::HarnessId::parse("other").unwrap();
        assert!(verify(&source, record.capability, &other_host, 120).is_err());
        for mutate in [
            (|s: &mut EvidenceSource| s.id = "other".into()) as fn(&mut EvidenceSource),
            |s| s.content_sha256 = "9".repeat(64),
            |s| s.observed_at_unix_seconds += 1,
            |s| s.observed_version = Some("fixture-2.0".into()),
        ] {
            let mut changed = source.clone();
            mutate(&mut changed);
            assert!(verify(&changed, record.capability, &host(), 120).is_err());
        }
        for mutate in [
            (|b: &mut CapabilityProofBinding| b.binary_sha256 = "9".repeat(64))
                as fn(&mut CapabilityProofBinding),
            |b| b.observed_version = "fixture-2.0".into(),
            |b| b.argument_template_sha256 = "9".repeat(64),
            |b| b.adapter_id = "other".into(),
            |b| b.sandbox_helper_sha256 = "9".repeat(64),
            |b| b.sandbox_wrapper_sha256 = "9".repeat(64),
            |b| b.sandbox_profile_sha256 = "9".repeat(64),
            |b| b.network = true,
            |b| {
                b.write_roots.insert("other".into());
            },
            |b| b.os = "other".into(),
        ] {
            let mut expected = record.binding.clone();
            mutate(&mut expected);
            assert!(
                set(vec![proof.clone()], expected)
                    .verify(&source, record.capability, &host(), 120)
                    .is_err()
            );
        }
        assert!(matches!(
            verify(&source, record.capability, &host(), 109),
            Err(CapabilityProofError::Future)
        ));
        assert!(verify(&source, record.capability, &host(), 1110).is_ok());
        assert!(matches!(
            verify(&source, record.capability, &host(), 1111),
            Err(CapabilityProofError::Expired)
        ));
        // A historical clock remains valid after expiry on a current clock.
        assert!(verify(&source, record.capability, &host(), 120).is_ok());
    }

    #[test]
    fn mac_store_tier_and_approval_domain_cannot_be_substituted() {
        let record = record();
        let proof = key().authenticate(record.clone()).unwrap();
        let other_key = CapabilityProofKey::derive(&[43; 32], &record.store_id).unwrap();
        assert!(other_key.verify(&proof).is_err());
        assert!(
            CapabilityProofKey::derive(&[42; 32], &"2".repeat(64))
                .unwrap()
                .verify(&proof)
                .is_err()
        );
        let mut changed = proof.clone();
        changed.record.tier = CapabilityProofTier::Attested;
        assert!(key().verify(&changed).is_err());
        let mut changed = proof.clone();
        changed.record.sequence += 1;
        assert!(key().verify(&changed).is_err());
        let mut changed = proof.clone();
        changed.record.capability = Capability::Tools;
        assert!(key().verify(&changed).is_err());
        let mut raw = Hmac::<Sha256>::new_from_slice(&[42; 32]).unwrap();
        raw.update(b"agent-router/approval/v1");
        raw.update(&record.bytes().unwrap());
        let mut changed = proof.clone();
        changed.mac_sha256 = hex::encode(raw.finalize().into_bytes());
        assert!(key().verify(&changed).is_err());
        assert!(
            CapabilityProofSet::authenticate(
                &"2".repeat(64),
                &key(),
                vec![proof],
                vec![record.binding.clone()]
            )
            .is_err()
        );
        let mut attested = record.clone();
        attested.tier = CapabilityProofTier::Attested;
        let proof = key().authenticate(attested).unwrap();
        assert!(
            set(vec![proof.clone()], record.binding)
                .verify(&source(&proof), record.capability, &host(), 120)
                .is_err()
        );
    }

    #[test]
    fn newer_failure_revokes_until_newer_success_and_history_uses_run_clock() {
        let first = record();
        let mut failure = first.clone();
        failure.proof_id = "fixture-proof-2".into();
        failure.sequence = 2;
        failure.started_at_unix_seconds = 150;
        failure.ended_at_unix_seconds = 160;
        failure.outcome = CapabilityAttemptOutcome::Failed;
        let proof = key().authenticate(first.clone()).unwrap();
        let failed = key().authenticate(failure.clone()).unwrap();
        let proofs = set(vec![proof.clone(), failed.clone()], first.binding.clone());
        assert!(
            set(vec![proof.clone(), failed.clone()], first.binding.clone())
                .through_sequence(1)
                .verify(&source(&proof), first.capability, &host(), 120)
                .is_ok()
        );
        assert!(matches!(
            proofs.verify(&source(&proof), first.capability, &host(), 170),
            Err(CapabilityProofError::Revoked)
        ));
        assert!(
            proofs
                .verify(&source(&failed), first.capability, &host(), 170)
                .is_err()
        );
        assert!(proofs.deciding_proofs(&host(), 170).is_empty());
        let mut recovery = failure;
        recovery.proof_id = "fixture-proof-3".into();
        recovery.sequence = 3;
        recovery.started_at_unix_seconds = 200;
        recovery.ended_at_unix_seconds = 210;
        recovery.outcome = CapabilityAttemptOutcome::Succeeded;
        let recovered = key().authenticate(recovery).unwrap();
        let proofs = set(vec![proof, failed, recovered.clone()], first.binding);
        assert!(
            proofs
                .verify(&source(&recovered), first.capability, &host(), 220)
                .is_ok()
        );
        assert_eq!(proofs.deciding_proofs(&host(), 220).len(), 1);
    }

    #[test]
    fn structural_bounds_and_authentication_reject_malformed_records() {
        let record = record();
        for mutate in [
            (|r: &mut CapabilityProofRecord| r.max_age_seconds = DEFAULT_MAX_AGE_SECONDS + 1)
                as fn(&mut CapabilityProofRecord),
            |r| r.max_age_seconds = 0,
            |r| r.ended_at_unix_seconds = u64::MAX,
            |r| r.started_at_unix_seconds = r.ended_at_unix_seconds + 1,
            |r| r.capability = Capability::Network,
            |r| r.sequence = 0,
            |r| {
                r.binding.write_roots.insert("../store".into());
            },
        ] {
            let mut changed = record.clone();
            mutate(&mut changed);
            assert!(key().authenticate(changed).is_err());
        }
        let proof = key().authenticate(record.clone()).unwrap();
        assert!(
            CapabilityProofSet::authenticate(
                &record.store_id,
                &key(),
                vec![proof.clone(), proof.clone()],
                vec![record.binding.clone()]
            )
            .is_err()
        );
        let bytes = serde_json::to_vec(&proof).unwrap();
        assert_eq!(
            AuthenticatedCapabilityProof::from_json(&bytes).unwrap(),
            proof
        );
        assert!(
            AuthenticatedCapabilityProof::from_json(&vec![b' '; MAX_CAPABILITY_PROOF_BYTES + 257])
                .is_err()
        );
    }
    #[test]
    fn referenced_horizon_preserves_same_second_history_without_live_authority() {
        let first = key().authenticate(record()).unwrap();
        let source = source(&first);
        let mut failed = first.record.clone();
        failed.sequence += 1;
        failed.proof_id = "later-same-second-failure".into();
        failed.outcome = CapabilityAttemptOutcome::Failed;
        let records = vec![first.clone(), key().authenticate(failed).unwrap()];
        let proofs = set(records.clone(), first.record.binding.clone());
        assert_eq!(
            proofs
                .referenced_sequence(&host(), std::slice::from_ref(&source))
                .unwrap(),
            1
        );
        assert!(matches!(
            proofs.verify(&source, first.record.capability, &host(), 110),
            Err(CapabilityProofError::Revoked)
        ));
        assert!(
            set(records, first.record.binding.clone())
                .through_sequence(1)
                .verify(&source, first.record.capability, &host(), 110)
                .is_ok()
        );
        assert!(
            proofs
                .referenced_sequence(&host(), &[source.clone(), source.clone()])
                .is_err()
        );
        let mut changed = source.clone();
        changed.content_sha256 = "f".repeat(64);
        assert!(proofs.referenced_sequence(&host(), &[changed]).is_err());
        let mut changed = source;
        changed.id = "missing-reference".into();
        assert!(proofs.referenced_sequence(&host(), &[changed]).is_err());
        let mut wrong_host = host();
        wrong_host.id = "another-host".into();
        assert!(
            proofs
                .referenced_sequence(&wrong_host, &[super::tests::source(&first)])
                .is_err()
        );
    }
    #[test]
    fn proven_clock_rollback_cannot_hide_a_newer_authenticated_attempt() {
        let first = key().authenticate(record()).unwrap();
        let reference = source(&first);
        for outcome in [
            CapabilityAttemptOutcome::Failed,
            CapabilityAttemptOutcome::Succeeded,
        ] {
            let mut latest = record();
            latest.proof_id = "later-clock-attempt".into();
            latest.sequence = 2;
            latest.started_at_unix_seconds = 200;
            latest.ended_at_unix_seconds = 200;
            latest.outcome = outcome;
            let latest = key().authenticate(latest).unwrap();
            let current = set(
                vec![first.clone(), latest.clone()],
                first.record.binding.clone(),
            );
            assert!(
                current
                    .verify(&reference, record().capability, &host(), 150)
                    .is_err(),
                "rolling the clock back must not restore a superseded proof"
            );
            assert!(
                set(vec![first.clone(), latest], first.record.binding.clone())
                    .through_sequence(1)
                    .verify(&reference, record().capability, &host(), 150)
                    .is_ok(),
                "only an explicitly bounded historical horizon retains the old authority"
            );
        }
    }
}
