//! Native Claude CLI preparation, with no process or permission-granting code.

use router_adapter_api::{
    AdapterError, HarnessAdapter, StreamDialect, ValidatedExecutionRequest, decode_transport,
    prepared_run, worker_degradations,
};
use router_types::{
    EffortLevel, ExecutionSandbox, PreparedRun, RuntimeEvidence, ToolSelection, TransportOutcome,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ClaudeAdapter;

impl HarnessAdapter for ClaudeAdapter {
    fn host(&self) -> &'static str {
        "claude-code"
    }

    fn prepare(&self, validated: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        let request = validated.request();
        if request.host.as_str() != self.host() {
            return Err(AdapterError::HostMismatch);
        }
        let effort = match request.effort {
            EffortLevel::Low => "low",
            EffortLevel::Medium => "medium",
            EffortLevel::High => "high",
            EffortLevel::Xhigh => "xhigh",
            EffortLevel::Max => "max",
            _ => return Err(AdapterError::Unsupported("effort-native-flag")),
        };
        // A registry role is not proof that a same-named host agent is installed.
        let degradations = worker_degradations(request, false)?;
        let permission_mode = match request.sandbox {
            ExecutionSandbox::ReadOnly => "plan",
            ExecutionSandbox::WorkspaceWrite => "manual",
        };
        let mut argv = vec![
            "--print".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--input-format".into(),
            "text".into(),
            "--model".into(),
            request.model.to_string(),
            "--effort".into(),
            effort.into(),
            "--permission-mode".into(),
            permission_mode.into(),
            "--permission-prompts".into(),
            "none".into(),
            "--no-session-persistence".into(),
        ];
        match &request.tools {
            ToolSelection::HostPolicy => {}
            ToolSelection::Disabled => argv.extend(["--tools".into(), String::new()]),
            ToolSelection::AllowList(tools) => argv.extend(["--tools".into(), tools.join(",")]),
        }
        prepared_run(validated, "claude", argv, degradations)
    }

    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        decode_transport(bytes, outcome, max_bytes, StreamDialect::Claude)
    }
}
