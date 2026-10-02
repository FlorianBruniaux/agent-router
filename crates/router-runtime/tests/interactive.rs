#[path = "../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)] // Shared fixtures can coexist in one integration test.
mod fixture_proofs;
use router_adapter_api::{
    AdapterError, HostObservation, InteractiveAdapter, SnapshotValidation,
    ValidatedExecutionRequest, prepared_run,
};
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_runtime::*;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use std::{
    fs,
    path::PathBuf,
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
        Self::new_at(write, tempfile::tempdir().unwrap(), "synthetic-host").await
    }
    async fn new_with_host(write: bool, host: &'static str) -> Self {
        Self::new_at(write, tempfile::tempdir().unwrap(), host).await
    }
    async fn new_at(write: bool, temp: tempfile::TempDir, host: &'static str) -> Self {
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
        observation.host.harness = HarnessId::parse(host).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        observation.observed_at_unix_seconds = now;
        for evidence in &mut observation.evidence {
            evidence.observed_at_unix_seconds = now;
        }
        let snapshot = fixture_proofs::from_observation(
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
        Runtime::open(&self.temp.path().join("runtime"), RuntimeLimits::default())
            .map(|runtime| {
                runtime.with_capability_proof_verifier(std::sync::Arc::new(
                    fixture_proofs::FixtureVerifier,
                ))
            })
            .unwrap()
    }
    async fn review(&self, runtime: &Runtime) -> PendingInteractiveRun {
        runtime
            .review_interactive(
                "interactive-1",
                &self.router,
                &self.context,
                InteractiveExecutionRequest {
                    execution: self.request.clone(),
                    protocol: InteractiveProtocol::default(),
                },
                self.plan.clone(),
                "execute",
            )
            .await
            .unwrap()
    }
}

// Provider-free application fixture. Scripts are trusted test code, never callback arguments.
struct Controller(&'static str);
impl InteractiveAdapter for Controller {
    fn host(&self) -> &'static str {
        "synthetic-host"
    }
    fn prepare_interactive(
        &self,
        request: &ValidatedExecutionRequest,
        protocol: &InteractiveProtocol,
    ) -> std::result::Result<PreparedInteractiveRun, AdapterError> {
        Ok(PreparedInteractiveRun {
            launch: prepared_run(request, "/bin/sh", vec!["-c".into(), self.0.into()], vec![])?,
            protocol: protocol.clone(),
        })
    }
}
// Trusted test-only substitute for OS policy. Production still requires a probed native sandbox.
struct FixtureSandbox;
impl SandboxSupervisor for FixtureSandbox {
    fn command(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy,
    ) -> Result<tokio::process::Command> {
        assert!(!policy.network);
        let mut command = tokio::process::Command::new(&run.program);
        command
            .args(&run.argv)
            .current_dir(&run.cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        Ok(command)
    }
}
async fn supervisor() -> ResourceSupervisor<FixtureSandbox> {
    supervisor_with(FixtureSandbox).await
}
async fn supervisor_with<S: SandboxSupervisor>(sandbox: S) -> ResourceSupervisor<S> {
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("router-sandbox-helper");
    assert!(helper.is_file(), "build router-sandbox-helper first");
    ResourceSupervisor::probe(
        &helper,
        ResourceLimits {
            address_space_bytes: None,
            max_processes: 4096,
            ..ResourceLimits::default()
        },
        sandbox,
    )
    .await
    .unwrap()
}
const ROUND_TRIP: &str = r#"
test -z "${ROUTER_RUNTIME_LEASE-}" || exit 10
test -z "${ROUTER_RUNTIME_SUPERVISION-}" || exit 11
IFS= read -r start || exit 12
printf '%s\n' '{"type":"ping","turn_id":"turn-1","call_id":"call-1","payload":"hello"}'
IFS= read -r reply || exit 13
test "$reply" = '{"type":"pong","turn_id":"turn-1","call_id":"call-1","payload":"hello"}' || exit 14
printf '%s\n' '{"type":"completed","turn_id":"turn-1"}'
"#;

struct NativeCodexController;
impl InteractiveAdapter for NativeCodexController {
    fn host(&self) -> &'static str {
        "codex"
    }
    fn prepare_interactive(
        &self,
        request: &ValidatedExecutionRequest,
        protocol: &InteractiveProtocol,
    ) -> std::result::Result<PreparedInteractiveRun, AdapterError> {
        Ok(PreparedInteractiveRun {
            launch: prepared_run(request, "codex", vec!["app-server".into()], vec![])?,
            protocol: protocol.clone(),
        })
    }
}

