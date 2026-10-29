// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use crate::stream::{Dialect, decode};
use router_adapter_api::{
    AdapterError, HarnessAdapter, ValidatedExecutionRequest, prepared_run, worker_degradations,
};
use router_types::{
    EffortLevel, ExecutionSandbox, PreparedRun, RuntimeEvidence, ToolSelection, TransportOutcome,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct CopilotAdapter;

impl HarnessAdapter for CopilotAdapter {
    fn host(&self) -> &'static str {
        "copilot"
    }
    fn prepare(&self, validated: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        let request = validated.request();
        if request.host.as_str() != self.host() {
            return Err(AdapterError::HostMismatch);
        }
        if request.model.as_str() == "auto" {
            return Err(AdapterError::Unsupported("automatic-model-alias"));
        }
        let effort = match request.effort {
            EffortLevel::Low => "low",
            EffortLevel::Medium => "medium",
            EffortLevel::High => "high",
            EffortLevel::Xhigh => "xhigh",
            EffortLevel::Max => "max",
            _ => return Err(AdapterError::Unsupported("effort-native-flag")),
        };
        let mut argv = vec![
            "--model".into(),
            request.model.to_string(),
            "--effort".into(),
            effort.into(),
            "--output-format".into(),
            "json".into(),
            "--no-color".into(),
            "--no-auto-update".into(),
            "--no-remote-export".into(),
            "--log-level".into(),
            "none".into(),
            "-C".into(),
            request.workspace.clone(),
        ];
        if request.sandbox == ExecutionSandbox::ReadOnly {
            argv.extend(["--mode".into(), "plan".into()]);
        }
        match &request.tools {
            ToolSelection::HostPolicy => {}
            ToolSelection::AllowList(tools) => {
                argv.extend(["--available-tools".into(), tools.join(",")])
            }
            // Empty --available-tools semantics are not established by the docs.
            ToolSelection::Disabled => return Err(AdapterError::Unsupported("disable-all-tools")),
        }
        let degradations = worker_degradations(request, false)?;
        let mut run = prepared_run(validated, "copilot", argv, degradations)?;
        run.missing_runtime_evidence
            .push("native.cli_version_requires_deployment_check".into());
        Ok(run)
    }
    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        decode(bytes, outcome, max_bytes, Dialect::Copilot)
    }
}

/// OpenCode's inspected dev CLI contract uses separate --model and --variant.
/// V2's model#variant syntax is a different dialect and is not silently assumed.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenCodeAdapter;
impl HarnessAdapter for OpenCodeAdapter {
    fn host(&self) -> &'static str {
        "opencode"
    }
    fn prepare(&self, validated: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        let request = validated.request();
        if request.host.as_str() != self.host() {
            return Err(AdapterError::HostMismatch);
        }
        if !request
            .model
            .as_str()
            .split_once('/')
            .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
        {
            return Err(AdapterError::Unsupported(
                "provider-qualified-model-required",
            ));
        }
        if !matches!(request.tools, ToolSelection::HostPolicy) {
            return Err(AdapterError::Unsupported("tool-selection"));
        }
        let effort =
            serde_json::to_value(request.effort).map_err(|_| AdapterError::Invalid("effort"))?;
        let effort = effort.as_str().ok_or(AdapterError::Invalid("effort"))?;
        let argv = vec![
            "run".into(),
            "--model".into(),
            request.model.to_string(),
            "--variant".into(),
            effort.into(),
            "--format".into(),
            "json".into(),
            "--dir".into(),
            request.workspace.clone(),
        ];
        let degradations = worker_degradations(request, false)?;
        let mut run = prepared_run(validated, "opencode", argv, degradations)?;
        run.missing_runtime_evidence.extend([
            "native.cli_variant_mapping_requires_deployment_check".into(),
            "native.sandbox_unavailable_requires_supervisor_confinement".into(),
        ]);
        Ok(run)
    }
    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max_bytes: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        decode(bytes, outcome, max_bytes, Dialect::OpenCode)
    }
}
