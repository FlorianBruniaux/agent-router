// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Narrow terminal SGR presentation; never edits sources or provider streams.
use super::{ContextViewError, ToolResult};
use router_adapter_api::{content_digest, is_digest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputViewInfo {
    pub method: String,
    pub raw_handle: String,
    pub raw_content_bytes: usize,
    pub shown_content_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct PresentedOutput {
    pub result: ToolResult,
    /// Complete ToolResult JSON bytes, excluding the MCP envelope.
    pub serialized_bytes: usize,
}

/// Only ESC [ digits/semicolons m is removed, including ESC [ m reset.
/// Any other ESC refuses the entire projection. All other bytes stay exact.
pub fn strip_terminal_sgr(text: &str) -> Result<String, ContextViewError> {
    if text.len() > 32768 {
        return Err(ContextViewError::SourceBound);
    }
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0x1b {
            index += 1;
            continue;
        }
        output.push_str(&text[start..index]);
        index += 1;
        if bytes.get(index) != Some(&b'[') {
            return Err(ContextViewError::TerminalEscape);
        }
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|b| b.is_ascii_digit() || *b == b';')
        {
            index += 1;
        }
        if bytes.get(index) != Some(&b'm') {
            return Err(ContextViewError::TerminalEscape);
        }
        index += 1;
        start = index;
    }
    output.push_str(&text[start..]);
    Ok(output)
}

/// Hashes exact arguments, not a shell string. This grants no program authority.
pub fn exec_presentation_key(
    argv: &[String],
    timeout_ms: Option<u64>,
) -> Result<String, ContextViewError> {
    if argv.is_empty()
        || argv.len() > 32
        || argv
            .iter()
            .any(|arg| arg.is_empty() || arg.len() > 4096 || arg.contains('\0'))
        || argv.iter().map(String::len).sum::<usize>() > 16384
        || timeout_ms.is_some_and(|n| !(1..=20000).contains(&n))
    {
        return Err(ContextViewError::ExecArguments);
    }
    let bytes = serde_json::to_vec(&("router:exec-presentation-key:v1", argv, timeout_ms))
        .map_err(|_| ContextViewError::ExecArguments)?;
    Ok(content_digest(&bytes))
}

pub(super) fn complete_raw(raw: &ToolResult) -> Result<usize, ContextViewError> {
    let bytes = raw
        .stdout
        .len()
        .checked_add(raw.stderr.len())
        .ok_or(ContextViewError::RecallRaw)?;
    if bytes > 8192
        || raw.termination != "exited"
        || raw.truncated
        || raw.window.is_some()
        || raw.view.is_some()
    {
        return Err(ContextViewError::RecallRaw);
    }
    Ok(bytes)
}

/// Compare full JSON, including view metadata and handle. Also compare the
/// escaped MCP text field, so transport escaping cannot reverse the gain.
pub fn present_terminal(
    raw: &ToolResult,
    info: OutputViewInfo,
) -> Result<PresentedOutput, ContextViewError> {
    let raw_bytes = complete_raw(raw)?;
    let stdout = strip_terminal_sgr(&raw.stdout)?;
    let stderr = strip_terminal_sgr(&raw.stderr)?;
    let shown = stdout.len() + stderr.len();
    if info.method != "terminal_sgr_v1"
        || !is_digest(&info.raw_handle)
        || info.raw_content_bytes != raw_bytes
        || info.shown_content_bytes != shown
        || shown == 0
        || shown >= raw_bytes
    {
        return Err(ContextViewError::PresentationRefused);
    }
    let mut candidate = raw.clone();
    candidate.stdout = stdout;
    candidate.stderr = stderr;
    candidate.view = Some(info);
    let original = serde_json::to_string(raw).map_err(|_| ContextViewError::PresentationRefused)?;
    let projected =
        serde_json::to_string(&candidate).map_err(|_| ContextViewError::PresentationRefused)?;
    let escaped = |text: &str| {
        serde_json::to_vec(text)
            .map(|v| v.len())
            .map_err(|_| ContextViewError::PresentationRefused)
    };
    if original.len() > super::mcp_stdio::MAX_RESULT_JSON_BYTES
        || projected.len() > super::mcp_stdio::MAX_RESULT_JSON_BYTES
        || projected.len() >= original.len()
        || escaped(&projected)? >= escaped(&original)?
    {
        return Err(ContextViewError::PresentationRefused);
    }
    Ok(PresentedOutput {
        result: candidate,
        serialized_bytes: projected.len(),
    })
}
