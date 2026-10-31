// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::{Config, EntryResult, Submission};
use router_adapter_api::content_digest;
use router_catalog::Catalog;
use router_policy::PhaseRouter;
use router_runtime::*;
use router_types::*;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::TcpListener,
    path::Path,
};

pub(super) const HOST: &str = "finite-entry-linux-local";
pub(super) const RUN: &str = "entry";
pub(super) const TOOLS: [&str; 3] = [
    "mcp__finite__read_file",
    "mcp__finite__write_file",
    "mcp__finite__exec",
];
pub(super) fn phase(
    s: &Submission,
    task: NormalizedTask,
    config_hash: &str,
    execution_budget: &TaskBudget,
) -> EntryResult<(PhaseRouteRequest, Catalog, PhaseRouter)> {
    let mut restrictions = EffectiveRestrictions {
        schema_version: 1,
        sources: vec![config_hash.into()],
        allowed_models: Some([s.model.clone()].into()),
        allowed_capabilities: Some(task.required_capabilities.clone()),
        allowed_mutations: Some(task.allowed_mutations.clone()),
        max_cost_microusd: Some(task.budget.max_cost_microusd),
        max_tokens: Some(task.budget.max_tokens),
        max_concurrency: Some(1),
        preferred_model: Some(s.model.clone()),
        policy_hash: String::new(),
    };
    restrictions.policy_hash = content_digest(&serde_json::to_vec(&restrictions)?);
    let candidate = PhaseCandidate {
        id: "operator-selected".into(),
        model: s.model.clone(),
        effort: s.effort,
        agent: None,
        skills: BTreeSet::new(),
        tools: TOOLS.into_iter().map(str::to_owned).collect(),
        tool_policy: PhaseToolPolicy::Restricted,
        sandbox: PhaseSandbox::WorkspaceWrite,
        verification: PhaseVerification::Independent,
        required_capabilities: task.required_capabilities.clone(),
        required_mutations: task.allowed_mutations.clone(),
        estimated_cost_microusd: 0,
        estimated_tokens: execution_budget.max_tokens,
        estimated_elapsed_ms: execution_budget.max_elapsed_ms,
        relevance_micros: 0,
    };
    let assets = TOOLS
        .into_iter()
        .map(|id| PhaseRegisteredAsset {
            kind: PhaseAssetKind::Tool,
            id: id.into(),
            content_sha256: content_digest(include_bytes!(
                "../../../router-runtime/src/finite_tools/transport.rs"
            )),
            trust: SkillTrust::TrustedLocal,
            license: "Apache-2.0".into(),
            permission: CapabilitySupport::Supported,
            required_capabilities: [Capability::Tools].into(),
            required_mutations: BTreeSet::new(),
            allowed_sandboxes: [PhaseSandbox::WorkspaceWrite].into(),
            maximum_sensitivity: DataSensitivity::Public,
        })
        .collect();
    let catalog = Catalog::from_json(&serde_json::to_vec(
        &json!({"schema_version":1,"version":"local-scripted-entry-v1","models":[{"id":s.model,"stages":["execute"],"efforts":[s.effort],"capabilities":task.required_capabilities,"quality_micros":1,"cost_micros":0,"latency_micros":1,"source":"Operator-pinned local scripted transport; no provider availability or price claim"}],"skills":[]}),
    )?)?;
    let phase = PhaseRouteRequest {
        schema_version: 2,
        task,
        phase: StageKind::Execute,
        restrictions,
        permissions: BTreeMap::from([
            (
                PhasePermission::ModelInference,
                CapabilitySupport::Supported,
            ),
            (PhasePermission::WorkspaceRead, CapabilitySupport::Supported),
            (
                PhasePermission::WorkspaceWrite,
                CapabilitySupport::Supported,
            ),
            (PhasePermission::Network, CapabilitySupport::Supported),
        ]),
        registry: PhaseAssetRegistry {
            schema_version: 1,
            version: "finite-entry-tools-v1".into(),
            assets,
        },
        candidates: vec![candidate],
        previous_model: None,
    };
    // A caller pins one model and effort. Eligibility still runs; utility does
    // not pretend to compare prices or quality of authored local responses.
    let router = PhaseRouter::new(PhasePolicy {
        version: "finite-entry-pinned-v1".into(),
        quality_weight: 0,
        relevance_weight: 0,
        cost_weight: 0,
        latency_weight: 0,
        max_alternatives: 0,
        ..Default::default()
    })?;
    Ok((phase, catalog, router))
}
pub(super) fn seed(
    s: &Submission,
    task: &NormalizedTask,
    workspace: &Path,
    source: &str,
) -> EntryResult<PreparedRun> {
    Ok(PreparedRun {
        schema_version: 1,
        receipt_id: "a".repeat(64),
        source_state: source.into(),
        scope: vec![s.output.clone()],
        authorization_digest: "b".repeat(64),
        snapshot_hash: "c".repeat(64),
        host: HarnessId::parse("codex")?,
        requested_model: s.model.clone(),
        requested_effort: s.effort,
        program: "finite-client".into(),
        argv: vec!["--worker".into()],
        cwd: workspace.to_str().ok_or("path encoding")?.into(),
        stdin: PreparedInput::new(vec![]),
        mode: ExecutionMode::Execute,
        target: ExecutionTarget::ExternalWorker,
        sandbox: ExecutionSandbox::WorkspaceWrite,
        timeout_ms: task.budget.max_elapsed_ms,
        max_output_bytes: 65536,
        degradations: vec![],
        missing_runtime_evidence: vec![],
        parent_model_changed: false,
    })
}
pub(super) fn review(
    c: &Config,
    s: &Submission,
    run: &PreparedRun,
    store: &Path,
    listener: TcpListener,
    workspace: FiniteClientWorkspace,
    id: &str,
) -> EntryResult<FiniteClientReview> {
    let broker = OwnedBroker::from_isolated_listener(listener)?;
    let state = broker.prepare_codex_tools_state(store, run)?;
    let pin = |name: &str| c.programs.get(name).cloned().ok_or("program pin");
    let manifest = FiniteClientManifest {
        schema_version: 1,
        task_id: id.into(),
        host_snapshot_id: HOST.into(),
        client: pin("client")?,
        native: pin("native")?,
        helper: pin("helper")?,
        wrapper: pin("guard")?,
        profile: pin("profile")?,
        configuration: state.configuration().clone(),
        private_state: state.path().to_str().ok_or("path encoding")?.into(),
        native_version: "0.159.2-linux-arm64-musl".into(),
        broker_endpoint: broker.endpoint().into(),
        broker_instance_sha256: broker.instance_sha256().into(),
    };
    let public = s
        .public_files
        .iter()
        .map(|(n, f)| (n.clone(), f.sha256.clone()))
        .collect();
    Ok(FiniteClientReview::with_codex_state(
        manifest,
        run,
        broker,
        ResourceLimits::default(),
        state,
    )?
    .with_write_workspace(workspace)?
    .with_linux_namespace(pin("namespace")?)?
    .with_mcp_tools(
        public,
        finite_tools::Limits {
            max_calls: 8,
            max_file_bytes: 32768,
            max_output_bytes: 8192,
            max_total_output_bytes: 32768,
            max_exec_ms: 5000,
            max_total_exec_ms: 15000,
            max_session_ms: run.timeout_ms,
            max_scratch_entries: 64,
            max_scratch_bytes: 16777216,
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routed_work_fits_the_planned_node_not_the_whole_task() {
        let task:NormalizedTask=serde_json::from_value(json!({
            "objective":"Correct public score", "kind":"implement","deliverables":["lib.rs"],
            "scope":["lib.rs"],"risk":"medium","sensitivity":"public","constraints":[],"dependencies":[],
            "acceptance":["Score matches the public contract"],
            "allowed_mutations":["network","workspace_write"],"required_capabilities":["tools","network","workspace_write"],
            "budget":{"max_cost_microusd":10000,"max_tokens":6000,"max_elapsed_ms":60000}
        })).unwrap();
        let s:Submission=serde_json::from_value(json!({"schema_version":1,"kind":"submit","task":task,
            "risk_context":{"schema_version":1,"environment":"local","reversibility":"reversible","operations":["workspace_change"]},
            "model":"gpt-6-luna","effort":"medium","output":"lib.rs","public_files":{}})).unwrap();
        let plan = router_workflow::build_plan(
            &task,
            WorkflowPreset::DirectSafe,
            router_workflow::PlanLimits::default(),
        )
        .unwrap();
        let execute = plan.nodes.iter().find(|n| n.id == "execute").unwrap();
        assert_eq!(execute.budget.max_tokens, 3000);
        assert_eq!(execute.budget.max_elapsed_ms, 30000);
        let (phase, _, _) = phase(&s, task.clone(), "configuration", &execute.budget).unwrap();
        let candidate = &phase.candidates[0];
        assert!(candidate.estimated_tokens <= execute.budget.max_tokens);
        assert!(candidate.estimated_elapsed_ms <= execute.budget.max_elapsed_ms);
        assert_eq!(
            phase.task.budget.max_tokens, 6000,
            "the approved task budget must remain unchanged"
        );
    }
}
