//! Human projections of the exact structured result. Rendering never changes
//! authority, dispatch, exit status or the independently usable JSON contract.
use serde_json::Value;
use std::fmt::Write;

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_NODES: usize = 200_000;

pub(crate) fn render(title: &str, document: &str) -> String {
    if document.len() > MAX_BYTES {
        return format!(
            "{title}\nHuman view omitted: result exceeds the display limit. Use --json for the complete structured result.\n"
        );
    }
    let Ok(value) = serde_json::from_str::<Value>(document) else {
        return format!("{title}\nHuman view unavailable. Use --json for the structured result.\n");
    };
    let mut output = format!("{title}\n");
    let mut remaining = MAX_NODES;
    if write_value(&mut output, &value, 0, &mut remaining).is_err() {
        output.push_str("\nOutput shortened at the display limit. Use --json for the complete structured result.\n");
    }
    output
}

/// Escapes control characters and Unicode format characters (bidi controls,
/// zero-width and invisible marks) so untrusted text cannot rewrite the
/// terminal or visually reorder a line. Shared by every human projection.
pub(crate) fn escaped(text: &str) -> String {
    let mut output = String::new();
    for character in text.chars() {
        if character.is_control() || is_format(character) {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
}

/// Unicode general category Cf (Unicode 16) plus the Zl/Zp line and paragraph
/// separators, which also rewrite a terminal line.
fn is_format(character: char) -> bool {
    matches!(character as u32,
        0xad | 0x600..=0x605 | 0x61c | 0x6dd | 0x70f | 0x890..=0x891 | 0x8e2 | 0x180e
        | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x2064 | 0x2066..=0x206f
        | 0xfeff | 0xfff9..=0xfffb | 0x110bd | 0x110cd | 0x13430..=0x1343f
        | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a | 0xe0001 | 0xe0020..=0xe007f)
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("not set (null)".into()),
        Value::Bool(value) => Some(if *value { "yes" } else { "no" }.into()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) if value == "unknown" => Some("UNKNOWN".into()),
        Value::String(value) => Some(escaped(value)),
        Value::Array(values) if values.is_empty() => Some("none".into()),
        Value::Object(values) if values.is_empty() => Some("none".into()),
        _ => None,
    }
}

fn label(key: &str) -> String {
    let text = escaped(key).replace('_', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn write_value(
    output: &mut String,
    value: &Value,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), ()> {
    if depth > 64 || *remaining == 0 || output.len() > MAX_BYTES {
        return Err(());
    }
    *remaining -= 1;
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if let Some(text) = scalar(value) {
                    writeln!(output, "{indent}{}: {text}", label(key)).map_err(|_| ())?;
                    *remaining = remaining.checked_sub(1).ok_or(())?;
                } else {
                    writeln!(output, "{indent}{}:", label(key)).map_err(|_| ())?;
                    write_value(output, value, depth + 1, remaining)?;
                }
                if output.len() > MAX_BYTES {
                    return Err(());
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if let Some(text) = scalar(value) {
                    writeln!(output, "{indent}- {text}").map_err(|_| ())?;
                    *remaining = remaining.checked_sub(1).ok_or(())?;
                } else {
                    writeln!(output, "{indent}- [{}]", index + 1).map_err(|_| ())?;
                    write_value(output, value, depth + 1, remaining)?;
                }
                if output.len() > MAX_BYTES {
                    return Err(());
                }
            }
        }
        _ => writeln!(output, "{indent}{}", scalar(value).ok_or(())?).map_err(|_| ())?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unknown_and_exact_approval_while_neutralizing_terminal_controls() {
        let digest = "a".repeat(64);
        let value = serde_json::json!({"approval_digest":digest,"objective":"unknown",
            "observed_model":null,"warning":"untrusted\u{1b}[2J\n\u{202e}text", "ready":false});
        let output = render("Run review", &value.to_string());
        assert!(output.contains(&format!("Approval digest: {digest}")));
        assert!(output.contains("Objective: UNKNOWN"));
        assert!(output.contains("Observed model: not set (null)"));
        assert!(output.contains("Ready: no"));
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains('\u{202e}'));
        assert!(output.contains("\\n"));
    }

    #[test]
    fn escapes_a_sample_of_every_format_block_and_line_separators() {
        for code in [
            0xad_u32, 0x600, 0x605, 0x6dd, 0x70f, 0x890, 0x891, 0x8e2, 0x180e, 0x200b, 0x2028,
            0x2029, 0x2060, 0x2064, 0x206f, 0xfeff, 0xfff9, 0xfffb, 0x110bd, 0x110cd, 0x13430,
            0x1343f, 0x1bca0, 0x1bca3, 0x1d173, 0x1d17a, 0xe0001, 0xe0020, 0xe007f,
        ] {
            let character = char::from_u32(code).unwrap();
            let text = escaped(&format!("a{character}b"));
            assert!(!text.contains(character), "U+{code:04X} leaked");
            assert!(
                text.contains(&format!("\\u{{{code:x}}}")),
                "U+{code:04X}: {text}"
            );
        }
    }

    #[test]
    fn escapes_unicode_format_characters() {
        let text = escaped("a\u{200e}b\u{200f}c\u{61c}d\u{202e}e\u{2066}f\u{feff}g");
        assert_eq!(
            text,
            "a\\u{200e}b\\u{200f}c\\u{61c}d\\u{202e}e\\u{2066}f\\u{feff}g"
        );
        assert_eq!(escaped("plain \u{e9}\u{4e2d}"), "plain \u{e9}\u{4e2d}");
    }
}
