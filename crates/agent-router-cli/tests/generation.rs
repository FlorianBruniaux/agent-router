//! Synthetic metadata only. No provider, credentials or executable skills.
use assert_cmd::Command;
use router_types::*;
use router_workflow::{PlanLimits, build_plan, seal_execution_contract};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifest: Value,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/examples");
        let mut request: PhaseRouteRequest =
            serde_json::from_slice(&fs::read(fixtures.join("phase-request.json")).unwrap())
                .unwrap();
        request.task.allowed_mutations.clear();
        let plan = build_plan(
            &request.task,
            WorkflowPreset::DirectSafe,
            PlanLimits::default(),
        )
        .unwrap();
        let contract = seal_execution_contract(
            WorkflowExecutionContract {
                schema_version: 1,
                instance_id: "synthetic-cli-generation".into(),
                plan_id: plan.plan_id.clone(),
                workspace: root.to_string_lossy().into(),
                source_state: "a".repeat(64),
                contract_sha256: String::new(),
                nodes: plan
                    .nodes
                    .iter()
                    .map(|node| {
                        (
                            node.id.clone(),
                            NodeExecutionContract {
                                phase: if node.id == "verify" {
                                    StageKind::Verify
                                } else {
                                    StageKind::Execute
                                },
                                dependencies: node
                                    .dependencies
                                    .iter()
                                    .map(|id| (id.clone(), DependencyRequirement::Verified))
                                    .collect(),
                            },
                        )
                    })
                    .collect(),
            },
            &plan,
        )
        .unwrap();
        write(&root.join("plan.json"), &plan);
        write(&root.join("contract.json"), &contract);
        let nodes:Vec<_> = plan.nodes.iter().map(|node| {
            let mut request=request.clone();
            request.phase=contract.nodes[&node.id].phase;
            write(&root.join(format!("{}.json",node.id)), &request);
            json!({"node_id":node.id,"context":format!("{}.json",node.id),"host_snapshot":fixtures.join("phase-snapshot.json"),"catalog":fixtures.join("phase-catalog.json"),"policy":null,"asset_authority":null,"dimensions":null})
        }).collect();
        let compatibility:Vec<_>=plan.nodes.iter().flat_map(|node|node.dependencies.iter().map(|source|json!({"source":source,"target":node.id,"allowed_pairs":[["primary","primary"],["primary","secondary"],["secondary","primary"],["secondary","secondary"]]}))).collect();
        let manifest = json!({"schema_version":1,"plan":"plan.json","workflow_contract":"contract.json","policy":PlanGenerationPolicy::default(),"nodes":nodes,"compatibility":compatibility,"snapshot_max_age_seconds":30});
        Self {
            _temp: temp,
            root,
            manifest,
        }
    }
    fn command(&self) -> Command {
        write(&self.root.join("generation.json"), &self.manifest);
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
        cmd.args(["generate-plan", "--manifest"])
            .arg(self.root.join("generation.json"))
            .args(["--now", "110", "--json"]);
        cmd
    }
    fn report(&self) -> Value {
        let output = self.command().assert().success();
        serde_json::from_slice(&output.get_output().stdout).unwrap()
    }
}
#[test]
fn generates_stable_global_choices_and_binds_every_source() {
    let fixture = Fixture::new();
    let before = fixture.report();
    assert_eq!(before, fixture.report());
    assert_eq!(
        before["report"]["selected"]["choices"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        before["report"]["selected"]["choices"][0]["candidate_id"],
        "primary"
    );
    let path = fixture.root.join("execute.json");
    let mut request: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    request["candidates"][0]["estimated_cost_microusd"] = json!(101);
    write(&path, &request);
    let after = fixture.report();
    assert_ne!(before["source_sha256"], after["source_sha256"]);
    assert_ne!(
        before["report"]["input_hash"],
        after["report"]["input_hash"]
    );
}
#[test]
fn unknown_compatibility_abstains_and_never_creates_a_run() {
    let mut fixture = Fixture::new();
    fixture.manifest["compatibility"][0]["allowed_pairs"] = Value::Null;
    let output = fixture.command().assert().code(3);
    let value: Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(value["report"]["abstention"], "unknown_compatibility");
    assert!(value["report"]["selected"].is_null());
    assert!(!fixture.root.join("runs.sqlite3").exists());
}
#[test]
fn unknown_fields_stale_snapshots_and_invalid_node_scope_fail_closed() {
    let mut fixture = Fixture::new();
    fixture.manifest["unexpected_grant"] = json!(true);
    fixture.command().assert().code(2);
    fixture
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("unexpected_grant");
    fixture.manifest["snapshot_max_age_seconds"] = json!(1);
    fixture.command().assert().code(2);
    fixture.manifest["snapshot_max_age_seconds"] = json!(30);
    fixture.manifest["nodes"][0]["node_id"] = json!("not-authorized");
    fixture.command().assert().code(2);
}
