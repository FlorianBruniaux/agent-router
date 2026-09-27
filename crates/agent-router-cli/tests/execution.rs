use assert_cmd::Command;
use router_adapter_api::{HostObservation, SnapshotValidation, ValidatedHostObservation};
use router_catalog::Catalog;
use router_policy::PhaseRouter;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("agent-router"))
}
fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn output(assert: assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    store: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join("src")).unwrap();
        fs::write(repo.join("src/a"), "base").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "src"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "fixture",
            ],
        ] {
            assert!(
                std::process::Command::new("/usr/bin/git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let source = output(
            command()
                .args(["source-state", "--workspace"])
                .arg(&repo)
                .arg("--json")
                .assert()
                .success(),
        );
        let mut request: PhaseRouteRequest = serde_json::from_str(include_str!(
            "../../../fixtures/examples/phase-request.json"
        ))
        .unwrap();
        request.phase = StageKind::Execute;
        request
            .task
            .allowed_mutations
            .remove(&Mutation::WorkspaceWrite);
        request.candidates.truncate(1);
        request.candidates[0].sandbox = PhaseSandbox::ReadOnly;
        let plan = build_plan(
            &request.task,
            WorkflowPreset::DirectSafe,
            PlanLimits::default(),
        )
        .unwrap();
        let mut snapshot: HostObservation = serde_json::from_str(include_str!(
            "../../../fixtures/examples/phase-snapshot.json"
        ))
        .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        snapshot.observed_at_unix_seconds = now;
        for e in &mut snapshot.evidence {
            e.observed_at_unix_seconds = now;
        }
        let snapshot = snapshot.seal().unwrap();
        let validated = ValidatedHostObservation::from_observation(
            snapshot.clone(),
            SnapshotValidation {
                now_unix_seconds: now,
                max_age_seconds: 300,
            },
        )
        .unwrap();
        let catalog_bytes = include_bytes!("../../../fixtures/examples/phase-catalog.json");
        let catalog = Catalog::from_json(catalog_bytes).unwrap();
        let router = PhaseRouter::stable_v2();
        let context = router
            .context(request.clone(), &validated, &catalog)
            .unwrap();
        let receipt = router.route(&context).unwrap();
        let selected = &receipt.selected.as_ref().unwrap().candidate;
        let execution = ExecutionRequest {
            schema_version: 1,
            receipt_id: receipt.receipt_id.clone(),
            candidate_id: selected.id.clone(),
            phase: StageKind::Execute,
            host: validated.host().harness.clone(),
            model: selected.model.clone(),
            effort: selected.effort,
            workspace: repo.to_str().unwrap().into(),
            source_state: source["source_state"].as_str().unwrap().into(),
            scope: vec!["src".into()],
            sandbox: ExecutionSandbox::ReadOnly,
            required_capabilities: router
                .execution_capabilities(&context, receipt.selected.as_ref().unwrap())
                .unwrap(),
            tools: ToolSelection::Disabled,
            agent: None,
            skills: vec![],
            prompt: "private runtime prompt".into(),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            allow_degradation: false,
            timeout_ms: 500,
            max_output_bytes: 16384,
        };
        write(&root.join("context.json"), &request);
        write(&root.join("snapshot.json"), &snapshot);
        write(&root.join("request.json"), &execution);
        write(
            &root.join("plan.json"),
            &json!({"schema_version":1,"plan":plan,"assumptions":[]}),
        );
        fs::write(root.join("catalog.json"), catalog_bytes).unwrap();
        write(
            &root.join("definition.json"),
            &json!({"schema_version":1,"run_id":"cli-run","context":"context.json","host_snapshot":"snapshot.json","catalog":"catalog.json","policy":null,"plan":"plan.json","node_id":"execute","request":"request.json"}),
        );
        write(
            &root.join("sandbox.json"),
            &json!({"schema_version":1,"programs":{"codex":"/bin/cat"},"read_roots":[],"environment_names":[],"helper":"/bin/cat","bubblewrap":null,"limits":{"cpu_seconds":1,"address_space_bytes":null,"max_open_files":64,"max_processes":64,"max_file_bytes":1048576}}),
        );
        Self {
            _temp: temp,
            store: root.join("store"),
            root,
        }
    }
    fn run(&self) -> Command {
        let mut c = command();
        c.args(["run", "--definition"])
            .arg(self.root.join("definition.json"))
            .arg("--store")
            .arg(&self.store)
            .arg("--sandbox")
            .arg(self.root.join("sandbox.json"))
            .arg("--json");
        c
    }
}

