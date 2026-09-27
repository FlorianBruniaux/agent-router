use router_adapter_api::{
    AdapterError, HarnessAdapter, HostObservation, SnapshotValidation, ValidatedExecutionRequest,
    ValidatedHostObservation, prepared_run,
};
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_runtime::*;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    router: PhaseRouter,
    context: ValidatedPhaseContext,
    request: ExecutionRequest,
    plan: WorkflowPlan,
}
impl Fixture {
    async fn new(write: bool) -> Self {
        Self::new_at(write, tempfile::tempdir().unwrap()).await
    }
    async fn new_at(write: bool, temp: tempfile::TempDir) -> Self {
        let root = fs::canonicalize(temp.path()).unwrap().join("repo");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/base.txt"), "base\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "src"],
            vec![
                "-c",
                "user.name=Runtime Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "--no-gpg-sign",
                "-m",
                "fixture",
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
        request.phase = StageKind::Execute;
        request.task.budget.max_elapsed_ms = 10_000;
        if !write {
            request
                .task
                .allowed_mutations
                .remove(&Mutation::WorkspaceWrite);
        }
        request.candidates.truncate(1);
        let candidate = &mut request.candidates[0];
        candidate.sandbox = if write {
            PhaseSandbox::WorkspaceWrite
        } else {
            PhaseSandbox::ReadOnly
        };
        let plan = build_plan(
            &request.task,
            WorkflowPreset::DirectSafe,
            PlanLimits {
                reserve_elapsed_ms: 1_000,
                ..PlanLimits::default()
            },
        )
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
        let catalog = Catalog::from_json(include_bytes!(
            "../../../fixtures/examples/phase-catalog.json"
        ))
        .unwrap();
        let router = PhaseRouter::stable_v2();
        let context = router.context(request, &snapshot, &catalog).unwrap();
        let receipt = router.route(&context).unwrap();
        let selected = &receipt.selected.as_ref().unwrap().candidate;
        let request = ExecutionRequest {
            schema_version: 1,
            receipt_id: receipt.receipt_id.clone(),
            candidate_id: selected.id.clone(),
            phase: receipt.phase,
            host: snapshot.host().harness.clone(),
            model: selected.model.clone(),
            effort: selected.effort,
            workspace: root.to_str().unwrap().into(),
            source_state: source_state(&root).await.unwrap(),
            scope: vec!["src".into()],
            sandbox: if write {
                ExecutionSandbox::WorkspaceWrite
            } else {
                ExecutionSandbox::ReadOnly
            },
            required_capabilities: router
                .execution_capabilities(&context, receipt.selected.as_ref().unwrap())
                .unwrap(),
            tools: ToolSelection::Disabled,
            agent: None,
            skills: vec![],
            prompt: "private fixture prompt never stored".into(),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            allow_degradation: false,
            timeout_ms: 500,
            max_output_bytes: 16_384,
        };
        Self {
            temp,
            root,
            router,
            context,
            request,
            plan,
        }
    }
    fn runtime(&self) -> Runtime {
        Runtime::open(&self.temp.path().join("runtime"), RuntimeLimits::default()).unwrap()
    }
    async fn review(&self, runtime: &Runtime) -> PendingRun {
        runtime
            .review(
                "run-1",
                &self.router,
                &self.context,
                self.request.clone(),
                self.plan.clone(),
                "execute",
            )
            .await
            .unwrap()
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
        _: &[u8],
        _: TransportOutcome,
        _: usize,
    ) -> std::result::Result<RuntimeEvidence, AdapterError> {
        Err(AdapterError::Protocol)
    }
}
// Deliberately test-only trusted code, never a production sandbox claim.
struct FakeSupervisor;
impl SandboxSupervisor for FakeSupervisor {
    fn resource_limits_enforced(&self) -> bool {
        true
    }
    fn command(&self, run: &PreparedRun, _: &SandboxPolicy) -> Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new(&run.program);
        command
            .args(&run.argv)
            .current_dir(&run.cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        Ok(command)
    }
}

#[tokio::test]
async fn explicit_approval_and_unknown_sandbox_fail_closed() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    assert!(matches!(
        runtime.approve(pending.clone(), "wrong"),
        Err(RuntimeError::Approval)
    ));
    assert!(!fixture.temp.path().join("runtime/worktrees").exists());
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("exit 0"),
                &DenySandbox,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Sandbox)
    ));
    assert_eq!(runtime.summary("run-1").unwrap().state, RunState::Blocked);
    assert!(!fixture.temp.path().join("runtime/worktrees").exists());
}
#[tokio::test]
async fn process_success_is_not_verification_and_verified_steps_never_replay() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("printf 'verified=true'"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(output.transport.exit_code, Some(0));
    assert_eq!(output.validation, VerificationOutcome::Unknown);
    assert_eq!(output.objective, VerificationOutcome::Unknown);
    assert_eq!(output.run.state, RunState::Verifying);
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "independent-test".into(),
                evidence_sha256: "a".repeat(64),
                artifact_sha256: None,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    drop(runtime);
    let runtime = fixture.runtime();
    assert_eq!(runtime.summary("run-1").unwrap().state, RunState::Verified);
    assert!(matches!(
        runtime.approve(fixture.review(&runtime).await, pending.approval_digest()),
        Err(RuntimeError::Transition)
    ));
    runtime.verify_integrity().unwrap();
    let db = fs::read(fixture.temp.path().join("runtime/runtime.sqlite3")).unwrap();
    assert!(!String::from_utf8_lossy(&db).contains("private fixture prompt"));
}
#[tokio::test]
async fn source_change_after_approval_blocks_launch() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    fs::write(fixture.root.join("src/base.txt"), "changed").unwrap();
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("exit 0"),
                &FakeSupervisor,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::SourceChanged)
    ));
    assert_eq!(runtime.summary("run-1").unwrap().state, RunState::Blocked);
}
#[tokio::test]
async fn changing_prompt_rejects_resume_definition_and_forged_receipt() {
    let mut fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    fixture.review(&runtime).await;
    fixture.request.prompt = "changed".into();
    assert!(matches!(
        runtime
            .review(
                "run-1",
                &fixture.router,
                &fixture.context,
                fixture.request.clone(),
                fixture.plan.clone(),
                "execute"
            )
            .await,
        Err(RuntimeError::Integrity("definition changed"))
    ));
    fixture.request.receipt_id = "a".repeat(64);
    assert!(matches!(
        runtime
            .review(
                "other",
                &fixture.router,
                &fixture.context,
                fixture.request.clone(),
                fixture.plan.clone(),
                "execute"
            )
            .await,
        Err(RuntimeError::Route)
    ));
}

