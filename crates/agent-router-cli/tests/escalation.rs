//! Synthetic attempts and private local journals. No provider or real model.
#![cfg(all(unix, feature = "claude"))]
#[path = "support/native_fixture.rs"]
mod native_fixture;
use native_fixture::{Fixture as Files, checked, command, write};
use router_adapter_api::{
    AdapterError, HarnessAdapter, SnapshotValidation, ValidatedExecutionRequest,
    ValidatedHostObservation, prepared_run,
};
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_runtime::{Cancellation, Runtime, RuntimeLimits, SandboxPolicy, SandboxSupervisor};
use router_types::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
#[track_caller]
fn rejected(cmd: &mut Command, expected: &str) {
    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "expected {expected}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}
struct Fixture {
    files: Files,
    router: PhaseRouter,
    context: ValidatedPhaseContext,
    plan: WorkflowPlan,
}
impl Fixture {
    fn new() -> Self {
        Self::build(false)
    }
    fn native() -> Self {
        Self::build(true)
    }
    fn build(native: bool) -> Self {
        let files = if native {
            Files::new("exit 1", 5_000)
        } else {
            Files::metadata("exit 1", 5_000)
        };
        let root = &files.root;
        let mut route: PhaseRouteRequest = read(&root.join("context.json"));
        route
            .task
            .allowed_mutations
            .remove(&Mutation::WorkspaceWrite);
        route.candidates[0].sandbox = PhaseSandbox::ReadOnly;
        let mut second = route.candidates[0].clone();
        second.id = "secondary".into();
        second.model = ModelId::parse("fixture-second").unwrap();
        route.candidates.push(second);
        let snapshot = ValidatedHostObservation::from_observation(
            read(&root.join("snapshot.json")),
            SnapshotValidation {
                now_unix_seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                max_age_seconds: 300,
            },
        )
        .unwrap();
        let catalog = Catalog::from_json(&fs::read(root.join("catalog.json")).unwrap()).unwrap();
        let original: ExecutionRequest = read(&root.join("request.json"));
        let assets = native_fixture::asset_inputs(
            &route,
            &snapshot,
            &catalog,
            &files.repo,
            &original.source_state,
            snapshot.observation().observed_at_unix_seconds,
        );
        native_fixture::write_asset_authority(root, &assets);
        let router = PhaseRouter::stable_v2();
        let context = router
            .context_with_assets(
                route.clone(),
                &snapshot,
                &catalog,
                assets,
                snapshot.observation().observed_at_unix_seconds,
            )
            .unwrap();
        let plan = router_workflow::build_plan(
            &route.task,
            WorkflowPreset::DirectSafe,
            router_workflow::PlanLimits::default(),
        )
        .unwrap();
        write(&root.join("context.json"), &route);
        write(&root.join("plan.json"), &plan);
        let prompt = router_workflow::seal_prompt(PromptTemplate {
            schema_version: 1,
            id: "escalation-template".into(),
            version: "1".into(),
            template: "Inspect ${subject}".into(),
            variables: BTreeMap::from([("subject".into(), PromptVariableType::Text)]),
            compatible_hosts: BTreeSet::from(["claude-code".into()]),
            compatible_models: catalog.models().map(|(id, _)| id.clone()).collect(),
            max_rendered_bytes: 4096,
            output_contract: "Report findings".into(),
            content_sha256: String::new(),
        })
        .unwrap();
        write(&root.join("prompt.json"), &prompt);
        let metadata: router_retrieval::MetadataDocument = serde_json::from_value(json!({"schema_version":"1","catalog_version":catalog.version(),"assets":[{"kind":"prompt","id":prompt.id,"trust":"trusted_local","description":"report","body":{"path":"prompt.json","sha256":router_adapter_api::content_digest(&fs::read(root.join("prompt.json")).unwrap())}}]})).unwrap();
        write(
            &root.join("metadata.json"),
            &router_retrieval::MetadataEnvelope::new(metadata, &catalog.content_hash().unwrap())
                .unwrap(),
        );
        write(
            &root.join("variables.json"),
            &BTreeMap::from([(
                "subject",
                PromptValue::Text("PRIVATE_ESCALATION_SUBJECT".into()),
            )]),
        );
        write(
            &root.join("options.json"),
            &json!({"schema_version":1,"workspace":files.repo,"source_state":original.source_state,"target":"external_worker","allow_degradation":false,"timeout_ms":5000,"max_output_bytes":16384,"max_input_bytes":65536,"now_unix_seconds":0,"max_snapshot_age_seconds":300}),
        );
        write(
            &root.join("prepare.json"),
            &json!({"schema_version":1,"run_id":"source","context":"context.json","host_snapshot":"snapshot.json","catalog":"catalog.json","policy":null,"asset_authority":"asset-authority.json","plan":"plan.json","node_id":"execute","metadata":"metadata.json","body_root":".","template":"prompt.json","variables":"variables.json","options":"options.json"}),
        );
        checked(
            command()
                .args(["prepare", "--manifest"])
                .arg(root.join("prepare.json"))
                .arg("--output-directory")
                .arg(root.join("source"))
                .arg("--json"),
            0,
        );
        write(
            &root.join("escalation-policy.json"),
            &EscalationPolicy {
                schema_version: 1,
                minimum_failed_attempts: 2,
                retryable_failures: vec![RunFailure::Crash],
                transitions: vec![EscalationTransition {
                    from_candidate_id: "primary".into(),
                    to_candidate_id: "secondary".into(),
                }],
            },
        );
        let mut sandbox: Value = read(&root.join("sandbox.json"));
        sandbox["output_retention"] = json!({"mode":"disabled"});
        sandbox["feedback_store"] = Value::Null;
        write(&root.join("sandbox.json"), &sandbox);
        // A fixed binary handles both candidates, so changing the selection never
        // changes the approved executable hash between review and dispatch.
        fs::write(root.join("fake-claude"), format!("#!/bin/sh\nset -eu\nmodel=\nwhile [ \"$#\" -gt 0 ]; do\ncase \"$1\" in --model) model=\"$2\"; shift 2;; *) shift;; esac\ndone\n/bin/cat >/dev/null\n[ \"$model\" = fixture-best ] && exit 1\n[ \"$model\" = fixture-second ] || exit 42\nprintf '%s' {}\n", native_fixture::shell_quote(native_fixture::STREAM))).unwrap();
        Self {
            files,
            router,
            context,
            plan,
        }
    }
    fn source(&self) -> std::path::PathBuf {
        self.files.root.join("source/definition.json")
    }
    fn bound(&self, attempt: u32) -> Command {
        let mut cmd = command();
        cmd.args(["escalation", "bound", "--store"])
            .arg(&self.files.store)
            .arg("--source-definition")
            .arg(self.source())
            .args([
                "--attempt-run-id",
                "source",
                "--attempt",
                &attempt.to_string(),
                "--max-cost-microusd",
                "10",
                "--max-tokens",
                "10",
                "--json",
            ]);
        cmd
    }
    fn propose(&self) -> Command {
        let mut cmd = command();
        cmd.args(["escalation", "propose", "--store"])
            .arg(&self.files.store)
            .arg("--source-definition")
            .arg(self.source())
            .arg("--policy")
            .arg(self.files.root.join("escalation-policy.json"))
            .args(["--max-attempts", "3", "--json"]);
        cmd
    }
    fn attempts(&self) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let runtime = Runtime::open(&self.files.store, RuntimeLimits::default()).unwrap();
            let request = read(&self.files.root.join("source/request.json"));
            let pending = runtime
                .review(
                    "source",
                    &self.router,
                    &self.context,
                    request,
                    self.plan.clone(),
                    "execute",
                )
                .await
                .unwrap();
            let mut pending = pending;
            for attempt in 1..=2 {
                let approved = runtime
                    .approve(pending.clone(), pending.approval_digest())
                    .unwrap();
                let completion = runtime
                    .execute(
                        approved,
                        &FailedWorker,
                        &TestSupervisor,
                        Cancellation::default(),
                    )
                    .await
                    .unwrap();
                assert_eq!(completion.run.failure, Some(RunFailure::Crash));
                assert_eq!(completion.run.attempts, attempt);
                if attempt == 1 {
                    pending = runtime.retry(pending).unwrap();
                }
            }
        });
    }
    fn account(&self) {
        for attempt in 1..=2 {
            let preview = checked(&mut self.bound(attempt), 0);
            assert_eq!(preview["approved"], false);
            let digest = preview["preview"]["approval_digest"].as_str().unwrap();
            checked(self.bound(attempt).args(["--approve", digest]), 0);
        }
    }
}
struct FailedWorker;
impl HarnessAdapter for FailedWorker {
    fn host(&self) -> &'static str {
        "claude-code"
    }
    fn prepare(&self, request: &ValidatedExecutionRequest) -> Result<PreparedRun, AdapterError> {
        prepared_run(
            request,
            "/bin/sh",
            vec!["-c".into(), "exit 1".into()],
            vec![],
        )
    }
    fn decode_stream(
        &self,
        _: &[u8],
        _: TransportOutcome,
        _: usize,
    ) -> Result<RuntimeEvidence, AdapterError> {
        Err(AdapterError::Unsupported("synthetic-test"))
    }
}
// Intentionally test-only. These ordinary tests establish journal/CLI behavior,
// not native OS sandbox enforcement; the separate ignored canary exercises it.
struct TestSupervisor;
impl SandboxSupervisor for TestSupervisor {
    fn resource_limits_enforced(&self) -> bool {
        true
    }
    fn command(
        &self,
        run: &PreparedRun,
        _: &SandboxPolicy,
    ) -> router_runtime::Result<tokio::process::Command> {
        let mut cmd = tokio::process::Command::new(&run.program);
        cmd.args(&run.argv).current_dir(&run.cwd).env_clear();
        Ok(cmd)
    }
}
#[test]
fn parent_accounting_approval_never_upgrades_unknown_cleanup() {
    let fixture = Fixture::new();
    fixture.attempts();
    rejected(
        &mut fixture.propose(),
        "escalation.attempt_evidence.missing_or_changed",
    );
    let preview = checked(&mut fixture.bound(1), 0);
    rejected(
        fixture.bound(1).args(["--approve", &"0".repeat(64)]),
        "escalation.approval.required_or_changed",
    );
    rejected(
        &mut fixture.propose(),
        "escalation.attempt_evidence.missing_or_changed",
    );
    checked(
        fixture.bound(1).args([
            "--approve",
            preview["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        0,
    );
    let second = checked(&mut fixture.bound(2), 0);
    checked(
        fixture.bound(2).args([
            "--approve",
            second["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        0,
    );
    rejected(
        &mut fixture.propose(),
        "escalation.attempt_evidence.missing_or_changed",
    );
    assert_eq!(preview["observed_cost_microusd"], Value::Null);
    assert_eq!(preview["observed_tokens"], Value::Null);
}

impl Fixture {
    fn run(&self, definition: &Path) -> Command {
        let mut cmd = command();
        cmd.args(["run", "--definition"])
            .arg(definition)
            .arg("--store")
            .arg(&self.files.store)
            .arg("--sandbox")
            .arg(self.files.root.join("sandbox.json"))
            .arg("--json");
        cmd
    }
    fn prepare_target(&self) -> Command {
        let mut manifest: Value = read(&self.files.root.join("prepare.json"));
        manifest["run_id"] = json!("target");
        manifest["escalation_decision"] = json!("proposal.json");
        write(&self.files.root.join("prepare-target.json"), &manifest);
        let mut cmd = command();
        cmd.args(["prepare", "--manifest"])
            .arg(self.files.root.join("prepare-target.json"))
            .arg("--output-directory")
            .arg(self.files.root.join("target"))
            .arg("--json");
        cmd
    }
    fn native_attempts(&self) {
        for attempt in 1..=2 {
            let mut cmd = self.run(&self.source());
            if attempt == 2 {
                cmd.arg("--retry");
            }
            let preview = checked(&mut cmd, 0);
            let done = checked(
                self.run(&self.source())
                    .args(["--approve", preview["approval_digest"].as_str().unwrap()]),
                3,
            );
            assert_eq!(done["completion"]["run"]["failure"], "crash");
            assert_eq!(done["completion"]["run"]["attempts"], attempt);
        }
    }
}
#[test]
#[ignore = "requires built native helper and real OS sandbox; no provider"]
fn native_cli_bounds_proposal_preparation_approval_dispatch_and_restart() {
    let fixture = Fixture::native();
    fixture.native_attempts();
    rejected(&mut fixture.propose(), "escalation.accounting.unknown");
    fixture.account();
    let proposal = checked(
        fixture
            .propose()
            .arg("--output")
            .arg(fixture.files.root.join("proposal.json")),
        0,
    );
    assert_eq!(
        proposal["preview"]["selection"]["candidate"]["id"],
        "secondary"
    );
    assert_eq!(proposal["preview"]["uses_parent_accounting_bounds"], true);
    let prepared = checked(&mut fixture.prepare_target(), 0);
    assert_eq!(prepared["executed"], false);
    let baseline: PhaseReceipt = read(&fixture.files.root.join("source/receipt.json"));
    let target_baseline: PhaseReceipt = read(&fixture.files.root.join("target/receipt.json"));
    assert_eq!(target_baseline, baseline);
    let request: ExecutionRequest = read(&fixture.files.root.join("target/request.json"));
    let source_request: ExecutionRequest = read(&fixture.files.root.join("source/request.json"));
    assert_eq!(request.prompt, source_request.prompt);
    assert_eq!(request.model.as_str(), "fixture-second");
    let target = fixture.files.root.join("target/definition.json");
    let review = checked(&mut fixture.run(&target), 0);
    assert_eq!(review["decision_origin"], "escalated");
    assert_eq!(review["baseline_receipt_id"], baseline.receipt_id);
    assert_eq!(
        review["escalation_decision"]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    rejected(
        fixture.run(&target).args(["--approve", &"0".repeat(64)]),
        "runtime.approval.required_or_changed",
    );
    let completion = checked(
        fixture
            .run(&target)
            .args(["--approve", review["approval_digest"].as_str().unwrap()]),
        0,
    );
    assert_eq!(completion["completion"]["run"]["state"], "verifying");
    assert_eq!(completion["completion"]["run"]["attempts"], 1);
    assert_eq!(completion["completion"]["objective"], "unknown");
    // A new CLI invocation must reconstruct the historical escalation for artifacts.
    checked(
        command()
            .args(["artifacts", "--definition"])
            .arg(&target)
            .arg("--store")
            .arg(&fixture.files.store)
            .args(["outputs", "--json"]),
        0,
    );
    rejected(fixture.run(&target).arg("--retry"), "runtime.invalid");
    for output in [proposal, prepared, review, completion] {
        assert!(!output.to_string().contains("PRIVATE_ESCALATION_SUBJECT"));
    }
}

#[test]
fn accounting_previews_are_single_use_and_bound_to_the_current_ledger() {
    let fixture = Fixture::new();
    fixture.attempts();
    let a = checked(&mut fixture.bound(1), 0);
    let b = checked(&mut fixture.bound(2), 0);
    checked(
        fixture.bound(1).args([
            "--approve",
            a["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        0,
    );
    rejected(
        fixture.bound(1).args([
            "--approve",
            a["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        "escalation.approval.required_or_changed",
    );
    rejected(
        fixture.bound(2).args([
            "--approve",
            b["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        "escalation.approval.required_or_changed",
    );
    let fresh = checked(&mut fixture.bound(2), 0);
    assert_ne!(
        fresh["preview"]["approval_digest"],
        b["preview"]["approval_digest"]
    );
    checked(
        fixture.bound(2).args([
            "--approve",
            fresh["preview"]["approval_digest"].as_str().unwrap(),
        ]),
        0,
    );
}
#[test]
fn composition_and_recursive_sources_are_rejected_before_opening_the_store() {
    let fixture = Fixture::new();
    let original: Value = read(&fixture.source());
    for field in ["generated_plan", "escalation_decision"] {
        let mut source = original.clone();
        source[field] = json!("not-a-readable-authority.json");
        write(&fixture.source(), &source);
        rejected(&mut fixture.bound(1), "escalation.source.origin_denied");
        assert!(!fixture.files.store.exists());
    }
    let mut source = original;
    source["escalation_decision"] = json!("unused");
    source["backend_decision"] = json!("unused");
    write(&fixture.source(), &source);
    rejected(&mut fixture.bound(1), "escalation.composition.denied");
    assert!(!fixture.files.store.exists());
}
#[test]
fn third_attempt_ceiling_is_required_and_bounded() {
    for limit in ["0", "1", "2", "6", "4294967295"] {
        rejected(
            command().args([
                "escalation",
                "propose",
                "--store",
                "unused",
                "--source-definition",
                "unused",
                "--policy",
                "unused",
                "--max-attempts",
                limit,
                "--json",
            ]),
            "cli.invalid",
        );
    }
    rejected(
        command().args([
            "escalation",
            "propose",
            "--store",
            "unused",
            "--source-definition",
            "unused",
            "--policy",
            "unused",
            "--json",
        ]),
        "cli.invalid",
    );
}
fn copy_store(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    fs::set_permissions(target, fs::metadata(source).unwrap().permissions()).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let next = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_store(&entry.path(), &next);
        } else {
            fs::copy(entry.path(), next).unwrap();
        }
    }
}
#[test]
#[ignore = "requires native helper and OS sandbox; pins, prompt and claim regressions"]
fn native_cli_escalation_rejects_changed_sources_stores_prompts_and_approval_limits() {
    let fixture = Fixture::native();
    fixture.native_attempts();
    fixture.account();
    checked(
        fixture
            .propose()
            .arg("--output")
            .arg(fixture.files.root.join("proposal.json")),
        0,
    );
    let path = fixture.files.root.join("proposal.json");
    let original: Value = read(&path);
    let variables = fixture.files.root.join("variables.json");
    let saved = fs::read(&variables).unwrap();
    write(
        &variables,
        &BTreeMap::from([("subject", PromptValue::Text("changed task".into()))]),
    );
    rejected(&mut fixture.prepare_target(), "runtime.invalid");
    assert!(!fixture.files.root.join("target").exists());
    fs::write(&variables, saved).unwrap();
    for (field, value, expected) in [
        (
            "expected_ledger_hash",
            json!("0".repeat(64)),
            "escalation.evidence.changed",
        ),
        (
            "source_material_sha256",
            json!("0".repeat(64)),
            "escalation.source.changed",
        ),
        (
            "expected_decision_id",
            json!("0".repeat(64)),
            "escalation.evidence.changed",
        ),
        ("max_attempts", json!(6), "escalation.document.invalid"),
        ("unexpected", json!(true), "request.json.invalid"),
    ] {
        let mut altered = original.clone();
        altered[field] = value;
        write(&path, &altered);
        rejected(&mut fixture.prepare_target(), expected);
        assert!(!fixture.files.root.join("target").exists());
    }
    write(&path, &original);
    let alias = fixture.files.root.join("source/alias.json");
    fs::copy(fixture.source(), &alias).unwrap();
    let mut altered = original.clone();
    altered["source_definition"] = json!(alias);
    write(&path, &altered);
    rejected(&mut fixture.prepare_target(), "escalation.source.changed");
    let clone = fixture.files.root.join("runtime-copy");
    copy_store(&fixture.files.store, &clone);
    altered = original.clone();
    altered["runtime_store"] = json!(clone);
    write(&path, &altered);
    rejected(&mut fixture.prepare_target(), "escalation.evidence.changed");
    write(&path, &original);
    checked(&mut fixture.prepare_target(), 0);
    let target = fixture.files.root.join("target/definition.json");
    let review = checked(&mut fixture.run(&target), 0);
    altered = original.clone();
    altered["max_attempts"] = json!(4);
    write(&path, &altered);
    rejected(
        fixture
            .run(&target)
            .args(["--approve", review["approval_digest"].as_str().unwrap()]),
        "runtime.approval.required_or_changed",
    );
    write(&path, &original);
    let mut duplicate: Value = read(&target);
    duplicate["run_id"] = json!("second-target");
    write(
        &fixture.files.root.join("target/second-definition.json"),
        &duplicate,
    );
    rejected(
        &mut fixture.run(&fixture.files.root.join("target/second-definition.json")),
        "runtime.approval.required_or_changed",
    );
    // Reopening the original after the rejected competing claim remains valid.
    assert_eq!(
        checked(&mut fixture.run(&target), 0)["approval_digest"],
        review["approval_digest"]
    );
    rejected(
        fixture.run(&fixture.source()).arg("--retry"),
        "runtime.invalid",
    );
}

#[test]
#[ignore = "requires native helper and OS sandbox; durable source ledger regression"]
fn native_cli_escalation_rejects_a_proposal_after_source_history_changes() {
    let fixture = Fixture::native();
    fixture.native_attempts();
    fixture.account();
    checked(
        fixture
            .propose()
            .arg("--output")
            .arg(fixture.files.root.join("proposal.json")),
        0,
    );
    // Trusted test caller explicitly authorizes a third ordinary source attempt.
    // This changes durable history, without forging or editing SQLite events.
    {
        let runtime = Runtime::open(
            &fixture.files.store,
            RuntimeLimits {
                max_attempts: 3,
                ..RuntimeLimits::default()
            },
        )
        .unwrap();
        let request = read(&fixture.files.root.join("source/request.json"));
        let pending = runtime
            .reopen(
                "source",
                &fixture.router,
                &fixture.context,
                request,
                fixture.plan.clone(),
                "execute",
            )
            .unwrap();
        runtime.retry(pending).unwrap();
    }
    rejected(
        &mut fixture.prepare_target(),
        "escalation.attempts.insufficient_or_not_retryable",
    );
    assert!(!fixture.files.root.join("target").exists());
}