#[test]
fn review_binds_definition_and_sandbox_then_cancel_is_observable() {
    let f = Fixture::new();
    let review = output(f.run().assert().success());
    assert_eq!(review["executed"], false);
    assert_eq!(review["run"]["state"], "awaiting_approval");
    assert!(!review.to_string().contains("private runtime prompt"));
    assert_ne!(review["approval_digest"], review["run"]["approval_digest"]);
    f.run()
        .args([
            "--approve",
            review["run"]["approval_digest"].as_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "runtime.approval.required_or_changed",
        ));
    let mut config: Value =
        serde_json::from_slice(&fs::read(f.root.join("sandbox.json")).unwrap()).unwrap();
    config["limits"]["cpu_seconds"] = json!(2);
    write(&f.root.join("sandbox.json"), &config);
    f.run()
        .args(["--approve", review["approval_digest"].as_str().unwrap()])
        .assert()
        .code(2);
    command()
        .args(["cancel", "--store"])
        .arg(&f.store)
        .args(["--run-id", "cli-run", "--json"])
        .assert()
        .success();
    let status = output(
        command()
            .args(["status", "--store"])
            .arg(&f.store)
            .args(["--run-id", "cli-run", "--json"])
            .assert()
            .success(),
    );
    assert_eq!(status["run"]["state"], "cancelled");
    let events = output(
        command()
            .args(["events", "--store"])
            .arg(&f.store)
            .args(["--run-id", "cli-run", "--json"])
            .assert()
            .success(),
    );
    assert!(events["events"].as_array().unwrap().len() >= 3);
    command()
        .args(["artifacts", "--definition"])
        .arg(f.root.join("definition.json"))
        .arg("--store")
        .arg(&f.store)
        .args(["keep", "--json"])
        .assert()
        .success();
}

#[test]
fn memory_limit_omission_is_not_implicit_authorization_to_drop_it() {
    let f = Fixture::new();
    let mut config: Value =
        serde_json::from_slice(&fs::read(f.root.join("sandbox.json")).unwrap()).unwrap();
    config["limits"]
        .as_object_mut()
        .unwrap()
        .remove("address_space_bytes");
    write(&f.root.join("sandbox.json"), &config);
    f.run()
        .assert()
        .code(2)
        .stderr(predicates::str::contains("request.json.invalid"));
    assert!(!f.store.exists());
}

#[test]
fn status_of_missing_store_does_not_create_it() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("missing");
    command()
        .args(["status", "--store"])
        .arg(&store)
        .args(["--run-id", "none", "--json"])
        .assert()
        .code(2);
    assert!(!store.exists());
}