#[tokio::test]
async fn historical_reopen_does_not_require_a_fresh_snapshot_but_execution_does() {
    let fixture = Fixture::new(false).await;
    let root = fixture.temp.path().join("runtime");
    let runtime = Runtime::open(
        &root,
        RuntimeLimits {
            snapshot_max_age_seconds: 1,
            ..RuntimeLimits::default()
        },
    )
    .unwrap();
    let pending = fixture.review(&runtime).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let historical = runtime
        .reopen(
            "run-1",
            &fixture.router,
            &fixture.context,
            fixture.request.clone(),
            fixture.plan.clone(),
            "execute",
        )
        .unwrap();
    let approved = runtime
        .approve(historical, pending.approval_digest())
        .unwrap();
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("exit 0"),
                &FakeSupervisor,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Route)
    ));
    assert_eq!(runtime.summary("run-1").unwrap().state, RunState::Blocked);
}
#[tokio::test]
async fn timeout_cancel_and_bounded_retry_are_persisted() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.timeout_ms = 80;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("sleep 30"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(out.run.state, RunState::TimedOut);
    assert!(out.transport.elapsed_ms < 2_000);
    let old_approval = pending.approval_digest().to_owned();
    let pending = runtime.retry(pending).unwrap();
    assert_ne!(pending.approval_digest(), old_approval);
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let cancelling = async {
        while runtime.summary("run-1").unwrap().state != RunState::Running {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        signal.cancel();
    };
    let (out, ()) = tokio::join!(
        runtime.execute(approved, &FakeAdapter("sleep 30"), &FakeSupervisor, cancel),
        cancelling
    );
    let out = out.unwrap();
    assert_eq!(out.run.state, RunState::Cancelled);
    assert_eq!(out.run.attempts, 2);
    assert!(runtime.retry(pending).is_err());
    runtime.verify_integrity().unwrap();
}
#[tokio::test]
async fn output_is_bounded_and_not_retained() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.max_output_bytes = 1024;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("yes private-secret"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(out.run.failure, Some(RunFailure::OutputLimit));
    assert!(out.stdout_bytes + out.stderr_bytes <= 1024);
    assert!(
        !serde_json::to_string(&runtime.events().unwrap())
            .unwrap()
            .contains("private-secret")
    );
}
#[tokio::test]
async fn writes_are_isolated_and_artifact_changes_invalidate_integration() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("printf 'new\\n' > src/new.txt"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(out.run.state, RunState::Verifying);
    assert!(!fixture.root.join("src/new.txt").exists());
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "test".into(),
                evidence_sha256: "b".repeat(64),
                artifact_sha256: out.run.artifact_sha256.clone(),
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    let preview = runtime.integration_preview(&pending).await.unwrap();
    assert_eq!(preview.changed_paths, vec!["src/new.txt"]);
    assert!(matches!(
        runtime.apply(&pending, "bad").await,
        Err(RuntimeError::Approval)
    ));
    fs::write(
        Path::new(out.run.worktree.as_ref().unwrap()).join("src/new.txt"),
        "different",
    )
    .unwrap();
    assert!(matches!(
        runtime.apply(&pending, &preview.approval_digest).await,
        Err(RuntimeError::Integrity("artifact changed"))
    ));
    runtime
        .discard(&pending, pending.approval_digest())
        .await
        .unwrap();
    assert!(!Path::new(out.run.worktree.as_ref().unwrap()).exists());
}
#[tokio::test]
async fn explicit_apply_integrates_only_verified_patch() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("printf 'changed\\n' > src/base.txt"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "test".into(),
                evidence_sha256: "b".repeat(64),
                artifact_sha256: out.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    let preview = runtime.integration_preview(&pending).await.unwrap();
    runtime
        .apply(&pending, &preview.approval_digest)
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/base.txt")).unwrap(),
        "changed\n"
    );
    let reopened = runtime
        .reopen(
            "run-1",
            &fixture.router,
            &fixture.context,
            fixture.request.clone(),
            fixture.plan.clone(),
            "execute",
        )
        .unwrap();
    runtime
        .discard(&reopened, reopened.approval_digest())
        .await
        .unwrap();
}
#[tokio::test]
async fn out_of_scope_write_is_not_an_approved_artifact() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("printf bad > outside.txt"),
                &FakeSupervisor,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Scope)
    ));
    assert_eq!(
        runtime.summary("run-1").unwrap().state,
        RunState::VerificationFailed
    );
    assert!(!fixture.root.join("outside.txt").exists());
}
#[tokio::test]
#[ignore = "Live OS enforcement canary; explicitly run on macOS and report unavailable as UNKNOWN"]
async fn native_seatbelt_write_and_network_canary() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let hashes = expected_hashes(&[
        Path::new("/usr/bin/touch"),
        Path::new("/usr/bin/sandbox-exec"),
    ]);
    SeatbeltSandbox::probe_expected(
        &root,
        BTreeMap::from([("touch".into(), PathBuf::from("/usr/bin/touch"))]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .expect("native sandbox unavailable or canary failed; do not authorize execution");
}

#[tokio::test]
#[ignore = "requires the built resource helper and live macOS Seatbelt enforcement"]
async fn native_resource_helper_and_seatbelt_execute_only_the_isolated_scope() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let root = fs::canonicalize(fixture.temp.path()).unwrap();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("router-sandbox-helper");
    let program = fs::canonicalize("/bin/sh").unwrap();
    let hashes = expected_hashes(&[&helper, &program, Path::new("/usr/bin/sandbox-exec")]);
    let sandbox = SeatbeltSandbox::probe_expected(
        &root,
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
            address_space_bytes: None,
            ..ResourceLimits::default()
        },
        sandbox,
        &hashes,
    )
    .await
    .unwrap();
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("printf safe > src/native.txt"),
            &supervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(output.run.state, RunState::Verifying);
    assert_eq!(output.memory_enforcement, MemoryEnforcement::NotRequested);
    assert!(!fixture.root.join("src/native.txt").exists());
}
#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires live Linux user namespaces, bubblewrap and netcat"]
async fn native_bubblewrap_read_write_and_network_canary() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let program = fs::canonicalize("/usr/bin/touch").unwrap();
    let hashes = expected_hashes(&[&program, Path::new("/usr/bin/bwrap")]);
    BubblewrapSandbox::probe_expected(
        &root,
        Path::new("/usr/bin/bwrap"),
        BTreeMap::from([("touch".into(), program)]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .unwrap();
}

fn expected_hashes(paths: &[&Path]) -> BTreeMap<PathBuf, String> {
    paths
        .iter()
        .map(|path| {
            (
                (*path).to_path_buf(),
                router_adapter_api::content_digest(&fs::read(path).unwrap()),
            )
        })
        .collect()
}

#[tokio::test]
async fn repository_filters_are_rejected_before_review_checkout_or_artifact() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let marker = fixture.temp.path().join("filter-executed");
    for key in [
        "filter.hostile.clean",
        "filter.hostile.smudge",
        "filter.hostile.process",
    ] {
        assert!(
            Command::new("/usr/bin/git")
                .arg("-C")
                .arg(&fixture.root)
                .args(["config", "--local", key])
                .arg(format!("touch '{}'", marker.display()))
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(matches!(
        source_state(&fixture.root).await,
        Err(RuntimeError::Invalid(
            "repository filters are not permitted"
        ))
    ));
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    assert!(matches!(
        runtime
            .execute(
                approved,
                &FakeAdapter("exit 0"),
                &FakeSupervisor,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Invalid(
            "repository filters are not permitted"
        ))
    ));
    assert!(!marker.exists());
    assert!(!fixture.temp.path().join("runtime/worktrees").exists());
}
#[test]
fn observer_control_child_process() {
    let Some(root) = std::env::var_os("ROUTER_RUNTIME_TEST_CONTROL_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let run = observe(root, "run-1").unwrap();
    let expected = match std::env::var("ROUTER_RUNTIME_TEST_CONTROL_STATE").as_deref() {
        Ok("awaiting_approval") => RunState::AwaitingApproval,
        Ok("queued") => RunState::Queued,
        _ => RunState::Running,
    };
    assert_eq!(run.state, expected);
    request_cancel(root, "run-1").unwrap();
    if expected != RunState::Running {
        assert_eq!(observe(root, "run-1").unwrap().state, RunState::Cancelled);
    }
}

#[tokio::test]
async fn second_process_cancels_unstarted_work_without_a_supervisor_loop() {
    for queued in [false, true] {
        let fixture = Fixture::new(false).await;
        let runtime = fixture.runtime();
        let pending = fixture.review(&runtime).await;
        let approved = queued.then(|| {
            runtime
                .approve(pending.clone(), pending.approval_digest())
                .unwrap()
        });
        let root = fixture.temp.path().join("runtime");
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("observer_control_child_process")
            .env("ROUTER_RUNTIME_TEST_CONTROL_ROOT", &root)
            .env(
                "ROUTER_RUNTIME_TEST_CONTROL_STATE",
                if queued {
                    "queued"
                } else {
                    "awaiting_approval"
                },
            )
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(observe(&root, "run-1").unwrap().state, RunState::Cancelled);
        request_cancel(&root, "run-1").unwrap();
        if let Some(approved) = approved {
            assert!(matches!(
                runtime
                    .execute(
                        approved,
                        &FakeAdapter("exit 99"),
                        &FakeSupervisor,
                        Cancellation::default()
                    )
                    .await,
                Err(RuntimeError::Cancelled)
            ));
        } else {
            assert!(
                runtime
                    .approve(pending.clone(), pending.approval_digest())
                    .is_err()
            );
        }
        assert_eq!(runtime.summary("run-1").unwrap().attempts, 0);
        assert!(!root.join("worktrees").exists());
        assert_eq!(
            observe_events(&root, "run-1")
                .unwrap()
                .iter()
                .filter(|event| event.kind == "control.cancelled_before_start")
                .count(),
            1
        );
        runtime.verify_integrity().unwrap();
    }
}

#[tokio::test]
async fn approved_helper_replacement_and_missing_digest_are_rejected_before_execution() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let helper = root.join("helper");
    let original =
        b"#!/bin/sh\nprintf 'router-sandbox-helper-v1:limits-applied:address-space=enforced\\n'\n";
    fs::write(&helper, original).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let expected = BTreeMap::from([(helper.clone(), router_adapter_api::content_digest(original))]);
    // A replacement can impersonate the helper probe protocol, but cannot use
    // an earlier operator approval to run even its first instruction.
    fs::write(&helper, b"#!/bin/sh\nprintf ran > \"${0}.executed\"\nprintf 'router-sandbox-helper-v1:limits-applied:address-space=enforced\\n'\n").unwrap();
    for hashes in [expected, BTreeMap::new()] {
        assert!(matches!(
            ResourceSupervisor::probe_expected(
                &helper,
                ResourceLimits::default(),
                FakeSupervisor,
                &hashes
            )
            .await,
            Err(RuntimeError::Integrity(
                "approved binary changed or missing"
            ))
        ));
        assert!(!root.join("helper.executed").exists());
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn approved_seatbelt_program_or_wrapper_mismatch_rejects_before_canaries() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let program = root.join("program");
    fs::write(&program, b"original").unwrap();
    let wrapper = PathBuf::from("/usr/bin/sandbox-exec");
    let mut expected = BTreeMap::from([
        (
            program.clone(),
            router_adapter_api::content_digest(b"original"),
        ),
        (
            wrapper.clone(),
            router_adapter_api::content_digest(&fs::read(&wrapper).unwrap()),
        ),
    ]);
    fs::write(&program, b"replaced").unwrap();
    for mismatch in ["program", "wrapper"] {
        if mismatch == "wrapper" {
            expected.insert(wrapper.clone(), "0".repeat(64));
        }
        assert!(matches!(
            SeatbeltSandbox::probe_expected(
                &root,
                BTreeMap::from([("fake".into(), program.clone())]),
                vec![],
                BTreeMap::new(),
                &expected
            )
            .await,
            Err(RuntimeError::Integrity(
                "approved binary changed or missing"
            ))
        ));
        assert!(!root.join("sandbox-probe").exists());
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn approved_bubblewrap_wrapper_mismatch_rejects_before_canaries() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let wrapper = root.join("bwrap");
    let original = b"#!/bin/sh\nexit 0\n";
    fs::write(&wrapper, original).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let program = fs::canonicalize("/bin/sh").unwrap();
    let expected = BTreeMap::from([
        (
            wrapper.clone(),
            router_adapter_api::content_digest(original),
        ),
        (
            program.clone(),
            router_adapter_api::content_digest(&fs::read(&program).unwrap()),
        ),
    ]);
    fs::write(
        &wrapper,
        b"#!/bin/sh\nprintf ran > \"${0}.executed\"\nexit 0\n",
    )
    .unwrap();
    assert!(matches!(
        BubblewrapSandbox::probe_expected(
            &root,
            &wrapper,
            BTreeMap::from([("fake".into(), program)]),
            vec![],
            BTreeMap::new(),
            &expected
        )
        .await,
        Err(RuntimeError::Integrity(
            "approved binary changed or missing"
        ))
    ));
    assert!(!root.join("bwrap.executed").exists());
    assert!(!root.join("bwrap-probe").exists());
}
#[tokio::test]
async fn second_process_observes_and_cancels_live_owner_without_pid_signalling() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.timeout_ms = 900;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let controlling = async {
        while runtime.summary("run-1").unwrap().state != RunState::Running {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("observer_control_child_process")
            .env(
                "ROUTER_RUNTIME_TEST_CONTROL_ROOT",
                fixture.temp.path().join("runtime"),
            );
        let output = command.output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    };
    let (out, ()) = tokio::join!(
        runtime.execute(
            approved,
            &FakeAdapter("sleep 30"),
            &FakeSupervisor,
            Cancellation::default()
        ),
        controlling
    );
    let out = out.unwrap();
    assert_eq!(out.run.state, RunState::Cancelled);
    assert!(
        runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind == "control.cancel_requested")
    );
    runtime.verify_integrity().unwrap();
}

fn retaining_runtime(fixture: &Fixture) -> Runtime {
    Runtime::open(
        &fixture.temp.path().join("runtime"),
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 60,
            },
            ..RuntimeLimits::default()
        },
    )
    .unwrap()
}

#[tokio::test]
async fn output_retention_disabled_has_no_payload_files_or_serialized_payload() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("printf never-persist-this; printf private-error >&2"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert!(output.output_artifacts.is_none());
    assert!(runtime.output_artifacts("run-1").unwrap().is_empty());
    assert!(!fixture.temp.path().join("runtime/outputs").exists());
    let preview = serde_json::to_string(&output).unwrap();
    assert!(!preview.contains("never-persist-this"));
    assert!(!preview.contains("private-error"));
    assert!(!preview.contains("output_retention"));
    assert!(!preview.contains("output_artifacts"));
}

#[tokio::test]
async fn opt_in_outputs_are_exact_private_and_explicitly_purged_after_reopen() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(false).await;
    let runtime = retaining_runtime(&fixture);
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("printf 'private-stdout\\000bytes'; printf 'private-stderr\\n' >&2"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    let metadata = output.output_artifacts.as_ref().unwrap();
    assert_eq!(metadata.state, OutputArtifactsState::Retained);
    assert_eq!(metadata.attempt, 1);
    assert!(!metadata.truncated);
    for (artifact, wanted) in [
        (&metadata.stdout, b"private-stdout\0bytes".as_slice()),
        (&metadata.stderr, b"private-stderr\n".as_slice()),
    ] {
        assert_eq!(fs::read(&artifact.path).unwrap(), wanted);
        assert_eq!(artifact.sha256, router_adapter_api::content_digest(wanted));
        assert_eq!(artifact.bytes, wanted.len());
        assert_eq!(
            fs::metadata(&artifact.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let outputs = fixture.temp.path().join("runtime/outputs");
    assert_eq!(
        fs::metadata(&outputs).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for json in [
        serde_json::to_string(&output).unwrap(),
        serde_json::to_string(&runtime.events().unwrap()).unwrap(),
    ] {
        assert!(!json.contains("private-stdout"));
        assert!(!json.contains("private-stderr"));
    }
    let foreign = outputs.join("unrelated");
    fs::write(&foreign, b"keep").unwrap();
    drop(runtime);
    // Observing/purging historical metadata does not enable retention for new runs.
    let reopened = fixture.runtime();
    assert_eq!(
        reopened.output_artifacts("run-1").unwrap(),
        vec![metadata.clone()]
    );
    assert_eq!(
        reopened
            .purge_expired_output_artifacts(metadata.expires_at_unix_seconds - 1, 10)
            .unwrap(),
        0
    );
    assert_eq!(
        reopened
            .purge_expired_output_artifacts(metadata.expires_at_unix_seconds, 10)
            .unwrap(),
        1
    );
    assert!(!Path::new(&metadata.stdout.path).exists());
    assert!(!Path::new(&metadata.stderr.path).exists());
    assert_eq!(fs::read(foreign).unwrap(), b"keep");
    assert_eq!(
        reopened.purge_output_artifacts("run-1", 1).unwrap().state,
        OutputArtifactsState::Purged
    );
    assert_eq!(
        reopened
            .purge_expired_output_artifacts(metadata.expires_at_unix_seconds, 10)
            .unwrap(),
        0
    );
    assert!(matches!(
        reopened.purge_expired_output_artifacts(metadata.expires_at_unix_seconds, 101),
        Err(RuntimeError::Limit)
    ));
    assert!(matches!(
        reopened.purge_output_artifacts("../foreign", 1),
        Err(RuntimeError::NotFound)
    ));
    reopened.verify_integrity().unwrap();
}

#[tokio::test]
async fn retained_overflow_is_bounded_and_marked_truncated() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.max_output_bytes = 1024;
    let runtime = retaining_runtime(&fixture);
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("while :; do printf 'sensitive-output\\n'; done"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(output.run.failure, Some(RunFailure::OutputLimit));
    let metadata = output.output_artifacts.unwrap();
    assert!(metadata.truncated);
    assert!(metadata.stdout.bytes + metadata.stderr.bytes <= 1024);
    assert_eq!(
        fs::read(&metadata.stdout.path).unwrap().len(),
        metadata.stdout.bytes
    );
    runtime.purge_output_artifacts("run-1", 1).unwrap();
}

#[tokio::test]
async fn retention_policy_is_approval_bound_and_cannot_change_on_an_approved_handle() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    drop(runtime);
    let retained = retaining_runtime(&fixture);
    assert!(matches!(
        retained.approve(pending.clone(), pending.approval_digest()),
        Err(RuntimeError::Approval)
    ));
    assert!(matches!(
        retained
            .execute(
                approved,
                &FakeAdapter("exit 0"),
                &FakeSupervisor,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Approval)
    ));
    assert!(!fixture.temp.path().join("runtime/outputs").exists());
    let other = Runtime::open(
        &fixture.temp.path().join("other"),
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 60,
            },
            ..RuntimeLimits::default()
        },
    )
    .unwrap();
    let new_pending = fixture.review(&other).await;
    assert_ne!(
        pending.summary().definition_hash,
        new_pending.summary().definition_hash
    );
    assert_ne!(pending.approval_digest(), new_pending.approval_digest());
}

#[tokio::test]
async fn output_purge_refuses_symlink_files_and_parent_directories() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new(false).await;
    let runtime = retaining_runtime(&fixture);
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let output = runtime
        .execute(
            approved,
            &FakeAdapter("printf safe; printf error >&2"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    let metadata = output.output_artifacts.unwrap();
    let stdout = Path::new(&metadata.stdout.path);
    let saved = stdout.with_extension("saved");
    fs::rename(stdout, &saved).unwrap();
    let foreign = fixture.temp.path().join("foreign");
    fs::write(&foreign, b"never-delete").unwrap();
    symlink(&foreign, stdout).unwrap();
    assert!(runtime.purge_output_artifacts("run-1", 1).is_err());
    assert_eq!(fs::read(&foreign).unwrap(), b"never-delete");
    assert!(Path::new(&metadata.stderr.path).exists());
    fs::remove_file(stdout).unwrap();
    fs::rename(&saved, stdout).unwrap();
    let outputs = fixture.temp.path().join("runtime/outputs");
    let moved = fixture.temp.path().join("original-outputs");
    fs::rename(&outputs, &moved).unwrap();
    symlink(&moved, &outputs).unwrap();
    assert!(runtime.purge_output_artifacts("run-1", 1).is_err());
    assert_eq!(
        fs::read(moved.join(stdout.file_name().unwrap())).unwrap(),
        b"safe"
    );
    fs::remove_file(&outputs).unwrap();
    fs::rename(&moved, &outputs).unwrap();
    runtime.purge_output_artifacts("run-1", 1).unwrap();
}

impl Fixture {
    fn contract(&self, instance: &str, artifact_verification: bool) -> WorkflowExecutionContract {
        router_workflow::seal_execution_contract(
            WorkflowExecutionContract {
                schema_version: 1,
                instance_id: instance.into(),
                plan_id: self.plan.plan_id.clone(),
                workspace: self.request.workspace.clone(),
                source_state: self.request.source_state.clone(),
                nodes: self
                    .plan
                    .nodes
                    .iter()
                    .map(|node| {
                        (
                            node.id.clone(),
                            NodeExecutionContract {
                                phase: match node.id.as_str() {
                                    "plan" => StageKind::Plan,
                                    "verify" => StageKind::Verify,
                                    _ => StageKind::Execute,
                                },
                                dependencies: node
                                    .dependencies
                                    .iter()
                                    .map(|id| {
                                        (
                                            id.clone(),
                                            if artifact_verification && node.id == "verify" {
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
            &self.plan,
        )
        .unwrap()
    }
    fn node_request(&self, node: &str) -> (ValidatedPhaseContext, ExecutionRequest) {
        let node = self.plan.nodes.iter().find(|n| n.id == node).unwrap();
        let mut route = self.context.request().clone();
        route.phase = match node.id.as_str() {
            "plan" => StageKind::Plan,
            "verify" => StageKind::Verify,
            _ => StageKind::Execute,
        };
        route.candidates[0].sandbox = if node.mode == NodeMode::IsolatedWrite {
            PhaseSandbox::WorkspaceWrite
        } else {
            PhaseSandbox::ReadOnly
        };
        if node.independent_verifier {
            route.previous_model = Some(self.request.model.clone());
            route.candidates[0].model = ModelId::parse("fixture-second").unwrap();
            route.candidates[0].verification = PhaseVerification::Independent;
        }
        let snapshot = ValidatedHostObservation::from_observation(
            self.context.snapshot().clone(),
            SnapshotValidation {
                now_unix_seconds: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                max_age_seconds: 300,
            },
        )
        .unwrap();
        let catalog = Catalog::from_json(include_bytes!(
            "../../../fixtures/examples/phase-catalog.json"
        ))
        .unwrap();
        let context = self.router.context(route, &snapshot, &catalog).unwrap();
        let receipt = self.router.route(&context).unwrap();
        let selected = receipt.selected.as_ref().unwrap();
        let mut request = self.request.clone();
        request.phase = receipt.phase;
        request.receipt_id = receipt.receipt_id.clone();
        request.model = selected.candidate.model.clone();
        request.sandbox = if node.mode == NodeMode::IsolatedWrite {
            ExecutionSandbox::WorkspaceWrite
        } else {
            ExecutionSandbox::ReadOnly
        };
        request.required_capabilities = self
            .router
            .execution_capabilities(&context, selected)
            .unwrap();
        (context, request)
    }
    async fn review_node(
        &self,
        runtime: &Runtime,
        run: &str,
        node: &str,
        contract: &WorkflowExecutionContract,
        generation: u32,
    ) -> PendingRun {
        let (context, request) = self.node_request(node);
        let receipt = self.router.route(&context).unwrap();
        let proof = self.router.validate_receipt(&context, &receipt).unwrap();
        runtime
            .review_selected(
                run,
                &self.router,
                &context,
                &proof,
                request,
                self.plan.clone(),
                node,
                Some((contract, generation)),
            )
            .await
            .unwrap()
    }
}
fn verify_pass(runtime: &Runtime, pending: &PendingRun) {
    runtime
        .record_verification(
            pending,
            VerificationRecord {
                verifier: "explicit-human-reviewer".into(),
                evidence_sha256: "a".repeat(64),
                artifact_sha256: runtime
                    .summary(&pending.summary().run_id)
                    .unwrap()
                    .artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
}

#[tokio::test]
async fn workflow_instances_cannot_borrow_verified_dependencies() {
    let f = Fixture::new(false).await;
    let rt = f.runtime();
    let a = f.contract("instance-a", false);
    let b = f.contract("instance-b", false);
    let producer = f.review_node(&rt, "a-execute", "execute", &a, 0).await;
    rt.execute(
        rt.approve(producer.clone(), producer.approval_digest())
            .unwrap(),
        &FakeAdapter("true"),
        &FakeSupervisor,
        Cancellation::default(),
    )
    .await
    .unwrap();
    verify_pass(&rt, &producer);
    // Identical plan, workspace and source still do not identify an execution.
    let consumer = f.review_node(&rt, "b-verify", "verify", &b, 0).await;
    let digest = consumer.approval_digest().to_owned();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(80),
            rt.execute(
                rt.approve(consumer.clone(), &digest).unwrap(),
                &FakeAdapter("true"),
                &FakeSupervisor,
                Cancellation::default()
            )
        )
        .await
        .is_err()
    );
    assert_eq!(rt.summary("b-verify").unwrap().state, RunState::Queued);
    drop(rt);
    let rt = f.runtime();
    let (context, request) = f.node_request("verify");
    let proof = f
        .router
        .validate_receipt(&context, &f.router.route(&context).unwrap())
        .unwrap();
    let consumer = rt
        .reopen_selected(
            "b-verify",
            &f.router,
            &context,
            &proof,
            request,
            f.plan.clone(),
            "verify",
            Some((&b, 0)),
        )
        .unwrap();
    assert!(rt.resume_approved(consumer.clone(), "wrong").is_err());
    let approved = rt.resume_approved(consumer, &digest).unwrap();
    let producer = f.review_node(&rt, "b-execute", "execute", &b, 0).await;
    let producer_approved = rt
        .approve(producer.clone(), producer.approval_digest())
        .unwrap();
    let (result, ()) = tokio::join!(
        rt.execute(
            approved,
            &FakeAdapter("true"),
            &FakeSupervisor,
            Cancellation::default()
        ),
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(rt.summary("b-verify").unwrap().state, RunState::Queued);
            rt.execute(
                producer_approved,
                &FakeAdapter("true"),
                &FakeSupervisor,
                Cancellation::default(),
            )
            .await
            .unwrap();
            verify_pass(&rt, &producer);
        }
    );
    assert_eq!(result.unwrap().run.state, RunState::Verifying);
    assert_eq!(rt.summary("a-execute").unwrap().state, RunState::Verified);
}

#[tokio::test]
async fn explicit_artifact_dependency_allows_verifier_before_producer_verification() {
    let mut f = Fixture::new(true).await;
    f.plan = build_plan(
        &f.plan.task,
        WorkflowPreset::PlanExecuteVerify,
        PlanLimits {
            reserve_elapsed_ms: 1_000,
            ..PlanLimits::default()
        },
    )
    .unwrap();
    let contract = f.contract("artifact-workflow", true);
    let rt = f.runtime();
    let planner = f.review_node(&rt, "planner", "plan", &contract, 0).await;
    rt.execute(
        rt.approve(planner.clone(), planner.approval_digest())
            .unwrap(),
        &FakeAdapter("true"),
        &FakeSupervisor,
        Cancellation::default(),
    )
    .await
    .unwrap();
    verify_pass(&rt, &planner);
    let producer = f
        .review_node(&rt, "producer", "execute", &contract, 0)
        .await;
    rt.execute(
        rt.approve(producer.clone(), producer.approval_digest())
            .unwrap(),
        &FakeAdapter("printf 'changed\\n' > src/base.txt"),
        &FakeSupervisor,
        Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(rt.summary("producer").unwrap().state, RunState::Verifying);
    assert!(rt.summary("producer").unwrap().artifact_sha256.is_some());
    let verifier = f.review_node(&rt, "verifier", "verify", &contract, 0).await;
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        rt.execute(
            rt.approve(verifier.clone(), verifier.approval_digest())
                .unwrap(),
            &FakeAdapter("true"),
            &FakeSupervisor,
            Cancellation::default(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(outcome.run.state, RunState::Verifying);
    assert_eq!(outcome.objective, VerificationOutcome::Unknown);
    assert_eq!(rt.summary("producer").unwrap().state, RunState::Verifying);
    // Readiness is not artifact handoff or verification evidence.
}

#[tokio::test]
async fn workflow_binding_rejects_changed_instance_phase_and_legacy_dependencies() {
    let f = Fixture::new(false).await;
    let rt = f.runtime();
    let contract = f.contract("bound", false);
    f.review_node(&rt, "bound-run", "execute", &contract, 0)
        .await;
    let (context, request) = f.node_request("verify");
    assert!(
        rt.review(
            "legacy-dependent",
            &f.router,
            &context,
            request,
            f.plan.clone(),
            "verify"
        )
        .await
        .is_err()
    );
    assert!(matches!(
        rt.summary("legacy-dependent"),
        Err(RuntimeError::NotFound)
    ));
    let mut changed = contract.clone();
    changed.nodes.get_mut("execute").unwrap().phase = StageKind::Plan;
    let changed = router_workflow::seal_execution_contract(changed, &f.plan).unwrap();
    let receipt = f.router.route(&f.context).unwrap();
    let proof = f.router.validate_receipt(&f.context, &receipt).unwrap();
    assert!(
        rt.review_selected(
            "wrong-phase",
            &f.router,
            &f.context,
            &proof,
            f.request.clone(),
            f.plan.clone(),
            "execute",
            Some((&changed, 0))
        )
        .await
        .is_err()
    );
    let mut source = contract.clone();
    source.source_state = "a".repeat(40);
    let source = router_workflow::seal_execution_contract(source, &f.plan).unwrap();
    assert!(
        rt.review_selected(
            "wrong-source",
            &f.router,
            &f.context,
            &proof,
            f.request.clone(),
            f.plan.clone(),
            "execute",
            Some((&source, 0))
        )
        .await
        .is_err()
    );
    // An independent-verifier node may not be invoked with an Execute receipt.
    let mut plan = build_plan(
        &f.plan.task,
        WorkflowPreset::PlanExecuteVerify,
        PlanLimits::default(),
    )
    .unwrap();
    plan.task = f.plan.task.clone();
    assert!(
        rt.review(
            "wrong-verifier",
            &f.router,
            &f.context,
            f.request.clone(),
            plan,
            "verify"
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn workflow_generation_retry_is_cumulative_and_requires_fresh_approval() {
    let f = Fixture::new(false).await;
    let rt = f.runtime();
    let contract = f.contract("retry-workflow", false);
    let first = f
        .review_node(&rt, "generation-zero", "execute", &contract, 0)
        .await;
    let failed = rt
        .execute(
            rt.approve(first.clone(), first.approval_digest()).unwrap(),
            &FakeAdapter("exit 7"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(failed.run.state, RunState::Failed);
    let mut route = f.context.request().clone();
    let mut secondary = route.candidates[0].clone();
    secondary.id = "secondary".into();
    secondary.model = ModelId::parse("fixture-second").unwrap();
    route.candidates.push(secondary);
    let snapshot = ValidatedHostObservation::from_observation(
        f.context.snapshot().clone(),
        SnapshotValidation {
            now_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            max_age_seconds: 300,
        },
    )
    .unwrap();
    let catalog = Catalog::from_json(include_bytes!(
        "../../../fixtures/examples/phase-catalog.json"
    ))
    .unwrap();
    let context = f.router.context(route, &snapshot, &catalog).unwrap();
    let receipt = f
        .router
        .fallback(
            &context,
            &std::collections::BTreeSet::from(["primary".into()]),
        )
        .unwrap();
    let proof = f.router.validate_receipt(&context, &receipt).unwrap();
    let mut request = f.request.clone();
    request.receipt_id = receipt.receipt_id.clone();
    request.candidate_id = "secondary".into();
    request.model = proof.selection().candidate.model.clone();
    request.required_capabilities = f
        .router
        .execution_capabilities(&context, proof.selection())
        .unwrap();
    let second = rt
        .review_selected(
            "generation-one",
            &f.router,
            &context,
            &proof,
            request.clone(),
            f.plan.clone(),
            "execute",
            Some((&contract, 1)),
        )
        .await
        .unwrap();
    assert_ne!(first.approval_digest(), second.approval_digest());
    assert!(rt.approve(second.clone(), first.approval_digest()).is_err());
    assert!(rt.retry(first).is_err());
    rt.execute(
        rt.approve(second.clone(), second.approval_digest())
            .unwrap(),
        &FakeAdapter("exit 8"),
        &FakeSupervisor,
        Cancellation::default(),
    )
    .await
    .unwrap();
    assert!(
        rt.review_selected(
            "generation-two",
            &f.router,
            &context,
            &proof,
            request,
            f.plan.clone(),
            "execute",
            Some((&contract, 2))
        )
        .await
        .is_err()
    );
    assert!(rt.retry(second).is_err());
}

#[tokio::test]
async fn raw_request_cannot_exceed_the_declared_node_budget() {
    use sha2::Digest;
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    for dimension in 0..3 {
        let mut plan = fixture.plan.clone();
        let budget = &mut plan
            .nodes
            .iter_mut()
            .find(|n| n.id == "execute")
            .unwrap()
            .budget;
        match dimension {
            0 => budget.max_cost_microusd = 99,
            1 => budget.max_tokens = 99,
            _ => budget.max_elapsed_ms = 99,
        }
        plan.plan_id.clear();
        plan.plan_id = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&plan).unwrap()));
        router_workflow::validate_plan(&plan).unwrap();
        let mut request = fixture.request.clone();
        request.timeout_ms = 50;
        let run_id = format!("over-node-budget-{dimension}");
        assert!(matches!(
            runtime
                .review(
                    &run_id,
                    &fixture.router,
                    &fixture.context,
                    request,
                    plan,
                    "execute"
                )
                .await,
            Err(RuntimeError::Route)
        ));
        assert!(matches!(
            runtime.summary(&run_id),
            Err(RuntimeError::NotFound)
        ));
    }
}

#[tokio::test]
#[ignore = "requires native OS sandbox, built helper and subprocess SIGKILL"]
async fn native_supervisor_sigkill_recovery_waits_for_cleanup_proof() {
    const CHILD: &str = "AR_SIGKILL_FIXTURE_ROOT";
    if let Some(parent) = std::env::var_os(CHILD) {
        let parent = PathBuf::from(parent);
        let mut fixture = Fixture::new_at(true, tempfile::tempdir_in(&parent).unwrap()).await;
        fixture.request.timeout_ms = 2_000;
        let runtime = fixture.runtime();
        let pending = fixture.review(&runtime).await;
        let helper = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("router-sandbox-helper");
        let root = fs::canonicalize(fixture.temp.path()).unwrap();
        #[cfg(target_os = "macos")]
        let sandbox = SeatbeltSandbox::probe(
            &root,
            BTreeMap::from([("/bin/sh".into(), fs::canonicalize("/bin/sh").unwrap())]),
            vec![],
            BTreeMap::new(),
        )
        .await
        .unwrap();
        #[cfg(target_os = "linux")]
        let sandbox = BubblewrapSandbox::probe(
            &root,
            BTreeMap::from([("/bin/sh".into(), fs::canonicalize("/bin/sh").unwrap())]),
            vec![],
            BTreeMap::new(),
        )
        .await
        .unwrap();
        let supervisor = ResourceSupervisor::probe(
            &helper,
            ResourceLimits {
                max_processes: 4_096,
                address_space_bytes: if cfg!(target_os = "macos") {
                    None
                } else {
                    Some(256 * 1024 * 1024)
                },
                ..ResourceLimits::default()
            },
            sandbox,
        )
        .await
        .unwrap();
        fs::write(
            parent.join("runtime-path"),
            runtime.storage_root().to_str().unwrap(),
        )
        .unwrap();
        let approved = runtime
            .approve(pending.clone(), pending.approval_digest())
            .unwrap();
        let result = runtime.execute(approved,&FakeAdapter("trap '' TERM; (trap '' TERM; while :; do printf x >> src/heartbeat; done) & wait"),&supervisor,Cancellation::default()).await;
        panic!("fixture parent must be killed before transport completion: {result:?}");
    }
    let temp = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "native_supervisor_sigkill_recovery_waits_for_cleanup_proof",
            "--nocapture",
        ])
        .env(CHILD, temp.path())
        .spawn()
        .unwrap();
    let mut ready = None;
    for _ in 0..1_000 {
        if let Ok(root) = fs::read_to_string(temp.path().join("runtime-path")) {
            let root = PathBuf::from(root);
            if let Ok(summary) = observe(&root, "run-1") {
                if let Some(worktree) = summary.worktree {
                    let heartbeat = PathBuf::from(worktree).join("src/heartbeat");
                    if summary.state == RunState::Running
                        && fs::metadata(&heartbeat).is_ok_and(|v| v.len() > 0)
                    {
                        ready = Some((root, heartbeat));
                        break;
                    }
                }
            }
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    child.kill().ok();
    let status = child.wait().unwrap();
    let (root, heartbeat) = ready.expect("subprocess must enter Running with a real heartbeat");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(nix::libc::SIGKILL));
    let mut proof = None;
    for _ in 0..500 {
        let runtime = Runtime::open(&root, RuntimeLimits::default()).unwrap();
        let summary = runtime.summary("run-1").unwrap();
        assert_eq!(summary.state, RunState::Failed);
        assert_eq!(summary.failure, Some(RunFailure::Interrupted));
        assert_eq!(summary.attempts, 1);
        let evidence = observe_cleanup(&root, "run-1").unwrap();
        if evidence != ProcessCleanupEvidence::Unknown {
            proof = Some(evidence);
            break;
        }
        let db = rusqlite::Connection::open(root.join("runtime.sqlite3")).unwrap();
        let locks: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM writer_locks WHERE run_id='run-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(locks, 1, "unknown cleanup retains write authority");
        drop(runtime);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let proof = proof.expect("watchdog must publish a lock-protected cleanup proof");
    #[cfg(target_os = "macos")]
    assert_eq!(proof, ProcessCleanupEvidence::GroupTerminated);
    #[cfg(target_os = "linux")]
    assert_eq!(proof, ProcessCleanupEvidence::NamespaceTerminated);
    let db = rusqlite::Connection::open(root.join("runtime.sqlite3")).unwrap();
    let locks: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM writer_locks WHERE run_id='run-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(locks, 0, "cleanup proof permits release of write authority");
    let bytes = fs::metadata(&heartbeat).unwrap().len();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fs::metadata(&heartbeat).unwrap().len(), bytes);
    assert!(
        observe_events(&root, "run-1")
            .unwrap()
            .iter()
            .any(|event| event.kind == "recovery.interrupted")
    );
}
