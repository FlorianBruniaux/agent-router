//! Native Codex CLI preparation. Process supervision belongs to router-runtime.

use router_adapter_api::{
    AdapterError, HarnessAdapter, StreamDialect, ValidatedExecutionRequest, decode_transport,
    prepared_run, worker_degradations,
};
use router_types::{
    ExecutionSandbox, PreparedRun, RuntimeEvidence, ToolSelection, TransportOutcome,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct CodexAdapter;

impl HarnessAdapter for CodexAdapter {
    fn host(&self) -> &'static str {
        "codex"
    }

    fn prepare(&self, validated: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        let request = validated.request();
        if request.host.as_str() != self.host() {
            return Err(AdapterError::HostMismatch);
        }
        // The observed CLI has no per-run tool allowlist. Never broaden a subset.
        if !matches!(request.tools, ToolSelection::HostPolicy) {
            return Err(AdapterError::Unsupported("tool-selection"));
        }
        let degradations = worker_degradations(request, false)?;
        let effort =
            serde_json::to_string(&request.effort).map_err(|_| AdapterError::Invalid("effort"))?;
        let sandbox = match request.sandbox {
            ExecutionSandbox::ReadOnly => "read-only",
            ExecutionSandbox::WorkspaceWrite => "workspace-write",
        };
        let argv = vec![
            "exec".into(),
            "--model".into(),
            request.model.to_string(),
            "--config".into(),
            format!("model_reasoning_effort={effort}"),
            "--sandbox".into(),
            sandbox.into(),
            "--json".into(),
            "--ephemeral".into(),
            "--color".into(),
            "never".into(),
            "--cd".into(),
            request.workspace.clone(),
            "-".into(),
        ];
        prepared_run(validated, "codex", argv, degradations)
    }

    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        decode_transport(bytes, outcome, max_bytes, StreamDialect::Codex)
    }
}
