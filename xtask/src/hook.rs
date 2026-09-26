use crate::projection;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_INPUT: u64 = 65536;

// Hosts may add unrelated fields. They are never reflected, read as paths or
// used as authority. The launcher supplies the root independently of stdin.
#[derive(Deserialize)]
struct Input {
    hook_event_name: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    hook_specific_output: Context,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    hook_event_name: &'static str,
    additional_context: String,
}

fn context(host: &str, state: &str) -> Output {
    Output {
        hook_specific_output: Context {
            hook_event_name: "SessionStart",
            additional_context: format!(
                "agent-router-harness:{host}:{state}. Development harness only. Native host permissions remain authoritative."
            ),
        },
    }
}

pub fn run(root: &Path, host: &str, input: &[u8]) -> Option<Output> {
    if !matches!(host, "codex" | "claude") {
        return None;
    }
    if input.len() as u64 > MAX_INPUT {
        return Some(context(host, "UNKNOWN-input-limit"));
    }
    let Ok(event) = serde_json::from_slice::<Input>(input) else {
        return Some(context(host, "UNKNOWN-invalid-input"));
    };
    if event.hook_event_name != "SessionStart" {
        return None;
    }
    if !matches!(
        event.source.as_deref(),
        Some("startup" | "resume" | "clear" | "compact")
    ) {
        return None;
    }
    let status = if projection::check(root).is_ok() {
        "parity-ok"
    } else {
        "CHECK-REQUIRED; run cargo run -p xtask --locked -- check before editing; do not edit generated files"
    };
    Some(context(host, status))
}
