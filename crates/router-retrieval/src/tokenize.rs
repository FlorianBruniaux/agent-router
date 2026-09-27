use std::collections::BTreeMap;

/// Keep full technical tokens (e.g. C++, src/lib.rs, foo::bar) alongside words.
/// Delimiters are deterministic and language-independent. No stemming or stop list.
pub(crate) fn terms(text: &str) -> BTreeMap<String, u32> {
    let lower = text.to_lowercase();
    let mut result = BTreeMap::new();
    for word in lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        *result.entry(word.to_string()).or_default() += 1;
    }
    for token in lower.split_whitespace() {
        let token = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | '!' | '?'
            )
        });
        if !token.is_empty()
            && token
                .chars()
                .any(|c| matches!(c, '_' | '-' | '.' | '/' | ':' | '+' | '#'))
        {
            *result.entry(token.to_string()).or_default() += 1;
        }
    }
    result
}

pub(crate) fn quoted_phrases(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(phrase)
        .filter(|p| !p.is_empty())
        .collect()
}

pub(crate) fn phrase(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| {
            !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '+' | '#'))
        })
        .map(|token| token.trim_end_matches(['.', ':']))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
