//! Secret redaction by refusal. A response that carries a credential, in any
//! form listed in `docs/provider-transport.md`, is refused instead of scrubbed.
use super::TransportError;
use serde_json::Value;
use std::collections::BTreeMap;

pub const MIN_SECRET_BYTES: usize = 8;
pub const MAX_SECRET_BYTES: usize = 4096;
const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Detects secret values and their common encodings. Detection is a substring
/// match on byte patterns, so it is exact for the forms below and blind to
/// every other transformation.
pub struct Redactor {
    /// Byte patterns. Base64 patterns match exactly; all others ignore ASCII case.
    patterns: Vec<(Vec<u8>, bool)>,
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
        let mut patterns: Vec<(Vec<u8>, bool)> = plain
            .into_iter()
            .map(|p| (p.to_ascii_lowercase(), true))
            .chain(exact.into_iter().map(|p| (p, false)))
            .collect();
        patterns.sort();
        patterns.dedup();
        Ok(Self { patterns })
    }

    /// True when `bytes` contains the value in any covered form, also once ASCII
    /// whitespace is removed (base64 wrapped by MIME line breaks, a value split
    /// by a line break).
    pub fn leaks(&self, bytes: &[u8]) -> bool {
        if self.matches(bytes) {
            return true;
        }
        if bytes.iter().any(u8::is_ascii_whitespace) {
            let squeezed: Vec<u8> = bytes
                .iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect();
            return self.matches(&squeezed);
        }
        false
    }

    fn matches(&self, bytes: &[u8]) -> bool {
        self.patterns.iter().any(|(pattern, fold)| {
            !pattern.is_empty()
                && bytes.len() >= pattern.len()
                && bytes.windows(pattern.len()).any(|w| {
                    if *fold {
                        w.eq_ignore_ascii_case(pattern)
                    } else {
                        w == pattern.as_slice()
                    }
                })
        })
    }

    /// Scans a whole SSE body: raw bytes, every decoded JSON string and key of
    /// each `data:` payload, and the concatenation of the strings found at the
    /// same JSON path across events (a value split between two deltas).
    pub fn leaks_in_sse(&self, body: &[u8]) -> bool {
        if self.leaks(body) {
            return true;
        }
        let Ok(text) = std::str::from_utf8(body) else {
            return false;
        };
        let text = text.replace("\r\n", "\n");
        let mut joined: BTreeMap<String, String> = BTreeMap::new();
        for frame in text.split("\n\n") {
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
            if self.leaks(unbroken.as_bytes()) {
                return true;
            }
            let Ok(value) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if self.walk(&value, "", &mut joined) {
                return true;
            }
        }
        joined.values().any(|joined| self.leaks(joined.as_bytes()))
    }

    fn walk(&self, value: &Value, path: &str, joined: &mut BTreeMap<String, String>) -> bool {
        match value {
            Value::String(s) => {
                if self.leaks(s.as_bytes()) {
                    return true;
                }
                joined.entry(path.to_owned()).or_default().push_str(s);
                false
            }
            Value::Array(items) => {
                let path = format!("{path}[]");
                items.iter().any(|item| self.walk(item, &path, joined))
            }
            Value::Object(map) => map.iter().any(|(key, item)| {
                self.leaks(key.as_bytes()) || self.walk(item, &format!("{path}/{key}"), joined)
            }),
            _ => false,
        }
    }
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
