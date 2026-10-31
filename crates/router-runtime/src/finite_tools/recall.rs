// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Complete, bounded raw results scoped to one caller-owned session binding.
use super::{ContextViewError, ToolResult, presentation::complete_raw};
use router_adapter_api::{content_digest, is_digest};
use std::collections::BTreeMap;

pub struct RecallStore {
    session_binding: String,
    entries: BTreeMap<String, (u32, ToolResult)>,
    bytes: usize,
}
impl RecallStore {
    /// The owner must supply a fresh, private binding for each session.
    pub fn new(session_binding: String) -> Result<Self, ContextViewError> {
        if !is_digest(&session_binding) {
            return Err(ContextViewError::RecallBinding);
        }
        Ok(Self {
            session_binding,
            entries: BTreeMap::new(),
            bytes: 0,
        })
    }
    pub(super) fn preview(
        &self,
        sequence: u32,
        policy_sha256: &str,
        raw: &ToolResult,
    ) -> Result<String, ContextViewError> {
        if sequence == 0 || !is_digest(policy_sha256) {
            return Err(ContextViewError::RecallBinding);
        }
        let bytes = complete_raw(raw)?;
        if self.entries.len() >= 8 || self.bytes.checked_add(bytes).is_none_or(|n| n > 32768) {
            return Err(ContextViewError::RecallBound);
        }
        if self.entries.values().any(|(seen, _)| *seen == sequence) {
            return Err(ContextViewError::RecallSequence);
        }
        let bytes = serde_json::to_vec(&(
            "router:raw-output:v1",
            &self.session_binding,
            sequence,
            policy_sha256,
            raw,
        ))
        .map_err(|_| ContextViewError::RecallRaw)?;
        Ok(content_digest(&bytes))
    }
    /// Atomic admission: failures retain every existing entry and no fragment.
    pub fn insert(
        &mut self,
        sequence: u32,
        policy_sha256: &str,
        raw: &ToolResult,
    ) -> Result<String, ContextViewError> {
        let handle = self.preview(sequence, policy_sha256, raw)?;
        self.bytes += raw.stdout.len() + raw.stderr.len();
        self.entries.insert(handle.clone(), (sequence, raw.clone()));
        Ok(handle)
    }
    pub fn get(&self, handle: &str) -> Result<ToolResult, ContextViewError> {
        if !is_digest(handle) {
            return Err(ContextViewError::RecallMissing);
        }
        self.entries
            .get(handle)
            .map(|(_, raw)| raw.clone())
            .ok_or(ContextViewError::RecallMissing)
    }
}
