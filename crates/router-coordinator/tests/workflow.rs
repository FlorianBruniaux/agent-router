use router_adapter_api::{
    AdapterError, HarnessAdapter, HostObservation, SnapshotValidation, StreamDialect,
    ValidatedExecutionRequest, ValidatedHostObservation, decode_transport, prepared_run,
};
use router_catalog::Catalog;
use router_coordinator::*;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_runtime::{Runtime, RuntimeLimits, SandboxPolicy, SandboxSupervisor};
use router_types::*;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    router: PhaseRouter,
    snapshot: ValidatedHostObservation,
    catalog: Catalog,
    request: PhaseRouteRequest,
    definition: WorkflowDefinition,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap().join("repo");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/base.txt"), "base\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "src"],
            vec![
                "-c",
                "user.name=Coordinator Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "--no-gpg-sign",
                "-m",
                "base",
            ],
        ] {
            assert!(
                Command::new("/usr/bin/git")
                    .arg("-C")
                    .arg(&root)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut request: PhaseRouteRequest = serde_json::from_str(include_str!(
            "../../../fixtures/examples/phase-request.json"
        ))
        .unwrap();
        request.task.budget.max_elapsed_ms = 30_000;
        request.task.budget.max_tokens = 10_000;
        request.restrictions.max_tokens = Some(10_000);
        request.restrictions = router_workflow::merge_restrictions(&[RestrictionLayer {
            name: "fixture".into(),
            allowed_models: None,
            allowed_capabilities: request.restrictions.allowed_capabilities.clone(),
            allowed_mutations: request.restrictions.allowed_mutations.clone(),
            max_cost_microusd: Some(10_000),
            max_tokens: Some(10_000),
            max_concurrency: Some(2),
            preferred_model: None,
        }])
        .unwrap();
        let plan = router_workflow::build_plan(
            &request.task,
            WorkflowPreset::PlanExecuteVerify,
            router_workflow::PlanLimits::default(),
        )
        .unwrap();
        let execution = router_workflow::seal_execution_contract(
            WorkflowExecutionContract {
                schema_version: 1,
                instance_id: "fixture-workflow".into(),
                plan_id: plan.plan_id.clone(),
                workspace: root.to_str().unwrap().into(),
                source_state: router_runtime::source_state(&root).await.unwrap(),
                nodes: plan
                    .nodes
                    .iter()
                    .map(|n| {
                        (
                            n.id.clone(),
                            NodeExecutionContract {
                                phase: phase(&n.id),
                                dependencies: n
                                    .dependencies
                                    .iter()
                                    .map(|d| {
                                        (
                                            d.clone(),
                                            if n.id == "verify" {
                                                DependencyRequirement::ArtifactReadyForVerification
                                            } else {
                                                DependencyRequirement::Verified
                                            },
                                        )
                                    })
                                    .collect(),
                            },
                        )
                    })
                    .collect(),
                contract_sha256: String::new(),
            },
            &plan,
        )
        .unwrap();
        let artifacts = plan
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    NodeArtifactDeclaration {
                        outputs: vec![OutputDeclaration {
                            label: n.outputs[0].clone(),
                            source: if n.id == "execute" {
                                DeclaredOutputSource::Patch
                            } else {
                                DeclaredOutputSource::NativeFinalText
                            },
                            max_bytes: 8192,
                            retention_seconds: 3600,
                        }],
                        inputs: n
                            .dependencies
                            .iter()
                            .map(|d| ArtifactEdge {
                                producer: d.clone(),
                                label: format!("artifact:{d}"),
                                exposure: if n.id == "verify" {
                                    ArtifactExposure::ReadOnlyWorkspacePatch
                                } else {
                                    ArtifactExposure::PromptData
                                },
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        let definition = seal_definition(WorkflowDefinition {
            schema_version: 1,
            plan,
            execution,
            artifacts,
            failure_policy: WorkflowFailurePolicy::ContinueIndependent,
            max_input_bytes: 32 * 1024,
            definition_sha256: String::new(),
        })
        .unwrap();
        let mut observation: HostObservation = serde_json::from_str(include_str!(
            "../../../fixtures/examples/phase-snapshot.json"
        ))
        .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        observation.observed_at_unix_seconds = now;
        for evidence in &mut observation.evidence {
            evidence.observed_at_unix_seconds = now;
        }
        let snapshot = ValidatedHostObservation::from_observation(
            observation.seal().unwrap(),
            SnapshotValidation {
                now_unix_seconds: now,
                max_age_seconds: 300,
            },
        )
        .unwrap();
        Self {
            temp,
            root,
            router: PhaseRouter::stable_v2(),
            snapshot,
            catalog: Catalog::from_json(include_bytes!(
                "../../../fixtures/examples/phase-catalog.json"
            ))
            .unwrap(),
            request,
            definition,
        }
    }
    fn runtime(&self) -> Arc<Runtime> {
        Arc::new(
            Runtime::open(&self.temp.path().join("runtime"), RuntimeLimits::default()).unwrap(),
        )
    }
    fn coordinator(&self, runtime: Arc<Runtime>) -> Coordinator {
        Coordinator::open(&self.temp.path().join("coordinator"), runtime).unwrap()
    }
    fn material(&self, node: &str) -> (ValidatedPhaseContext, ExecutionRequest) {
        let mut route = self.request.clone();
        route.phase = self.definition.execution.nodes[node].phase;
        route.candidates.truncate(1);
        route.candidates[0].sandbox = if node == "execute" {
            PhaseSandbox::WorkspaceWrite
        } else {
            PhaseSandbox::ReadOnly
        };
        if node == "verify" {
            route.previous_model = Some(ModelId::parse("fixture-best").unwrap());
            route.candidates[0].model = ModelId::parse("fixture-second").unwrap();
            route.candidates[0].verification = PhaseVerification::Independent;
        }
        let context = self
            .router
            .context(route, &self.snapshot, &self.catalog)
            .unwrap();
        let receipt = self.router.route(&context).unwrap();
        let selected = receipt.selected.as_ref().unwrap();
        let request = ExecutionRequest {
            schema_version: 1,
            receipt_id: receipt.receipt_id.clone(),
            candidate_id: selected.candidate.id.clone(),
            phase: receipt.phase,
            host: self.snapshot.host().harness.clone(),
            model: selected.candidate.model.clone(),
            effort: selected.candidate.effort,
            workspace: self.root.to_str().unwrap().into(),
            source_state: self.definition.execution.source_state.clone(),
            scope: vec!["src".into()],
            sandbox: if node == "execute" {
                ExecutionSandbox::WorkspaceWrite
            } else {
                ExecutionSandbox::ReadOnly
            },
            required_capabilities: self
                .router
                .execution_capabilities(&context, selected)
                .unwrap(),
            tools: ToolSelection::Disabled,
            agent: None,
            skills: vec![],
            prompt: format!("private prompt for {node}"),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            allow_degradation: false,
            timeout_ms: 3000,
            max_output_bytes: 32 * 1024,
        };
        (context, request)
    }
    async fn review(&self, coordinator: &Coordinator, node: &str) -> PendingStep {
        let (context, request) = self.material(node);
        let proof = self
            .router
            .validate_receipt(&context, &self.router.route(&context).unwrap())
            .unwrap();
        coordinator
            .review_step(
                &self.definition,
                node,
                0,
                &self.router,
                &context,
                &proof,
                request,
            )
            .await
            .unwrap()
    }
}
fn phase(node: &str) -> StageKind {
    match node {
        "plan" => StageKind::Plan,
        "execute" => StageKind::Execute,
        _ => StageKind::Verify,
    }
}
struct FakeAdapter(&'static str);
impl HarnessAdapter for FakeAdapter {
    fn host(&self) -> &'static str {
        "codex"
    }
    fn prepare(
        &self,
        request: &ValidatedExecutionRequest,
    ) -> std::result::Result<PreparedRun, AdapterError> {
        prepared_run(request, "/bin/sh", vec!["-c".into(), self.0.into()], vec![])
    }
    fn decode_stream(
        &self,
        bytes: &[u8],
        outcome: TransportOutcome,
        max: usize,
    ) -> std::result::Result<RuntimeEvidence, AdapterError> {
        decode_transport(bytes, outcome, max, StreamDialect::Codex)
    }
}
struct FakeSupervisor;
impl SandboxSupervisor for FakeSupervisor {
    fn resource_limits_enforced(&self) -> bool {
        true
    }
    fn command(
        &self,
        run: &PreparedRun,
        _: &SandboxPolicy,
    ) -> router_runtime::Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new(&run.program);
        command
            .args(&run.argv)
            .current_dir(&run.cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        Ok(command)
    }
}
const PLAN: &str = "printf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Proposal: change src/base.txt; validate changed content.\"}}' '{\"type\":\"turn.completed\"}'";
const EXECUTE: &str =
    "printf 'changed\\n' > src/base.txt; printf '%s\\n' '{\"type\":\"turn.completed\"}'";
const VERIFY: &str = "IFS= read -r value < src/base.txt; test \"$value\" = changed || exit 9; printf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Independent patch view inspected.\"}}' '{\"type\":\"turn.completed\"}'";
fn record(runtime: &Runtime, step: &PendingStep) -> VerificationRecord {
    VerificationRecord {
        verifier: "explicit-parent".into(),
        evidence_sha256: "a".repeat(64),
        artifact_sha256: runtime
            .summary(&step.summary().run_id)
            .unwrap()
            .artifact_sha256,
        validation: VerificationOutcome::Passed,
        objective: VerificationOutcome::Passed,
    }
}
async fn run(
    coordinator: &Coordinator,
    definition: &WorkflowDefinition,
    step: &PendingStep,
    script: &'static str,
    supervisor: &dyn SandboxSupervisor,
) -> RunCompletion {
    let approved = coordinator
        .approve_step(definition, step.clone(), step.approval_digest())
        .unwrap();
    coordinator
        .run_ready(
            definition,
            vec![StepExecution {
                step: approved,
                adapter: &FakeAdapter(script),
                supervisor,
            }],
        )
        .await
        .unwrap()
        .completions
        .remove(0)
}

async fn pipeline(
    f: &Fixture,
    runtime: Arc<Runtime>,
    coordinator: &Coordinator,
    supervisor: &dyn SandboxSupervisor,
) {
    coordinator.register(&f.definition).unwrap();
    assert!(matches!(
        coordinator.inputs(&f.definition, "execute"),
        Err(CoordinatorError::DependenciesPending)
    ));
    let planner = f.review(coordinator, "plan").await;
    let outcome = run(coordinator, &f.definition, &planner, PLAN, supervisor).await;
    assert_eq!(outcome.run.state, RunState::Verifying);
    assert_eq!(outcome.objective, VerificationOutcome::Unknown);
    assert!(matches!(
        coordinator.inputs(&f.definition, "execute"),
        Err(CoordinatorError::DependenciesPending)
    ));
    let output = runtime
        .declared_artifacts(&planner.summary().run_id)
        .unwrap()
        .remove(0);
    assert_eq!(
        runtime.read_declared_artifact(&output.artifact_id).unwrap(),
        b"Proposal: change src/base.txt; validate changed content."
    );
    assert!(!runtime.storage_root().join("outputs").exists());
    coordinator
        .verify_step(&f.definition, &planner, record(&runtime, &planner))
        .unwrap();
    let executor = f.review(coordinator, "execute").await;
    assert!(executor.final_request().prompt.contains("untrusted_data"));
    assert!(
        executor
            .final_request()
            .prompt
            .contains("Proposal: change src/base.txt")
    );
    run(coordinator, &f.definition, &executor, EXECUTE, supervisor).await;
    let verifier = f.review(coordinator, "verify").await;
    let outcome = run(coordinator, &f.definition, &verifier, VERIFY, supervisor).await;
    assert_eq!(
        runtime.summary(&executor.summary().run_id).unwrap().state,
        RunState::Verifying
    );
    assert_ne!(
        outcome.run.worktree,
        runtime
            .summary(&executor.summary().run_id)
            .unwrap()
            .worktree
    );
    assert_eq!(
        fs::read_to_string(f.root.join("src/base.txt")).unwrap(),
        "base\n"
    );
    coordinator
        .verify_step(&f.definition, &verifier, record(&runtime, &verifier))
        .unwrap();
    coordinator
        .verify_step(&f.definition, &executor, record(&runtime, &executor))
        .unwrap();
    assert!(coordinator.status(&f.definition).unwrap().all_verified);
    let preview = coordinator.integration_preview(&executor).await.unwrap();
    assert!(coordinator.apply(&executor, "wrong").await.is_err());
    coordinator
        .apply(&executor, &preview.approval_digest)
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("src/base.txt")).unwrap(),
        "changed\n"
    );
    assert!(
        !serde_json::to_string(&runtime.events().unwrap())
            .unwrap()
            .contains("Proposal:")
    );
}
#[tokio::test]
async fn plan_requires_parent_approval_and_verifier_gets_the_immutable_patch_view() {
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let coordinator = f.coordinator(runtime.clone());
    pipeline(&f, runtime, &coordinator, &FakeSupervisor).await;
}

#[tokio::test]
async fn durable_reopen_preserves_verified_steps_and_rejects_changed_material() {
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let coordinator = f.coordinator(runtime.clone());
    coordinator.register(&f.definition).unwrap();
    let planner = f.review(&coordinator, "plan").await;
    run(&coordinator, &f.definition, &planner, PLAN, &FakeSupervisor).await;
    let final_request = planner.final_request().clone();
    let artifacts = planner.artifact_contract().clone();
    let id = planner.summary().run_id.clone();
    let approval = planner.approval_digest().to_owned();
    request_verification(
        &f.temp.path().join("coordinator"),
        &f.definition.execution.instance_id,
        "plan",
        0,
        &approval,
        record(&runtime, &planner),
    )
    .unwrap();
    drop(planner);
    drop(coordinator);
    drop(runtime);
    let runtime = f.runtime();
    let coordinator = f.coordinator(runtime.clone());
    coordinator.register(&f.definition).unwrap();
    assert_eq!(coordinator.service_controls(&f.definition).unwrap(), 0);
    let (context, _) = f.material("plan");
    let proof = f
        .router
        .validate_receipt(&context, &f.router.route(&context).unwrap())
        .unwrap();
    let mut changed = final_request.clone();
    changed.prompt.push_str("modified");
    assert!(
        coordinator
            .reopen_step(
                &f.definition,
                "plan",
                0,
                &f.router,
                &context,
                &proof,
                changed,
                artifacts.clone()
            )
            .is_err()
    );
    let reopened = coordinator
        .reopen_step(
            &f.definition,
            "plan",
            0,
            &f.router,
            &context,
            &proof,
            final_request,
            artifacts,
        )
        .unwrap();
    assert_eq!(coordinator.service_controls(&f.definition).unwrap(), 1);
    assert_eq!(runtime.summary(&id).unwrap().state, RunState::Verified);
    assert!(
        coordinator
            .approve_step(&f.definition, reopened, &approval)
            .is_err()
    );
    assert_eq!(runtime.summary(&id).unwrap().attempts, 1);
}

#[tokio::test]
async fn live_owner_accepts_durable_approval_and_cancellation_without_reopening_runtime() {
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let coordinator = f.coordinator(runtime.clone());
    coordinator.register(&f.definition).unwrap();
    let pending = f.review(&coordinator, "plan").await;
    assert!(Coordinator::open(&f.temp.path().join("coordinator"), runtime.clone()).is_err());
    assert!(
        request_approval(
            &f.temp.path().join("coordinator"),
            &f.definition.execution.instance_id,
            "plan",
            0,
            "wrong"
        )
        .is_err()
    );
    request_approval(
        &f.temp.path().join("coordinator"),
        &f.definition.execution.instance_id,
        "plan",
        0,
        pending.approval_digest(),
    )
    .unwrap();
    assert_eq!(coordinator.service_controls(&f.definition).unwrap(), 1);
    assert_eq!(
        runtime.summary(&pending.summary().run_id).unwrap().state,
        RunState::Queued
    );
    request_cancel(
        &f.temp.path().join("coordinator"),
        &f.definition.execution.instance_id,
    )
    .unwrap();
    assert_eq!(
        runtime.summary(&pending.summary().run_id).unwrap().state,
        RunState::Cancelled
    );
    assert!(
        observe(&f.temp.path().join("coordinator"), &f.definition)
            .unwrap()
            .cancelled
    );
}

fn parallel_definition(f: &mut Fixture) {
    use sha2::Digest;
    let mut plan = f.definition.plan.clone();
    let base = plan.nodes[0].clone();
    plan.nodes = ["first", "second"]
        .into_iter()
        .map(|id| {
            let mut node = base.clone();
            node.id = id.into();
            node.owner = format!("owner-{id}");
            node.outputs = vec![format!("artifact:{id}")];
            node
        })
        .collect();
    plan.preset = WorkflowPreset::DirectSafe;
    plan.max_width = 2;
    plan.plan_id.clear();
    plan.plan_id = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&plan).unwrap()));
    let mut execution = f.definition.execution.clone();
    execution.plan_id = plan.plan_id.clone();
    execution.nodes = plan
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                NodeExecutionContract {
                    phase: StageKind::Plan,
                    dependencies: BTreeMap::new(),
                },
            )
        })
        .collect();
    execution = router_workflow::seal_execution_contract(execution, &plan).unwrap();
    let outputs = f.definition.artifacts["plan"].outputs[0].clone();
    let artifacts = plan
        .nodes
        .iter()
        .map(|node| {
            let mut output = outputs.clone();
            output.label = node.outputs[0].clone();
            (
                node.id.clone(),
                NodeArtifactDeclaration {
                    outputs: vec![output],
                    inputs: vec![],
                },
            )
        })
        .collect();
    f.definition = seal_definition(WorkflowDefinition {
        plan,
        execution,
        artifacts,
        definition_sha256: String::new(),
        ..f.definition.clone()
    })
    .unwrap();
}
#[tokio::test]
async fn partial_wave_keeps_successful_completion_and_typed_error() {
    let mut f = Fixture::new().await;
    parallel_definition(&mut f);
    let runtime = f.runtime();
    let c = f.coordinator(runtime.clone());
    c.register(&f.definition).unwrap();
    let first = f.review(&c, "first").await;
    let second = f.review(&c, "second").await;
    let wave = c
        .run_ready(
            &f.definition,
            vec![
                StepExecution {
                    step: c
                        .approve_step(&f.definition, first.clone(), first.approval_digest())
                        .unwrap(),
                    adapter: &FakeAdapter(PLAN),
                    supervisor: &router_runtime::DenySandbox,
                },
                StepExecution {
                    step: c
                        .approve_step(&f.definition, second.clone(), second.approval_digest())
                        .unwrap(),
                    adapter: &FakeAdapter(PLAN),
                    supervisor: &FakeSupervisor,
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(wave.errors.len(), 1);
    assert_eq!(wave.errors[0].run_id, first.summary().run_id);
    assert_eq!(
        wave.errors[0].code,
        StepExecutionErrorCode::PreparationDenied
    );
    assert_eq!(wave.completions.len(), 1);
    assert_eq!(wave.completions[0].run.run_id, second.summary().run_id);
    assert!(!wave.cancelled);
}
#[tokio::test]
async fn two_nodes_overlap_and_verification_controls_work_during_another_active_run() {
    let mut f = Fixture::new().await;
    parallel_definition(&mut f);
    let runtime = f.runtime();
    let c = f.coordinator(runtime.clone());
    c.register(&f.definition).unwrap();
    let first = f.review(&c, "first").await;
    let second = f.review(&c, "second").await;
    let jobs = vec![
        StepExecution {
            step: c
                .approve_step(&f.definition, first.clone(), first.approval_digest())
                .unwrap(),
            adapter: &FakeAdapter(PLAN),
            supervisor: &FakeSupervisor,
        },
        StepExecution {
            step: c
                .approve_step(&f.definition, second.clone(), second.approval_digest())
                .unwrap(),
            adapter: &FakeAdapter(
                "/bin/sleep 0.7; printf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Second result\"}}' '{\"type\":\"turn.completed\"}'",
            ),
            supervisor: &FakeSupervisor,
        },
    ];
    let (wave, ()) = tokio::join!(c.run_ready(&f.definition, jobs), async {
        for _ in 0..200 {
            if runtime.summary(&first.summary().run_id).unwrap().state == RunState::Verifying
                && runtime.summary(&second.summary().run_id).unwrap().state == RunState::Running
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            runtime.summary(&second.summary().run_id).unwrap().state,
            RunState::Running
        );
        request_verification(
            &f.temp.path().join("coordinator"),
            &f.definition.execution.instance_id,
            "first",
            0,
            first.approval_digest(),
            record(&runtime, &first),
        )
        .unwrap();
        for _ in 0..200 {
            if runtime.summary(&first.summary().run_id).unwrap().state == RunState::Verified {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            runtime.summary(&first.summary().run_id).unwrap().state,
            RunState::Verified
        );
        assert_eq!(
            runtime.summary(&second.summary().run_id).unwrap().state,
            RunState::Running
        );
    });
    let wave = wave.unwrap();
    assert_eq!(wave.completions.len(), 2);
    assert!(wave.errors.is_empty());
}

#[tokio::test]
async fn artifact_grants_reject_cross_instance_tampering_and_purge_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let c = f.coordinator(runtime.clone());
    c.register(&f.definition).unwrap();
    let planner = f.review(&c, "plan").await;
    run(&c, &f.definition, &planner, PLAN, &FakeSupervisor).await;
    c.verify_step(&f.definition, &planner, record(&runtime, &planner))
        .unwrap();
    let input = c.inputs(&f.definition, "execute").unwrap();
    let id = &input.inputs[0].artifact_id;
    let path = runtime
        .storage_root()
        .join("results")
        .join(format!("{id}.blob"));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let (context, mut request) = f.material("execute");
    request
        .prompt
        .push_str(&runtime.render_dependency_context(&input).unwrap());
    let proof = f
        .router
        .validate_receipt(&context, &f.router.route(&context).unwrap())
        .unwrap();
    let mut wrong = input.clone();
    wrong.inputs[0].workflow.instance_id = "different".into();
    assert!(
        runtime
            .review_with_artifacts(
                "wrong-grant",
                &f.router,
                &context,
                &proof,
                request.clone(),
                f.definition.plan.clone(),
                "execute",
                Some((&f.definition.execution, 0)),
                Some(&wrong)
            )
            .await
            .is_err()
    );
    fs::write(&path, b"tampered").unwrap();
    assert!(runtime.render_dependency_context(&input).is_err());
    fs::remove_file(&path).unwrap();
    let outside = f.temp.path().join("unowned");
    fs::write(&outside, "keep").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(runtime.purge_declared_artifact(id).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
#[ignore = "requires real Seatbelt or bubblewrap and built native resource helper"]
async fn native_sandbox_plan_execute_verify_handoff() {
    use router_runtime::{ResourceLimits, ResourceSupervisor};
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let c = f.coordinator(runtime.clone());
    let root = fs::canonicalize(f.temp.path()).unwrap();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("router-sandbox-helper");
    let program = fs::canonicalize("/bin/sh").unwrap();
    #[cfg(target_os = "macos")]
    let wrapper = Path::new("/usr/bin/sandbox-exec");
    #[cfg(target_os = "linux")]
    let wrapper = Path::new("/usr/bin/bwrap");
    let hashes: BTreeMap<PathBuf, String> = [helper.as_path(), program.as_path(), wrapper]
        .into_iter()
        .map(|path| {
            (
                path.into(),
                router_adapter_api::content_digest(&fs::read(path).unwrap()),
            )
        })
        .collect();
    #[cfg(target_os = "macos")]
    let sandbox = router_runtime::SeatbeltSandbox::probe_expected(
        &root,
        BTreeMap::from([("/bin/sh".into(), program)]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .unwrap();
    #[cfg(target_os = "linux")]
    let sandbox = router_runtime::BubblewrapSandbox::probe_expected(
        &root,
        wrapper,
        BTreeMap::from([("/bin/sh".into(), program)]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .unwrap();
    let supervisor = ResourceSupervisor::probe_expected(
        &helper,
        ResourceLimits {
            address_space_bytes: if cfg!(target_os = "macos") {
                None
            } else {
                Some(256 * 1024 * 1024)
            },
            ..ResourceLimits::default()
        },
        sandbox,
        &hashes,
    )
    .await
    .unwrap();
    pipeline(&f, runtime, &c, &supervisor).await;
}

#[path = "../../router-policy/tests/support/asset_fixture.rs"]
mod asset_fixture;

#[tokio::test]
async fn expired_v3_asset_authority_is_rechecked_before_any_native_process() {
    use router_asset_contracts::{ValidatedAvailability, seal_availability};
    let f = Fixture::new().await;
    let runtime = f.runtime();
    let (legacy, mut request) = f.material("plan");
    let mut route = legacy.request().clone();
    route.candidates[0].estimated_tokens = 900;
    let old: HostObservation = serde_json::from_str(include_str!(
        "../../../fixtures/examples/phase-snapshot.json"
    ))
    .unwrap();
    let old_snapshot = ValidatedHostObservation::from_observation(
        old,
        SnapshotValidation {
            now_unix_seconds: 100,
            max_age_seconds: 100,
        },
    )
    .unwrap();
    let mut inputs = asset_fixture::inputs(
        &route,
        &old_snapshot,
        &f.catalog,
        router_policy::PhaseExecutionScope {
            workspace: request.workspace.clone(),
            source_state: request.source_state.clone(),
            scope: request.scope.clone(),
        },
        &BTreeMap::new(),
        512,
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut availability = inputs.availability.document().clone();
    availability.snapshot_hash = f.snapshot.hash().into();
    availability.observed_at_unix_seconds = f.snapshot.observation().observed_at_unix_seconds;
    availability.expires_at_unix_seconds = now + 2;
    for evidence in &mut availability.evidence {
        evidence.observed_at_unix_seconds = availability.observed_at_unix_seconds;
    }
    let availability = seal_availability(availability, &f.snapshot).unwrap();
    inputs.availability = ValidatedAvailability::from_json(
        &serde_json::to_vec(&availability).unwrap(),
        &f.snapshot,
        &availability.content_hash,
        now,
    )
    .unwrap();
    let context = f
        .router
        .context_with_assets(route, &f.snapshot, &f.catalog, inputs, now)
        .unwrap();
    let receipt = f.router.route(&context).unwrap();
    request.receipt_id = receipt.receipt_id.clone();
    assert!(
        f.router
            .validate_execution_assets(&context, &request, request.prompt.as_bytes(), now)
            .unwrap()
            .is_some()
    );
    let pending = runtime
        .review(
            "stale-v3",
            &f.router,
            &context,
            request,
            f.definition.plan.clone(),
            "plan",
        )
        .await
        .unwrap();
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("printf unexpected > src/unexpected.txt"),
                &FakeSupervisor,
                router_runtime::Cancellation::default()
            )
            .await,
        Err(router_runtime::RuntimeError::Route)
    ));
    assert_eq!(
        runtime.summary("stale-v3").unwrap().state,
        RunState::Blocked
    );
    assert!(!f.root.join("src/unexpected.txt").exists());
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|event| event.state == RunState::Running)
    );
}

#[tokio::test]
async fn fail_fast_reports_jobs_cancelled_before_they_start() {
    use sha2::Digest;
    let mut f = Fixture::new().await;
    parallel_definition(&mut f);
    let mut plan = f.definition.plan.clone();
    let mut children = Vec::new();
    for id in ["third", "fourth"] {
        let mut node = plan.nodes[0].clone();
        node.id = id.into();
        node.owner = format!("owner-{id}");
        node.dependencies = vec!["first".into()];
        node.inputs = vec!["artifact:first".into()];
        node.outputs = vec![format!("artifact:{id}")];
        children.push(node);
    }
    plan.nodes.extend(children);
    for node in &mut plan.nodes {
        node.budget.max_cost_microusd = 2000;
        node.budget.max_tokens = 2000;
        node.budget.max_elapsed_ms = 6000;
    }
    plan.plan_id.clear();
    plan.plan_id = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&plan).unwrap()));
    let mut execution = f.definition.execution.clone();
    execution.plan_id = plan.plan_id.clone();
    execution.nodes = plan
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                NodeExecutionContract {
                    phase: StageKind::Plan,
                    dependencies: n
                        .dependencies
                        .iter()
                        .map(|id| (id.clone(), DependencyRequirement::Verified))
                        .collect(),
                },
            )
        })
        .collect();
    execution = router_workflow::seal_execution_contract(execution, &plan).unwrap();
    let declaration = f.definition.artifacts["first"].outputs[0].clone();
    let artifacts = plan
        .nodes
        .iter()
        .map(|n| {
            let mut output = declaration.clone();
            output.label = n.outputs[0].clone();
            (
                n.id.clone(),
                NodeArtifactDeclaration {
                    outputs: vec![output],
                    inputs: n
                        .dependencies
                        .iter()
                        .map(|id| ArtifactEdge {
                            producer: id.clone(),
                            label: format!("artifact:{id}"),
                            exposure: ArtifactExposure::PromptData,
                        })
                        .collect(),
                },
            )
        })
        .collect();
    f.definition = seal_definition(WorkflowDefinition {
        plan,
        execution,
        artifacts,
        failure_policy: WorkflowFailurePolicy::FailFast,
        definition_sha256: String::new(),
        ..f.definition.clone()
    })
    .unwrap();
    let runtime = f.runtime();
    let c = f.coordinator(runtime.clone());
    c.register(&f.definition).unwrap();
    let first = f.review(&c, "first").await;
    run(&c, &f.definition, &first, PLAN, &FakeSupervisor).await;
    c.verify_step(&f.definition, &first, record(&runtime, &first))
        .unwrap();
    let second = f.review(&c, "second").await;
    let third = f.review(&c, "third").await;
    let fourth = f.review(&c, "fourth").await;
    let jobs = vec![
        StepExecution {
            step: c
                .approve_step(&f.definition, second.clone(), second.approval_digest())
                .unwrap(),
            adapter: &FakeAdapter(PLAN),
            supervisor: &router_runtime::DenySandbox,
        },
        StepExecution {
            step: c
                .approve_step(&f.definition, third.clone(), third.approval_digest())
                .unwrap(),
            adapter: &FakeAdapter("/bin/sleep 2"),
            supervisor: &FakeSupervisor,
        },
        StepExecution {
            step: c
                .approve_step(&f.definition, fourth.clone(), fourth.approval_digest())
                .unwrap(),
            adapter: &FakeAdapter(PLAN),
            supervisor: &FakeSupervisor,
        },
    ];
    let wave = c.run_ready(&f.definition, jobs).await.unwrap();
    assert!(wave.cancelled);
    assert_eq!(wave.completions.len() + wave.errors.len(), 3);
    assert!(
        wave.errors
            .iter()
            .any(|error| error.run_id == fourth.summary().run_id
                && error.code == StepExecutionErrorCode::Cancelled)
    );
    assert_eq!(
        runtime.summary(&fourth.summary().run_id).unwrap().attempts,
        0
    );
}

