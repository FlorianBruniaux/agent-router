// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
// Shared authored public fixture; never a native sandbox qualification.
use super::*;

pub(super) struct Fixture {
    pub(super) temp: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) router: PhaseRouter,
    pub(super) context: ValidatedPhaseContext,
    pub(super) request: ExecutionRequest,
    pub(super) plan: WorkflowPlan,
}
impl Fixture {
    pub(super) async fn new(write: bool) -> Self {
        Self::new_at(write, tempfile::tempdir().unwrap()).await
    }
    pub(super) async fn new_at(write: bool, temp: tempfile::TempDir) -> Self {
        Self::new_at_with_task(write, temp, false).await
    }
    pub(super) async fn new_at_with_task(
        write: bool,
        temp: tempfile::TempDir,
        useful: bool,
    ) -> Self {
        Self::new_at_with_permissions(write, temp, useful, true).await
    }
    pub(super) async fn new_at_with_permissions(
        write: bool,
        temp: tempfile::TempDir,
        useful: bool,
        integrate: bool,
    ) -> Self {
        Self::new_at_with_network(write, temp, useful, integrate, false).await
    }
    pub(super) async fn new_at_with_network(
        write: bool,
        temp: tempfile::TempDir,
        useful: bool,
        integrate: bool,
        network: bool,
    ) -> Self {
        let root = fs::canonicalize(temp.path()).unwrap().join("repo");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/base.txt"), "base\n").unwrap();
        if useful {
            fs::write(
                root.join("src/clamp.py"),
                "def clamp(value):\n    return min(value, 0)\n",
            )
            .unwrap();
        }
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
                    // Synthetic repositories must not inherit user hooks or
                    // asynchronous Git note writers during object-count checks.
                    .args(["-c", "core.hooksPath=/dev/null"])
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .arg("-C")
                    .arg(&root)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut request: PhaseRouteRequest =
            serde_json::from_str(include_str!("../../fixtures/examples/phase-request.json"))
                .unwrap();
        if useful {
            request.task.objective = "Fix the nonnegative clamp function".into();
            request.task.kind = TaskKind::Implement;
            request.task.scope = vec!["src/clamp.py".into()];
            request.task.deliverables = vec!["corrected clamp function".into()];
            request.task.acceptance = vec!["clamp(-5)=0, clamp(0)=0, clamp(5)=5".into()];
        }
        if !integrate {
            request.task.allowed_mutations.remove(&Mutation::Integrate);
            request
                .restrictions
                .allowed_mutations
                .as_mut()
                .unwrap()
                .remove(&Mutation::Integrate);
            request.restrictions.policy_hash.clear();
            request.restrictions.policy_hash = router_adapter_api::content_digest(
                &serde_json::to_vec(&request.restrictions).unwrap(),
            );
        }
        if network {
            request
                .task
                .required_capabilities
                .insert(Capability::Network);
        }
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
        let mut observation: HostObservation =
            serde_json::from_str(include_str!("../../fixtures/examples/phase-snapshot.json"))
                .unwrap();
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
        let catalog =
            Catalog::from_json(include_bytes!("../../fixtures/examples/phase-catalog.json"))
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
            scope: if useful {
                vec!["src/clamp.py".into()]
            } else {
                vec!["src".into()]
            },
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
            prompt: if useful {
                "Fix src/clamp.py: clamp(-5)=0, clamp(0)=0, clamp(5)=5".into()
            } else {
                "private fixture prompt never stored".into()
            },
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
    pub(super) fn runtime(&self) -> Runtime {
        Runtime::open(&self.temp.path().join("runtime"), RuntimeLimits::default())
            .map(|runtime| {
                runtime.with_capability_proof_verifier(std::sync::Arc::new(
                    fixture_proofs::FixtureVerifier,
                ))
            })
            .unwrap()
    }
    pub(super) async fn review(&self, runtime: &Runtime) -> PendingRun {
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
pub(super) struct FakeAdapter(pub(super) &'static str);
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
pub(super) struct FakeSupervisor;
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
