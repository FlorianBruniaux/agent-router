//! Secret redaction by refusal. A response that carries a credential, in any
//! form listed in `docs/provider-transport.md`, is refused instead of scrubbed.
use super::TransportError;
use aho_corasick::{AhoCorasick, AhoCorasickKind};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MIN_SECRET_BYTES: usize = 8;
pub const MAX_SECRET_BYTES: usize = 4096;
const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Bound of one SSE frame scanned for JSON, the same as the stream decoder's.
const MAX_FRAME_BYTES: usize = 256 * 1024;
/// Bound of one JSON path built while walking a payload.
const MAX_PATH_BYTES: usize = 1024;
/// Bound of the bytes held by every distinct JSON path of one body.
const MAX_PATH_TOTAL_BYTES: usize = 1024 * 1024;

/// Called between scan passes so a deadline or a cancellation ends the scan.
pub type ScanCheck<'a> = &'a dyn Fn() -> Result<(), TransportError>;

/// Detects secret values and their common encodings. Detection is a substring
/// match on byte patterns, so it is exact for the forms below and blind to
/// every other transformation. Each pass is one linear multi-pattern search.
pub struct Redactor {
    /// Patterns that ignore ASCII case (stored lower-cased).
    folded: Option<AhoCorasick>,
    /// Base64 patterns, which match exactly.
    exact: Option<AhoCorasick>,
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Redactor([REDACTED])")
    }
}

impl Redactor {
    /// Values shorter than `MIN_SECRET_BYTES` are refused: a short pattern
    /// would match ordinary protocol text and refuse every response.
    pub fn new(values: &[&str]) -> Result<Self, TransportError> {
        let mut plain = Vec::new();
        let mut exact = Vec::new();
        for value in values {
            if value.len() < MIN_SECRET_BYTES || value.len() > MAX_SECRET_BYTES {
                return Err(TransportError::Credential);
            }
            forms(value, &mut plain, &mut exact);
        }
        let mut folded: Vec<Vec<u8>> = plain.into_iter().map(|p| p.to_ascii_lowercase()).collect();
        folded.sort();
        folded.dedup();
        exact.sort();
        exact.dedup();
        Ok(Self {
            folded: automaton(&folded, true)?,
            exact: automaton(&exact, false)?,
        })
    }

    /// True when `bytes` contains the value in any covered form, also once ASCII
    /// whitespace is removed (base64 wrapped by MIME line breaks, a value split
    /// by a line break).
    pub fn leaks(&self, bytes: &[u8]) -> bool {
        self.leaks_checked(bytes, &|| Ok(())).unwrap_or(true)
    }

    fn leaks_checked(&self, bytes: &[u8], check: ScanCheck<'_>) -> Result<bool, TransportError> {
        check()?;
        if self.matches(bytes) {
            return Ok(true);
        }
        if bytes.iter().any(u8::is_ascii_whitespace) {
            check()?;
            let squeezed: Vec<u8> = bytes
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect();
            return Ok(self.matches(&squeezed));
        }
        Ok(false)
    }

    fn matches(&self, bytes: &[u8]) -> bool {
        self.folded.as_ref().is_some_and(|a| a.is_match(bytes))
            || self.exact.as_ref().is_some_and(|a| a.is_match(bytes))
    }

    /// `scan_sse` without a deadline. A body the scan refuses to walk (`Limit`)
    /// counts as a leak: the answer is fail closed.
    pub fn leaks_in_sse(&self, body: &[u8]) -> bool {
        self.scan_sse(body, &|| Ok(())).unwrap_or(true)
    }

