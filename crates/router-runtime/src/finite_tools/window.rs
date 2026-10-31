// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Exact, bounded UTF-8 line selection. LF is the only separator; bytes stay intact.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineWindow {
    pub start_line: u32,
    pub max_lines: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedWindow<'a> {
    pub text: &'a str,
    pub first_line: u32,
    pub returned_lines: u32,
    pub total_lines: u32,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContextViewError {
    #[error("context.terminal_escape")]
    TerminalEscape,
    #[error("context.presentation_refused")]
    PresentationRefused,
    #[error("context.exec_arguments")]
    ExecArguments,
    #[error("context.recall_binding")]
    RecallBinding,
    #[error("context.recall_bound")]
    RecallBound,
    #[error("context.recall_sequence")]
    RecallSequence,
    #[error("context.recall_missing")]
    RecallMissing,
    #[error("context.recall_raw")]
    RecallRaw,
    #[error("context.window_arguments")]
    WindowArguments,
    #[error("context.source_bound")]
    SourceBound,
    #[error("context.window_range")]
    WindowRange,
}

impl LineWindow {
    fn validate(self) -> Result<Self, ContextViewError> {
        if !(1..=32768).contains(&self.start_line) || !(1..=32768).contains(&self.max_lines) {
            return Err(ContextViewError::WindowArguments);
        }
        Ok(self)
    }

    pub(super) fn from_arguments(
        args: &serde_json::Value,
        enabled: bool,
    ) -> Result<Option<Self>, ContextViewError> {
        match (args.get("start_line"), args.get("max_lines")) {
            (None, None) => Ok(None),
            (Some(start), Some(count)) if enabled => {
                let number = |value: &serde_json::Value| {
                    value
                        .as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or(ContextViewError::WindowArguments)
                };
                Self {
                    start_line: number(start)?,
                    max_lines: number(count)?,
                }
                .validate()
                .map(Some)
            }
            _ => Err(ContextViewError::WindowArguments),
        }
    }
}

/// Select complete lines, preserving CRLF, indentation and an unterminated tail.
/// Source size is bounded like finite files; EOF+1 is an empty valid window.
pub fn select_line_window(
    text: &str,
    window: LineWindow,
) -> Result<SelectedWindow<'_>, ContextViewError> {
    let window = window.validate()?;
    if text.len() > 32768 {
        return Err(ContextViewError::SourceBound);
    }
    let total_lines = text.bytes().filter(|b| *b == b'\n').count() as u32
        + u32::from(!text.is_empty() && !text.ends_with('\n'));
    if window.start_line > total_lines + 1 {
        return Err(ContextViewError::WindowRange);
    }
    let mut start = 0;
    let mut end = text.len();
    let mut line = 1;
    let stop_line = window.start_line + window.max_lines;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            line += 1;
            if line == window.start_line {
                start = index + 1;
            }
            if line == stop_line {
                end = index + 1;
                break;
            }
        }
    }
    if window.start_line == total_lines + 1 {
        start = text.len();
    }
    let returned_lines = window.max_lines.min(total_lines + 1 - window.start_line);
    Ok(SelectedWindow {
        text: &text[start..end],
        first_line: window.start_line,
        returned_lines,
        total_lines,
        has_more: window.start_line - 1 + returned_lines < total_lines,
    })
}
