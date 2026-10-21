//! Explicit context preparation. No environment, telemetry or worker data opts in.
use super::*;
use crate::finite_tools::{FiniteContextPolicy, mcp_catalog_digest};
pub(super) fn tool_names(context: Option<&FiniteContextPolicy>) -> Vec<&'static str> {
    let mut names = vec!["read_file", "write_file", "exec"];
    if context.is_some_and(|p| !p.terminal_argument_sha256.is_empty()) {
        names.push("read_output");
    }
    names
}
pub(super) fn request_matches(names: &[String], context: Option<&FiniteContextPolicy>) -> bool {
    let expected: BTreeSet<String> = tool_names(context)
        .into_iter()
        .map(|name| format!("mcp__finite__{name}"))
        .collect();
    names.len() == expected.len() && names.iter().cloned().collect::<BTreeSet<_>>() == expected
}
pub(super) fn configuration(
    run: &PreparedRun,
    broker: &OwnedBroker,
    tools: bool,
    subscription: Option<&SubscriptionPreparation>,
    context: Option<&FiniteContextPolicy>,
) -> Result<Vec<u8>> {
    let bytes = codex_configuration_for(run, broker, tools, subscription)?;
    let Some(policy) = context else {
        return Ok(bytes);
    };
    policy
        .validate()
        .map_err(|_| RuntimeError::Invalid("finite context policy"))?;
    if !tools || subscription.is_some() {
        return Err(RuntimeError::Sandbox);
    }
    let mut text = String::from_utf8(bytes).map_err(|_| RuntimeError::Sandbox)?;
    if !policy.terminal_argument_sha256.is_empty() {
        text = text.replacen(
            "enabled_tools = [\"read_file\", \"write_file\", \"exec\"]",
            "enabled_tools = [\"read_file\", \"write_file\", \"exec\", \"read_output\"]",
            1,
        );
        text.push_str("[mcp_servers.finite.tools.read_output]\napproval_mode = \"approve\"\n");
    }
    Ok(text.into_bytes())
}
pub(super) fn catalog(
    run: &PreparedRun,
    tools: bool,
    subscription: Option<&SubscriptionPreparation>,
    context: Option<&FiniteContextPolicy>,
) -> Result<Vec<u8>> {
    let bytes = codex_catalog_for(run, tools, subscription)?;
    let Some(policy) = context else {
        return Ok(bytes);
    };
    policy
        .validate()
        .map_err(|_| RuntimeError::Invalid("finite context policy"))?;
    if !tools || subscription.is_some() {
        return Err(RuntimeError::Sandbox);
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    value["models"][0]["base_instructions"]="Local authored MCP context qualification. Use only the advertised finite tools. No model ran.".into();
    Ok(serde_json::to_vec(&value)?)
}
pub(super) fn binding(base: &str, context: Option<&FiniteContextPolicy>) -> Result<String> {
    match context {
        Some(policy) => crate::digest(
            "finite-codex-context-tools-startup-v1",
            &(
                base,
                policy.digest().map_err(|_| RuntimeError::Sandbox)?,
                mcp_catalog_digest(Some(policy)).map_err(|_| RuntimeError::Sandbox)?,
                tool_names(Some(policy)),
            ),
        ),
        None => Ok(base.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_runtime_allowlist_is_explicit_and_profile_scoped() {
        let legacy = vec![
            "mcp__finite__read_file".into(),
            "mcp__finite__write_file".into(),
            "mcp__finite__exec".into(),
        ];
        let mut context = legacy.clone();
        context.push("mcp__finite__read_output".into());
        let policy = FiniteContextPolicy {
            schema_version: 1,
            read_windows: true,
            terminal_argument_sha256: std::collections::BTreeSet::from(["a".repeat(64)]),
        };
        assert!(request_matches(&legacy, None));
        assert!(!request_matches(&context, None));
        assert!(request_matches(&context, Some(&policy)));
        assert!(!request_matches(&legacy, Some(&policy)));
        context[0] = context[1].clone();
        assert!(!request_matches(&context, Some(&policy)));
    }
}