#[path = "../../router-execution-decision/tests/support/synthetic.rs"]
mod semantic_fixture;

#[tokio::test]
async fn semantic_step_binds_secondary_and_reopens_without_rerouting_baseline() {
    let f = Fixture::new().await;
    let (baseline_context, mut request) = f.material("plan");
    let mut route = baseline_context.request().clone();
    let mut secondary = route.candidates[0].clone();
    secondary.id = "secondary".into();
    secondary.model = ModelId::parse("fixture-second").unwrap();
    route.candidates.push(secondary);
    let context = f.router.context(route, &f.snapshot, &f.catalog).unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let promotion = semantic_fixture::Fixture::new_at(now - 124);
    let authority =
        semantic_fixture::TestAuthority::new(promotion.active(), promotion.keys.authorities());
    let decision =
        semantic_fixture::decision(&promotion, &f.router, &context, &authority, now, now + 60);
    request.receipt_id = decision.authorization_id().into();
    request.candidate_id = decision.selection().candidate.id.clone();
    request.model = decision.selection().candidate.model.clone();
    request.effort = decision.selection().candidate.effort;
    let runtime = f.runtime();
    let coordinator = f.coordinator(runtime.clone());
    coordinator.register(&f.definition).unwrap();
    let pending = coordinator
        .review_step_decision(
            &f.definition,
            "plan",
            0,
            &f.router,
            &context,
            &decision,
            request,
        )
        .await
        .unwrap();
    assert_eq!(pending.decision().baseline(), decision.baseline());
    let approved = coordinator
        .approve_step(&f.definition, pending.clone(), pending.approval_digest())
        .unwrap();
    let completion = coordinator
        .run_ready_decisions(
            &f.definition,
            vec![DecisionStepExecution {
                execution: StepExecution {
                    step: approved,
                    adapter: &FakeAdapter(PLAN),
                    supervisor: &FakeSupervisor,
                },
                authority: Some(&authority),
            }],
        )
        .await
        .unwrap();
    assert!(completion.errors.is_empty());
    assert_eq!(completion.completions.len(), 1);
    assert_eq!(completion.completions[0].run.state, RunState::Verifying);
    let reopened = coordinator
        .reopen_step_decision(
            &f.definition,
            "plan",
            0,
            &f.router,
            &context,
            &decision,
            pending.final_request().clone(),
            pending.artifact_contract().clone(),
        )
        .unwrap();
    assert_eq!(reopened.approval_digest(), pending.approval_digest());
    assert_eq!(reopened.final_request().candidate_id, "secondary");
    assert_eq!(
        reopened
            .decision()
            .baseline()
            .selected
            .as_ref()
            .unwrap()
            .candidate
            .id,
        "primary"
    );
    assert!(
        !runtime
            .declared_artifacts(&pending.summary().run_id)
            .unwrap()
            .is_empty()
    );
}