struct AliasedProviderController {
    host: &'static str,
}
impl InteractiveAdapter for AliasedProviderController {
    fn host(&self) -> &'static str {
        self.host
    }
    fn prepare_interactive(
        &self,
        request: &ValidatedExecutionRequest,
        protocol: &InteractiveProtocol,
    ) -> std::result::Result<PreparedInteractiveRun, AdapterError> {
        Ok(PreparedInteractiveRun {
            launch: prepared_run(request, "provider-cli-alias", vec![], vec![])?,
            protocol: protocol.clone(),
        })
    }
}

#[derive(Default)]
struct HostStateRecordingSandbox {
    host_state_calls: std::sync::atomic::AtomicUsize,
    launch_calls: std::sync::atomic::AtomicUsize,
}
impl SandboxSupervisor for HostStateRecordingSandbox {
    fn resource_limits_enforced(&self) -> bool {
        // This supervisor always refuses preparation and never spawns a process.
        true
    }
    fn create_host_state(&self, _: &std::path::Path, _: &PreparedRun) -> Result<Option<HostState>> {
        self.host_state_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // Record the authority boundary without creating files or reading credentials.
        Err(RuntimeError::Sandbox)
    }
    fn command(&self, _: &PreparedRun, _: &SandboxPolicy) -> Result<tokio::process::Command> {
        self.launch_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(RuntimeError::Sandbox)
    }
    fn interactive_command(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy,
    ) -> Result<tokio::process::Command> {
        self.command(run, policy)
    }
    fn parent_lifetime_enforced(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn interactive_codex_is_refused_before_host_state_or_launch() {
    let fixture = Fixture::new_with_host(false, "codex").await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let sandbox = HostStateRecordingSandbox::default();
    let result = runtime
        .execute_interactive(
            approved,
            &NativeCodexController,
            &sandbox,
            Cancellation::default(),
        )
        .await;
    assert!(result.is_err(), "native host must be refused");
    assert_eq!(
        sandbox
            .host_state_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0,
        "refuse before a supervisor can materialize host credentials"
    );
    assert_eq!(
        sandbox
            .launch_calls
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let summary = runtime.summary("interactive-1").unwrap();
    assert_eq!(summary.attempts, 0);
    assert_eq!(summary.state, RunState::Blocked);
    assert!(
        fs::read_dir(fixture.temp.path().join("runtime"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("host-state-"))
    );
}

#[tokio::test]
async fn interactive_provider_host_aliases_are_refused_before_host_state_or_launch() {
    for host in ["codex", "claude-code"] {
        let fixture = Fixture::new_with_host(false, host).await;
        let runtime = fixture.runtime();
        let pending = fixture.review(&runtime).await;
        let approved = runtime
            .approve_interactive(pending.clone(), pending.approval_digest())
            .unwrap();
        let sandbox = HostStateRecordingSandbox::default();
        let result = runtime
            .execute_interactive(
                approved,
                &AliasedProviderController { host },
                &sandbox,
                Cancellation::default(),
            )
            .await;
        assert!(result.is_err(), "provider host {host} must be refused");
        assert_eq!(
            sandbox
                .host_state_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "refuse {host} aliases before host state"
        );
        assert_eq!(
            sandbox
                .launch_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "refuse {host} aliases before launch"
        );
        let summary = runtime.summary("interactive-1").unwrap();
        assert_eq!(summary.attempts, 0);
        assert_eq!(summary.state, RunState::Blocked);
    }
}

struct EnvironmentFixtureSandbox;
impl SandboxSupervisor for EnvironmentFixtureSandbox {
    fn command(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy,
    ) -> Result<tokio::process::Command> {
        let mut command = FixtureSandbox.command(run, policy)?;
        for name in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "HOME",
        ] {
            command.env(name, "synthetic-test-only");
        }
        Ok(command)
    }
}

#[tokio::test]
async fn interactive_controller_observes_no_provider_or_personal_environment() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let script = r#"
printf '%s\n' "${OPENAI_API_KEY+present}" "${ANTHROPIC_API_KEY+present}" "${CODEX_HOME+present}" "${CLAUDE_CONFIG_DIR+present}" "${HOME+present}" > environment-observed.txt
IFS= read -r start || exit 12
printf '%s\n' '{"type":"completed","turn_id":"turn-1"}'
"#;
    let completion = runtime
        .execute_interactive(
            approved,
            &Controller(script),
            &supervisor_with(EnvironmentFixtureSandbox).await,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(completion.transport.exit_code, Some(0));
    assert_eq!(
        fs::read_to_string(fixture.root.join("environment-observed.txt")).unwrap(),
        "\n\n\n\n\n",
        "controller must observe all five sensitive environment variables as absent"
    );
}

#[tokio::test]
async fn approved_interactive_round_trip_has_verified_cleanup_and_no_objective_claim() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    assert!(matches!(
        runtime.approve_interactive(pending.clone(), "wrong"),
        Err(RuntimeError::Approval)
    ));
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let completion = runtime
        .execute_interactive(
            approved,
            &Controller(ROUND_TRIP),
            &supervisor().await,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(completion.transport.exit_code, Some(0));
    assert_eq!(completion.run.state, RunState::Verifying);
    assert_eq!(completion.validation, VerificationOutcome::Unknown);
    assert_eq!(completion.objective, VerificationOutcome::Unknown);
    assert_eq!(
        runtime.cleanup_evidence("interactive-1").unwrap(),
        ProcessCleanupEvidence::GroupTerminated
    );
    runtime.verify_integrity().unwrap();
}

#[tokio::test]
async fn invalid_correlation_replay_and_incomplete_turn_fail_after_cleanup() {
    let supervisor = supervisor().await;
    for script in [
        r#"read start; printf '%s\n' '{"type":"ping","turn_id":"other","call_id":"call-1","payload":"hello"}'; sleep 3"#,
        r#"read start; printf '%s\n' '{"type":"ping","turn_id":"turn-1","call_id":"call-1","payload":"hello"}'; read reply; printf '%s\n' '{"type":"ping","turn_id":"turn-1","call_id":"call-1","payload":"again"}'; sleep 3"#,
        r#"read start; printf '%s\n' '{"type":"completed","turn_id":"other"}'"#,
        r#"read start; printf '%s\n' '{"type":"ping","turn_id":"turn-1","call_id":"call-1","payload":"hello","extra":true}'"#,
        "read start; exit 0",
    ] {
        let fixture = Fixture::new(false).await;
        let runtime = fixture.runtime();
        let pending = fixture.review(&runtime).await;
        let approved = runtime
            .approve_interactive(pending.clone(), pending.approval_digest())
            .unwrap();
        let result = runtime
            .execute_interactive(
                approved,
                &Controller(script),
                &supervisor,
                Cancellation::default(),
            )
            .await;
        assert!(result.is_err(), "invalid application transcript must fail");
        assert_eq!(
            runtime.summary("interactive-1").unwrap().state,
            RunState::Failed
        );
        assert_eq!(
            runtime.cleanup_evidence("interactive-1").unwrap(),
            ProcessCleanupEvidence::GroupTerminated
        );
    }
}

#[tokio::test]
async fn interactive_timeout_and_cancellation_wait_for_cleanup() {
    for cancelled in [false, true] {
        let fixture = Fixture::new(false).await;
        let runtime = fixture.runtime();
        let pending = fixture.review(&runtime).await;
        let approved = runtime
            .approve_interactive(pending.clone(), pending.approval_digest())
            .unwrap();
        let cancel = Cancellation::default();
        let trigger = cancel.clone();
        let supervisor = supervisor().await;
        let run = runtime.execute_interactive(
            approved,
            &Controller("read start; printf ready > controller-ready; sleep 5"),
            &supervisor,
            cancel,
        );
        let ready = fixture.root.join("controller-ready");
        let signal = async move {
            if cancelled {
                tokio::time::timeout(Duration::from_secs(3), async {
                    while !ready.exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                trigger.cancel();
            }
        };
        let (result, ()) = tokio::join!(run, signal);
        let completion = result.unwrap();
        assert_eq!(
            completion.transport.termination,
            if cancelled {
                TransportTermination::Cancelled
            } else {
                TransportTermination::TimedOut
            }
        );
        assert_eq!(
            runtime.cleanup_evidence("interactive-1").unwrap(),
            ProcessCleanupEvidence::GroupTerminated
        );
    }
}

#[tokio::test]
async fn interactive_definition_is_distinct_from_batch_and_bounds_are_reviewed() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    assert!(
        runtime
            .review(
                "interactive-1",
                &fixture.router,
                &fixture.context,
                fixture.request.clone(),
                fixture.plan.clone(),
                "execute"
            )
            .await
            .is_err()
    );
    let protocol = InteractiveProtocol {
        max_frame_bytes: usize::MAX,
        ..InteractiveProtocol::default()
    };
    assert!(
        runtime
            .review_interactive(
                "invalid",
                &fixture.router,
                &fixture.context,
                InteractiveExecutionRequest {
                    execution: fixture.request.clone(),
                    protocol
                },
                fixture.plan.clone(),
                "execute"
            )
            .await
            .is_err()
    );
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    assert!(matches!(
        runtime
            .execute_interactive(
                approved,
                &Controller(ROUND_TRIP),
                &DenySandbox,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::Sandbox)
    ));
}

#[tokio::test]
async fn frame_and_combined_output_bounds_stop_the_controller() {
    let supervisor = supervisor().await;
    for (script, max_output_bytes, max_frame_bytes) in [
        ("read start; while :; do printf x; done", 1024, 4096),
        ("read start; while :; do printf x >&2; done", 1024, 4096),
        ("read start; while :; do printf x; done", 16_384, 128),
    ] {
        let mut fixture = Fixture::new(false).await;
        fixture.request.timeout_ms = 3000;
        fixture.request.max_output_bytes = max_output_bytes;
        let runtime = fixture.runtime();
        let pending = runtime
            .review_interactive(
                "interactive-1",
                &fixture.router,
                &fixture.context,
                InteractiveExecutionRequest {
                    execution: fixture.request.clone(),
                    protocol: InteractiveProtocol {
                        max_frame_bytes,
                        ..InteractiveProtocol::default()
                    },
                },
                fixture.plan.clone(),
                "execute",
            )
            .await
            .unwrap();
        let approved = runtime
            .approve_interactive(pending.clone(), pending.approval_digest())
            .unwrap();
        let result = runtime
            .execute_interactive(
                approved,
                &Controller(script),
                &supervisor,
                Cancellation::default(),
            )
            .await;
        if let Ok(completion) = result {
            assert_eq!(completion.run.failure, Some(RunFailure::OutputLimit));
            assert!(completion.stdout_bytes + completion.stderr_bytes <= max_output_bytes);
        } else {
            assert_eq!(
                runtime.summary("interactive-1").unwrap().failure,
                Some(RunFailure::OutputLimit)
            );
        }
        assert_eq!(
            runtime.cleanup_evidence("interactive-1").unwrap(),
            ProcessCleanupEvidence::GroupTerminated
        );
    }
}

#[tokio::test]
async fn changed_source_blocks_interactive_spawn() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    fs::write(fixture.root.join("src/base.txt"), "changed").unwrap();
    assert!(matches!(
        runtime
            .execute_interactive(
                approved,
                &Controller(ROUND_TRIP),
                &supervisor().await,
                Cancellation::default()
            )
            .await,
        Err(RuntimeError::SourceChanged)
    ));
    assert_eq!(runtime.summary("interactive-1").unwrap().attempts, 0);
}

#[tokio::test]
async fn dropping_interactive_future_closes_lifetime_and_stops_controller() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.timeout_ms = 3000;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let supervisor = supervisor().await;
    let controller =
        Controller("read start; trap '' TERM; while :; do printf x >> heartbeat; sleep 0.01; done");
    let mut running = Box::pin(runtime.execute_interactive(
        approved,
        &controller,
        &supervisor,
        Cancellation::default(),
    ));
    let heartbeat = fixture.root.join("heartbeat");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            tokio::select! {
                _ = &mut running => panic!("controller must remain running"),
                _ = tokio::time::sleep(Duration::from_millis(5)) => if heartbeat.exists() { break; },
            }
        }
    }).await.unwrap();
    drop(running);
    // Runtime recovery observes the watchdog lease, rather than assuming that
    // dropping an async future killed the native controller.
    drop(runtime);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let stopped = fs::metadata(&heartbeat).unwrap().len();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(fs::metadata(&heartbeat).unwrap().len(), stopped);
    let runtime = fixture.runtime();
    assert_eq!(
        runtime.cleanup_evidence("interactive-1").unwrap(),
        ProcessCleanupEvidence::GroupTerminated
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "requires live macOS Seatbelt; unavailable enforcement remains UNKNOWN"]
async fn native_seatbelt_interactive_round_trip() {
    use std::collections::BTreeMap;
    let mut fixture = Fixture::new(false).await;
    fixture.request.timeout_ms = 3000;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("router-sandbox-helper");
    let shell = fs::canonicalize("/bin/sh").unwrap();
    let wrapper = PathBuf::from("/usr/bin/sandbox-exec");
    let expected = [&helper, &shell, &wrapper]
        .into_iter()
        .map(|path| {
            (
                path.clone(),
                router_adapter_api::content_digest(&fs::read(path).unwrap()),
            )
        })
        .collect();
    let sandbox = SeatbeltSandbox::probe_expected(
        &fs::canonicalize(fixture.temp.path()).unwrap(),
        BTreeMap::from([("/bin/sh".into(), shell)]),
        vec![],
        BTreeMap::new(),
        &expected,
    )
    .await
    .expect("live sandbox enforcement unavailable; do not claim native qualification");
    let supervisor = ResourceSupervisor::probe_expected(
        &helper,
        ResourceLimits {
            address_space_bytes: None,
            max_processes: 4096,
            ..ResourceLimits::default()
        },
        sandbox,
        &expected,
    )
    .await
    .unwrap();
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let completion = runtime
        .execute_interactive(
            approved,
            &Controller(ROUND_TRIP),
            &supervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(completion.run.state, RunState::Verifying);
    assert_eq!(completion.transport.exit_code, Some(0));
    assert_eq!(
        runtime.cleanup_evidence("interactive-1").unwrap(),
        ProcessCleanupEvidence::GroupTerminated
    );
}

#[tokio::test]
async fn cancelled_interactive_output_without_stdout_eof_is_retained_as_truncated() {
    let mut fixture = Fixture::new(false).await;
    fixture.request.timeout_ms = 3000;
    let runtime = Runtime::open(
        &fixture.temp.path().join("runtime"),
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 60,
            },
            ..RuntimeLimits::default()
        },
    )
    .unwrap()
    .with_capability_proof_verifier(std::sync::Arc::new(fixture_proofs::FixtureVerifier));
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let supervisor = supervisor().await;
    let cancel = Cancellation::default();
    let ready = fixture.root.join("partial-ready");
    let controller = Controller(
        "read start; printf '{\"type\":\"ping\"'; printf ready > partial-ready; sleep 5",
    );
    let running = runtime.execute_interactive(approved, &controller, &supervisor, cancel.clone());
    let signal = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(running, signal);
    let completion = result.unwrap();
    assert_eq!(
        completion.transport.termination,
        TransportTermination::Cancelled
    );
    assert!(
        completion.stdout_bytes > 0,
        "the runtime must capture actual partial stdout"
    );
    assert!(
        completion.output_artifacts.unwrap().truncated,
        "closing the application socket without EOF cannot certify complete output"
    );
    assert_eq!(
        runtime.cleanup_evidence("interactive-1").unwrap(),
        ProcessCleanupEvidence::GroupTerminated
    );
}