    /// Scans a whole SSE body: raw bytes, every decoded JSON string and key of
    /// each `data:` payload, and the concatenation of the strings found at the
    /// same JSON path across events (a value split between two deltas).
    ///
    /// A frame over 256 KiB or a JSON path over 1 KiB, or 1 MiB of distinct
    /// paths, returns `Limit` before any per-path allocation grows with them.
    /// `check` runs between passes.
    pub fn scan_sse(&self, body: &[u8], check: ScanCheck<'_>) -> Result<bool, TransportError> {
        if self.leaks_checked(body, check)? {
            return Ok(true);
        }
        let Ok(text) = std::str::from_utf8(body) else {
            return Ok(false);
        };
        let text = text.replace("\r\n", "\n");
        let mut joined = Joined::default();
        for frame in text.split("\n\n") {
            check()?;
            if frame.len() > MAX_FRAME_BYTES {
                return Err(TransportError::Limit);
            }
            let mut data = String::new();
            for line in frame.lines() {
                if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.strip_prefix(' ').unwrap_or(value));
                }
            }
            // Lines of one event joined with no separator: a value cut by a
            // line break between two `data:` lines.
            let unbroken: String = frame
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|v| v.strip_prefix(' ').unwrap_or(v))
                .collect();
            if self.leaks_checked(unbroken.as_bytes(), check)? {
                return Ok(true);
            }
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if self.walk(&value, "", &mut joined, check)? {
                return Ok(true);
            }
        }
        for joined in joined.strings.values() {
            if self.leaks_checked(joined.as_bytes(), check)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn walk(
        &self,
        value: &Value,
        path: &str,
        joined: &mut Joined,
        check: ScanCheck<'_>,
    ) -> Result<bool, TransportError> {
        match value {
            Value::String(s) => {
                if self.leaks_checked(s.as_bytes(), check)? {
                    return Ok(true);
                }
                joined.push(path, s)?;
                Ok(false)
            }
            Value::Array(items) => {
                if path.len() + 2 > MAX_PATH_BYTES {
                    return Err(TransportError::Limit);
                }
                let path = format!("{path}[]");
                for item in items {
                    if self.walk(item, &path, joined, check)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Value::Object(map) => {
                for (key, item) in map {
                    if self.leaks_checked(key.as_bytes(), check)? {
                        return Ok(true);
                    }
                    if path.len() + 1 + key.len() > MAX_PATH_BYTES {
                        return Err(TransportError::Limit);
                    }
                    if self.walk(item, &format!("{path}/{key}"), joined, check)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
}

/// Strings found at one JSON path, across events, with a bound on the memory
/// the paths themselves take.
#[derive(Default)]
struct Joined {
    strings: BTreeMap<String, String>,
    path_bytes: usize,
}

impl Joined {
    fn push(&mut self, path: &str, s: &str) -> Result<(), TransportError> {
        if let Some(existing) = self.strings.get_mut(path) {
            existing.push_str(s);
            return Ok(());
        }
        self.path_bytes += path.len();
        if self.path_bytes > MAX_PATH_TOTAL_BYTES {
            return Err(TransportError::Limit);
        }
        self.strings.insert(path.to_owned(), s.to_owned());
        Ok(())
    }
}

fn automaton(patterns: &[Vec<u8>], fold: bool) -> Result<Option<AhoCorasick>, TransportError> {
    let patterns: Vec<&Vec<u8>> = patterns.iter().filter(|p| !p.is_empty()).collect();
    if patterns.is_empty() {
        return Ok(None);
    }
    // The default picks a full DFA, whose build time and size grow with the
    // patterns (about 250 KB for a 4096 byte credential). A contiguous NFA
    // builds in linear time and still searches in one linear pass.
    AhoCorasick::builder()
        .kind(Some(AhoCorasickKind::ContiguousNFA))
        .ascii_case_insensitive(fold)
        .build(patterns)
        .map(Some)
        .map_err(|_| TransportError::Limit)
}

fn forms(value: &str, out: &mut Vec<Vec<u8>>, exact: &mut Vec<Vec<u8>>) {
    let bytes = value.as_bytes();
    out.push(bytes.to_vec());
    out.push(json_escape(value, false, false).into_bytes());
    out.push(json_escape(value, true, false).into_bytes());
    out.push(json_escape(value, true, true).into_bytes());
    for upper in [false, true] {
        let mut full = String::new();
        for unit in value.encode_utf16() {
            full.push_str(&if upper {
                format!("\\u{unit:04X}")
            } else {
                format!("\\u{unit:04x}")
            });
        }
        out.push(full.into_bytes());
        out.push(percent(bytes, false, upper).into_bytes());
        out.push(percent(bytes, true, upper).into_bytes());
        out.push(
            bytes
                .iter()
                .map(|b| {
                    if upper {
                        format!("{b:02X}")
                    } else {
                        format!("{b:02x}")
                    }
                })
                .collect::<String>()
                .into_bytes(),
        );
    }
    for alphabet in [B64_STD, B64_URL] {
        base64_forms(bytes, alphabet, exact);
    }
}

fn json_escape(value: &str, ascii_only: bool, upper: bool) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (ascii_only && !c.is_ascii()) => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&if upper {
                        format!("\\u{unit:04X}")
                    } else {
                        format!("\\u{unit:04x}")
                    });
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn percent(bytes: &[u8], everything: bool, upper: bool) -> String {
    let mut out = String::new();
    for &b in bytes {
        if !everything && (b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')) {
            out.push(b as char);
        } else if upper {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push_str(&format!("%{b:02x}"));
        }
    }
    out
}

fn base64(bytes: &[u8], alphabet: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| (n << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(alphabet[((n >> (18 - 6 * i)) & 63) as usize]);
        }
    }
    out
}

/// A base64 substring depends on the offset of the value in the encoded text.
/// Encode with 0, 1 and 2 leading bytes and drop the characters that mix in a
/// neighbour byte. Padding is never part of a pattern.
fn base64_forms(value: &[u8], alphabet: &[u8; 64], out: &mut Vec<Vec<u8>>) {
    for (shift, skip) in [(0usize, 0usize), (1, 2), (2, 3)] {
        let mut input = vec![0u8; shift];
        input.extend_from_slice(value);
        let encoded = base64(&input, alphabet);
        let end = encoded.len() - usize::from((shift + value.len()) % 3 != 0);
        if end > skip {
            out.push(encoded[skip..end].to_vec());
        }
    }
}