#[test]
fn prepare_assembles_private_request_bundle_without_running_or_overwriting() {
    use std::collections::{BTreeMap, BTreeSet};
    let f = Fixture::new();
    let request: ExecutionRequest =
        serde_json::from_slice(&fs::read(f.root.join("request.json")).unwrap()).unwrap();
    let template = router_workflow::seal_prompt(PromptTemplate {
        schema_version: 1,
        id: "cli-template".into(),
        version: "v1".into(),
        template: "Report ${subject}".into(),
        variables: BTreeMap::from([("subject".into(), PromptVariableType::Text)]),
        compatible_hosts: BTreeSet::from(["codex".into()]),
        compatible_models: BTreeSet::from([request.model.clone()]),
        max_rendered_bytes: 4096,
        output_contract: "Evidence".into(),
        content_sha256: String::new(),
    })
    .unwrap();
    write(&f.root.join("prompt.json"), &template);
    let body = fs::read(f.root.join("prompt.json")).unwrap();
    let metadata = router_retrieval::MetadataDocument {
        schema_version: "1".into(),
        catalog_version: "fixture".into(),
        assets: vec![router_retrieval::AssetMetadata {
            id: "cli-template".into(),
            kind: router_retrieval::AssetKind::Prompt,
            trust: SkillTrust::TrustedLocal,
            task_kinds: vec![],
            triggers: vec![],
            description: "Report".into(),
            examples: vec![],
            tags: vec![],
            paths: vec![],
            anti_triggers: vec![],
            body: Some(router_retrieval::BodyReference {
                path: "prompt.json".into(),
                sha256: router_adapter_api::content_digest(&body),
            }),
        }],
    };
    write(&f.root.join("metadata.json"), &metadata);
    let index = output(
        command()
            .args(["index", "--metadata"])
            .arg(f.root.join("metadata.json"))
            .arg("--catalog")
            .arg(f.root.join("catalog.json"))
            .arg("--json")
            .assert()
            .success(),
    );
    write(&f.root.join("index.json"), &index);
    write(
        &f.root.join("variables.json"),
        &BTreeMap::from([(
            "subject".to_string(),
            PromptValue::Text("private input marker".into()),
        )]),
    );
    write(
        &f.root.join("options.json"),
        &json!({"schema_version":1,"workspace":request.workspace,"source_state":request.source_state,"target":"external_worker","allow_degradation":false,"timeout_ms":500,"max_output_bytes":16384,"max_input_bytes":65536,"now_unix_seconds":0,"max_snapshot_age_seconds":300}),
    );
    write(
        &f.root.join("prepare.json"),
        &json!({"schema_version":1,"run_id":"prepared-run","context":"context.json","host_snapshot":"snapshot.json","catalog":"catalog.json","policy":null,"policy_store":null,"plan":"plan.json","node_id":"execute","metadata":"index.json","body_root":".","template":"prompt.json","variables":"variables.json","options":"options.json"}),
    );
    let directory = f.root.join("prepared");
    let preview = output(
        command()
            .args(["prepare", "--manifest"])
            .arg(f.root.join("prepare.json"))
            .arg("--output-directory")
            .arg(&directory)
            .arg("--json")
            .assert()
            .success(),
    );
    assert_eq!(preview["executed"], false);
    assert_eq!(preview["approved"], false);
    assert!(!preview.to_string().contains("private input marker"));
    let prepared: ExecutionRequest =
        serde_json::from_slice(&fs::read(directory.join("request.json")).unwrap()).unwrap();
    assert!(prepared.prompt.contains("private input marker"));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(directory.join("request.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    command()
        .args(["prepare", "--manifest"])
        .arg(f.root.join("prepare.json"))
        .arg("--output-directory")
        .arg(&directory)
        .arg("--json")
        .assert()
        .code(2);
    let review = output(
        command()
            .args(["run", "--definition"])
            .arg(directory.join("definition.json"))
            .arg("--store")
            .arg(&f.store)
            .arg("--sandbox")
            .arg(f.root.join("sandbox.json"))
            .arg("--json")
            .assert()
            .success(),
    );
    assert_eq!(review["run"]["state"], "awaiting_approval");
    let feedback_store = f.root.join("policies");
    let active = router_feedback::FeedbackStore::open(&feedback_store)
        .unwrap()
        .register_baseline(
            PhasePolicy::default(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(f.root.join("prepare.json")).unwrap()).unwrap();
    manifest["policy_store"] = json!(feedback_store);
    write(&f.root.join("prepare.json"), &manifest);
    let active_directory = f.root.join("prepared-active");
    command()
        .args(["prepare", "--manifest"])
        .arg(f.root.join("prepare.json"))
        .arg("--output-directory")
        .arg(&active_directory)
        .arg("--json")
        .assert()
        .success();
    let definition_path = active_directory.join("definition.json");
    let mut definition: Value =
        serde_json::from_slice(&fs::read(&definition_path).unwrap()).unwrap();
    assert!(definition["policy"].is_null());
    assert_eq!(definition["policy_artifact_id"], active.artifact_id);
    let active_review = output(
        command()
            .args(["run", "--definition"])
            .arg(&definition_path)
            .arg("--store")
            .arg(&f.store)
            .arg("--sandbox")
            .arg(f.root.join("sandbox.json"))
            .arg("--json")
            .assert()
            .success(),
    );
    definition["policy_artifact_id"] = json!("0".repeat(64));
    write(&definition_path, &definition);
    command()
        .args(["run", "--definition"])
        .arg(&definition_path)
        .arg("--store")
        .arg(&f.store)
        .arg("--sandbox")
        .arg(f.root.join("sandbox.json"))
        .arg("--json")
        .assert()
        .code(2)
        .stderr(predicates::str::contains("policy.active.changed"));
    definition["policy"] = json!("policy.json");
    definition["policy_store"] = Value::Null;
    definition["policy_artifact_id"] = Value::Null;
    write(&definition_path, &definition);
    command()
        .args(["run", "--definition"])
        .arg(&definition_path)
        .arg("--store")
        .arg(&f.store)
        .arg("--sandbox")
        .arg(f.root.join("sandbox.json"))
        .args([
            "--approve",
            active_review["approval_digest"].as_str().unwrap(),
            "--json",
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "runtime.approval.required_or_changed",
        ));
}
