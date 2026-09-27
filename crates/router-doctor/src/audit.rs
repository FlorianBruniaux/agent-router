use crate::{
    DoctorError, MAX_ARTIFACTS, MAX_FILE_BYTES, MAX_TOTAL_BYTES, Result,
    diagnose::contains_secret_pattern,
};
use router_adapter_api::{content_digest, is_digest, strict_json};
use router_types::{AuditArtifactKind, AuditBundle, AuditMetadata, AuditRecord};

/// Raw content is transient input only. Debug never exposes it.
pub struct AuditInput<'a> {
    pub kind: AuditArtifactKind,
    pub bytes: &'a [u8],
}
impl std::fmt::Debug for AuditInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditInput")
            .field("kind", &self.kind)
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

pub fn build_audit_bundle(inputs: &[AuditInput<'_>]) -> Result<AuditBundle> {
    if inputs.len() > MAX_ARTIFACTS {
        return Err(DoctorError::Limit);
    }
    let mut total = 0usize;
    let mut records = Vec::new();
    let mut previous_hash = "0".repeat(64);
    for (index, input) in inputs.iter().enumerate() {
        total = total
            .checked_add(input.bytes.len())
            .ok_or(DoctorError::Limit)?;
        if input.bytes.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
            return Err(DoctorError::Limit);
        }
        let value = strict_json(input.bytes).map_err(|_| DoctorError::Schema)?;
        let entries = match &value {
            serde_json::Value::Object(fields) => fields.len(),
            serde_json::Value::Array(items) => items.len(),
            _ => return Err(DoctorError::Schema),
        };
        let metadata = AuditMetadata {
            schema_version: value
                .get("schema_version")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok()),
            top_level_entries: entries,
            contained_secret_pattern: contains_secret_pattern(input.bytes),
        };
        let mut record = AuditRecord {
            sequence: index as u64 + 1,
            kind: input.kind,
            content_sha256: content_digest(input.bytes),
            byte_count: input.bytes.len(),
            metadata,
            previous_hash,
            record_hash: String::new(),
        };
        record.record_hash = digest_record(&record)?;
        previous_hash = record.record_hash.clone();
        records.push(record);
    }
    Ok(AuditBundle {
        schema_version: 1,
        records,
        root_hash: previous_hash,
        raw_content_included: false,
        signature_verified: false,
    })
}

/// Check against an independently retained root to detect replacement/truncation.
/// Without that trust anchor this establishes internal integrity only, not
/// authenticity: a party can rewrite the chain and compute a different root.
pub fn verify_audit_bundle(bundle: &AuditBundle, expected_root: Option<&str>) -> Result<()> {
    if bundle.schema_version != 1
        || bundle.raw_content_included
        || bundle.signature_verified
        || bundle.records.len() > MAX_ARTIFACTS
        || !is_digest(&bundle.root_hash)
    {
        return Err(DoctorError::Integrity);
    }
    let mut previous = "0".repeat(64);
    let mut total = 0usize;
    for (index, record) in bundle.records.iter().enumerate() {
        total = total
            .checked_add(record.byte_count)
            .ok_or(DoctorError::Limit)?;
        if record.byte_count > MAX_FILE_BYTES
            || total > MAX_TOTAL_BYTES
            || record.metadata.top_level_entries > MAX_FILE_BYTES
        {
            return Err(DoctorError::Limit);
        }
        if record.sequence != index as u64 + 1
            || record.previous_hash != previous
            || !is_digest(&record.content_sha256)
            || digest_record(record)? != record.record_hash
        {
            return Err(DoctorError::Integrity);
        }
        previous = record.record_hash.clone();
    }
    if previous != bundle.root_hash || expected_root.is_some_and(|root| root != bundle.root_hash) {
        return Err(DoctorError::Integrity);
    }
    Ok(())
}

fn digest_record(record: &AuditRecord) -> Result<String> {
    let mut payload = b"agent-router:redacted-audit:v1\0".to_vec();
    payload.extend(
        serde_json::to_vec(&(
            record.sequence,
            record.kind,
            &record.content_sha256,
            record.byte_count,
            &record.metadata,
            &record.previous_hash,
        ))
        .map_err(|_| DoctorError::Schema)?,
    );
    Ok(content_digest(&payload))
}