struct VersionedController;
impl InteractiveAdapter for VersionedController {
    fn host(&self) -> &'static str {
        "codex"
    }
    fn requires_native_version(&self) -> bool {
        true
    }
    fn supports_native_version_pin(&self) -> bool {
        true
    }
    fn validate_native_version(&self, stdout: &[u8]) -> std::result::Result<(), AdapterError> {
        if stdout == b"interactive-controller 1.0\n" {
            Ok(())
        } else {
            Err(AdapterError::Invalid("interactive.version.mismatch"))
        }
    }
    fn validate_native_version_pin(
        &self,
        stdout: &[u8],
        expected: &str,
    ) -> std::result::Result<(), AdapterError> {
        self.validate_native_version(stdout)?;
        if expected == "1.0" {
            Ok(())
        } else {
            Err(AdapterError::Invalid("interactive.version.pin"))
        }
    }
    fn prepare_interactive(
        &self,
        request: &ValidatedExecutionRequest,
        protocol: &InteractiveProtocol,
    ) -> std::result::Result<PreparedInteractiveRun, AdapterError> {
        Controller(ROUND_TRIP).prepare_interactive(request, protocol)
    }
}
#[tokio::test]
async fn divergent_native_version_is_rejected_before_interactive_controller_launch() {
    let fixture = Fixture::new(false).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve_interactive(pending.clone(), pending.approval_digest())
        .unwrap();
    let result = runtime
        .execute_interactive(
            approved,
            &VersionedController,
            &supervisor().await,
            Cancellation::default(),
        )
        .await;
    assert!(matches!(
        result,
        Err(RuntimeError::Adapter(AdapterError::Invalid(
            "interactive.version.mismatch"
        )))
    ));
    assert_eq!(runtime.summary("interactive-1").unwrap().attempts, 0);
    assert_eq!(
        runtime.summary("interactive-1").unwrap().state,
        RunState::Blocked
    );
}
