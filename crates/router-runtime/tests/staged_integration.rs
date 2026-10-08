//! Synthetic shell-config fixture, not a claimed Agent Router product defect.
//! The acceptance checker is outside the approved mutation scope and unchanged.
use sha2::{Digest, Sha256};
#[path = "../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)] // Shared fixtures can coexist in one integration test.
mod fixture_proofs;
use router_adapter_api::{
    AdapterError, HarnessAdapter, HostObservation, SnapshotValidation, ValidatedExecutionRequest,
    prepared_run,
};
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_runtime::*;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
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
        fs::write(root.join("src/config.sh"), BASE_CONFIG).unwrap();
        fs::write(root.join("src/errors.sh"), BASE_ERRORS).unwrap();
        fs::write(root.join("acceptance.sh"), ACCEPTANCE).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
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

const BASE_CONFIG: &str = r#". ./src/errors.sh
validate_attempts() {
    if [ "$1" -ge 0 ]; then return 0; fi
    invalid_attempts
    return 2
}
"#;
const BASE_ERRORS: &str = "invalid_attempts() { printf 'invalid\\n' >&2; }\n";
const ACCEPTANCE: &str = r#"#!/bin/sh
. ./src/config.sh
actual=$(validate_attempts 0 2>&1)
status=$?
[ "$status" -eq 2 ] || { echo "zero attempts must be rejected" >&2; exit 1; }
[ "$actual" = 'max_attempts must be greater than zero' ] || exit 1
actual=$(validate_attempts 1 2>&1)
[ "$?" -eq 0 ] && [ -z "$actual" ] || exit 1
"#;
const PRODUCER: &str = r#"cat > src/config.sh <<'CONFIG'
. ./src/errors.sh
validate_attempts() {
    if [ "$1" -gt 0 ]; then return 0; fi
    invalid_attempts
    return 2
}
CONFIG
cat > src/errors.sh <<'ERRORS'
invalid_attempts() { printf 'max_attempts must be greater than zero\n' >&2; }
ERRORS
"#;

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn acceptance(root: &Path) -> std::process::Output {
    Command::new("/bin/sh")
        .arg("acceptance.sh")
        .current_dir(root)
        .output()
        .unwrap()
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl Fixture {
    async fn verified_patch(&self, runtime: &Runtime) -> PendingRun {
        let pending = self.review(runtime).await;
        let approved = runtime
            .approve(pending.clone(), pending.approval_digest())
            .unwrap();
        let out = runtime
            .execute(
                approved,
                &FakeAdapter(PRODUCER),
                &FakeSupervisor,
                Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(out.run.state, RunState::Verifying);
        let path = Path::new(out.run.worktree.as_ref().unwrap());
        assert!(
            acceptance(path).status.success(),
            "synthetic patch fails unchanged acceptance check"
        );
        assert_eq!(
            fs::read(path.join("acceptance.sh")).unwrap(),
            ACCEPTANCE.as_bytes()
        );
        runtime
            .record_verification(
                &pending,
                VerificationRecord {
                    verifier: "unchanged synthetic max_attempts contract".into(),
                    evidence_sha256: digest(ACCEPTANCE.as_bytes()),
                    artifact_sha256: out.run.artifact_sha256,
                    validation: VerificationOutcome::Passed,
                    objective: VerificationOutcome::Passed,
                },
            )
            .unwrap();
        pending
    }
    fn assert_live_unchanged(&self, head: &str, index: &[u8]) {
        assert_eq!(git(&self.root, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(self.root.join(".git/index")).unwrap(), index);
        assert!(
            git(
                &self.root,
                &["status", "--porcelain=v1", "--untracked-files=all"]
            )
            .is_empty()
        );
        assert_eq!(
            fs::read_to_string(self.root.join("src/config.sh")).unwrap(),
            BASE_CONFIG
        );
        assert_eq!(
            fs::read_to_string(self.root.join("src/errors.sh")).unwrap(),
            BASE_ERRORS
        );
    }
}

// Removing one file, staging the base tree or mutating the user checkout must fail.
#[tokio::test]
async fn two_file_patch_passes_unchanged_acceptance_on_exact_staged_tree_only() {
    let fixture = Fixture::new(true).await;
    let base = acceptance(&fixture.root);
    assert!(!base.status.success());
    assert_eq!(
        String::from_utf8(base.stderr).unwrap(),
        "zero attempts must be rejected\n"
    );
    let head = git(&fixture.root, &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.root.join(".git/index")).unwrap();
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let preview = runtime.staged_integration_preview(&pending).await.unwrap();
    assert_eq!(preview.allowed_scope, ["src"]);
    assert_eq!(
        preview
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["src/config.sh", "src/errors.sh"]
    );
    assert_eq!(preview.source_commit, head.trim());
    assert_eq!(
        preview.source_tree,
        git(&fixture.root, &["rev-parse", "HEAD^{tree}"]).trim()
    );
    for file in &preview.files {
        let before = file.before.as_ref().unwrap();
        assert_eq!(before.mode, "100644");
        assert_eq!(
            before.sha256,
            digest(&fs::read(fixture.root.join(&file.path)).unwrap())
        );
        assert_eq!(
            before.oid,
            git(
                &fixture.root,
                &["rev-parse", &format!("HEAD:{}", file.path)]
            )
            .trim()
        );
        assert_eq!(file.after.as_ref().unwrap().mode, "100644");
    }
    let staged = runtime
        .stage_integration(&pending, &preview.approval_digest)
        .await
        .unwrap();
    assert_eq!(staged.result_tree, preview.result_tree);
    assert_eq!(
        git(&fixture.root, &["rev-parse", &staged.staged_ref]).trim(),
        staged.result_tree
    );
    assert_eq!(
        git(&fixture.root, &["cat-file", "-t", &staged.staged_ref]).trim(),
        "tree"
    );
    let staged_root = fixture.temp.path().join("accepted-tree");
    fs::create_dir(&staged_root).unwrap();
    for path in ["acceptance.sh", "src/config.sh", "src/errors.sh"] {
        let target = staged_root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(
            target,
            git(
                &fixture.root,
                &["show", &format!("{}:{path}", staged.result_tree)],
            ),
        )
        .unwrap();
    }
    assert_eq!(
        fs::read(staged_root.join("acceptance.sh")).unwrap(),
        ACCEPTANCE.as_bytes()
    );
    assert!(acceptance(&staged_root).status.success());
    for file in &preview.files {
        let after = file.after.as_ref().unwrap();
        assert_eq!(
            after.sha256,
            digest(&fs::read(staged_root.join(&file.path)).unwrap())
        );
        assert_eq!(
            after.oid,
            git(
                &fixture.root,
                &[
                    "rev-parse",
                    &format!("{}:{}", staged.result_tree, file.path)
                ]
            )
            .trim()
        );
        assert!(
            git(&fixture.root, &["ls-tree", &staged.result_tree, &file.path])
                .starts_with("100644 blob ")
        );
    }
    assert_eq!(
        git(
            &fixture.root,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                &preview.source_tree,
                &staged.result_tree
            ]
        ),
        "src/config.sh\nsrc/errors.sh\n"
    );
    fixture.assert_live_unchanged(&head, &index);
    assert!(!acceptance(&fixture.root).status.success());
}

// Replaying a digest or racing two processes must not republish the tree.
#[tokio::test]
async fn concurrent_staging_has_one_winner_and_reopened_runtime_refuses_duplicate() {
    let fixture = Fixture::new(true).await;
    let first = fixture.runtime();
    let pending = fixture.verified_patch(&first).await;
    let preview = first.staged_integration_preview(&pending).await.unwrap();
    let (a, b) = tokio::join!(
        first.stage_integration(&pending, &preview.approval_digest),
        first.stage_integration(&pending, &preview.approval_digest)
    );
    assert_ne!(a.is_ok(), b.is_ok(), "exactly one publisher must win");
    let staged = a.or(b).unwrap();
    drop(first);
    let reopened = fixture.runtime();
    let pending = reopened
        .reopen(
            "run-1",
            &fixture.router,
            &fixture.context,
            fixture.request.clone(),
            fixture.plan.clone(),
            "execute",
        )
        .unwrap();
    assert!(matches!(
        reopened
            .stage_integration(&pending, &preview.approval_digest)
            .await,
        Err(RuntimeError::Integrity(
            "staged integration already claimed"
        ))
    ));
    assert_eq!(
        git(&fixture.root, &["rev-parse", &staged.staged_ref]).trim(),
        staged.result_tree
    );
    assert_eq!(
        reopened
            .events()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "integration.staged.completed")
            .count(),
        1
    );
}

// Using an unapproved digest or a changed artifact must never publish a ref.
#[tokio::test]
async fn unapproved_digest_and_tampered_stored_review_diff_are_rejected() {
    for mutation in ["digest", "review_diff"] {
        let fixture = Fixture::new(true).await;
        let runtime = fixture.runtime();
        let pending = fixture.verified_patch(&runtime).await;
        let preview = runtime.staged_integration_preview(&pending).await.unwrap();
        if mutation == "review_diff" {
            fs::write(
                runtime.storage_root().join("artifacts/run-1.patch"),
                "tampered diff\n",
            )
            .unwrap();
        }
        let approval = if mutation == "digest" {
            "unapproved"
        } else {
            &preview.approval_digest
        };
        let result = runtime.stage_integration(&pending, approval).await;
        if mutation == "digest" {
            assert!(
                matches!(result, Err(RuntimeError::Approval)),
                "{mutation}: {result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(RuntimeError::Integrity(_))),
                "{mutation}: {result:?}"
            );
        }
        assert!(
            git(
                &fixture.root,
                &["for-each-ref", "refs/agent-router/staged/"]
            )
            .is_empty(),
            "published after {mutation} tampering"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("src/config.sh")).unwrap(),
            BASE_CONFIG
        );
        assert!(
            !runtime
                .events()
                .unwrap()
                .iter()
                .any(|e| e.kind == "integration.staged.claimed"),
            "invalid approval or artifact consumed its claim"
        );
    }
}

// Immutable staging must preserve later live edits and still use the approved base.
#[tokio::test]
async fn live_source_change_survives_staging_against_immutable_approved_base() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let preview = runtime.staged_integration_preview(&pending).await.unwrap();
    fs::write(
        fixture.root.join("src/config.sh"),
        "# independent user edit\n",
    )
    .unwrap();
    let result = runtime
        .stage_integration(&pending, &preview.approval_digest)
        .await
        .unwrap();
    assert_eq!(result.result_tree, preview.result_tree);
    assert_eq!(
        git(&fixture.root, &["rev-parse", &result.staged_ref]).trim(),
        preview.result_tree
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/config.sh")).unwrap(),
        "# independent user edit\n"
    );
}

// Captured bytes are authoritative; later worker mutations cannot change publication.
#[tokio::test]
async fn later_worker_content_mode_and_scope_changes_cannot_replace_captured_patch() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let preview = runtime.staged_integration_preview(&pending).await.unwrap();
    let worktree = PathBuf::from(runtime.summary("run-1").unwrap().worktree.unwrap());
    fs::write(worktree.join("src/config.sh"), BASE_CONFIG).unwrap();
    fs::set_permissions(
        worktree.join("src/errors.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(worktree.join("outside.txt"), "not approved\n").unwrap();
    let result = runtime
        .stage_integration(&pending, &preview.approval_digest)
        .await
        .unwrap();
    assert_eq!(result.result_tree, preview.result_tree);
    assert_eq!(
        git(
            &fixture.root,
            &["ls-tree", "--name-only", &result.result_tree]
        ),
        "acceptance.sh\nsrc\n"
    );
    let config = git(
        &fixture.root,
        &["show", &format!("{}:src/config.sh", result.result_tree)],
    );
    assert!(config.contains("-gt 0"));
    assert!(
        git(
            &fixture.root,
            &["ls-tree", &result.result_tree, "src/errors.sh"]
        )
        .starts_with("100644 blob ")
    );
    assert!(!fixture.root.join("outside.txt").exists());
}

// A failed publication retains its claim even if the external conflict disappears.
#[tokio::test]
async fn publication_conflict_preserves_existing_ref_and_prevents_blind_retry() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let preview = runtime.staged_integration_preview(&pending).await.unwrap();
    let reserved_ref = "refs/agent-router/staged/run-1";
    git(
        &fixture.root,
        &["update-ref", reserved_ref, &preview.source_tree],
    );
    assert!(
        runtime
            .stage_integration(&pending, &preview.approval_digest)
            .await
            .is_err()
    );
    assert_eq!(
        git(&fixture.root, &["rev-parse", reserved_ref]).trim(),
        preview.source_tree
    );
    assert_eq!(
        runtime
            .events()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "integration.staged.claimed")
            .count(),
        1
    );
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind == "integration.staged.completed")
    );
    git(
        &fixture.root,
        &["update-ref", "-d", reserved_ref, &preview.source_tree],
    );
    drop(runtime);
    let reopened = fixture.runtime();
    assert!(matches!(
        reopened
            .stage_integration(&pending, &preview.approval_digest)
            .await,
        Err(RuntimeError::Integrity(
            "staged integration already claimed"
        ))
    ));
    assert!(
        git(
            &fixture.root,
            &["for-each-ref", "refs/agent-router/staged/"]
        )
        .is_empty()
    );
}

// Object inspection must not turn into an unapproved Git transport invocation.
#[tokio::test]
async fn promisor_repository_is_refused_before_a_missing_tree_can_launch_transport() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let tree = git(&fixture.root, &["rev-parse", "HEAD^{tree}"]);
    let tree = tree.trim();
    let marker = fixture.temp.path().join("transport-was-invoked");
    let upload_pack = fixture.temp.path().join("local-upload-pack");
    fs::write(
        &upload_pack,
        format!("#!/bin/sh\n: > '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&upload_pack, fs::Permissions::from_mode(0o755)).unwrap();
    for (key, value) in [
        ("core.repositoryformatversion", "1"),
        ("extensions.partialClone", "origin"),
        ("remote.origin.promisor", "true"),
        ("remote.origin.partialclonefilter", "blob:none"),
        ("remote.origin.url", fixture.root.to_str().unwrap()),
        ("remote.origin.uploadpack", upload_pack.to_str().unwrap()),
    ] {
        git(&fixture.root, &["config", key, value]);
    }
    // Local synthetic fixture only: make a required Git object absent to expose lazy fetch.
    fs::remove_file(
        fixture
            .root
            .join(".git/objects")
            .join(&tree[..2])
            .join(&tree[2..]),
    )
    .unwrap();
    assert!(matches!(
        runtime.staged_integration_preview(&pending).await,
        Err(RuntimeError::Invalid(
            "repository lazy fetch is not permitted"
        ))
    ));
    assert!(
        !marker.exists(),
        "object inspection invoked local transport"
    );
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind == "integration.staged.claimed")
    );
}

// Git's direct-ref CAS does not by itself preserve a pre-existing dangling symref.
#[tokio::test]
async fn existing_dangling_symbolic_destination_is_refused_and_preserved() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.verified_patch(&runtime).await;
    let preview = runtime.staged_integration_preview(&pending).await.unwrap();
    let staged_ref = "refs/agent-router/staged/run-1";
    git(
        &fixture.root,
        &["symbolic-ref", staged_ref, "refs/heads/absent"],
    );
    assert!(matches!(
        runtime
            .stage_integration(&pending, &preview.approval_digest)
            .await,
        Err(RuntimeError::Integrity("staged destination is symbolic"))
    ));
    assert_eq!(
        git(&fixture.root, &["symbolic-ref", staged_ref]),
        "refs/heads/absent\n"
    );
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind == "integration.staged.completed")
    );
}

// Stored verification must still authorize the exact patch at publication time.
#[tokio::test]
async fn changed_verification_after_review_refuses_publication_without_consuming_claim() {
    for mutation in ["unknown", "failed", "artifact"] {
        let fixture = Fixture::new(true).await;
        let runtime = fixture.runtime();
        let pending = fixture.verified_patch(&runtime).await;
        let preview = runtime.staged_integration_preview(&pending).await.unwrap();
        let mut record = runtime.verification_record("run-1").unwrap().unwrap();
        match mutation {
            "unknown" => record.validation = VerificationOutcome::Unknown,
            "failed" => record.objective = VerificationOutcome::Failed,
            "artifact" => record.artifact_sha256 = Some("0".repeat(64)),
            _ => unreachable!(),
        }
        let db =
            rusqlite::Connection::open(runtime.storage_root().join("runtime.sqlite3")).unwrap();
        assert_eq!(
            db.execute(
                "UPDATE verifications SET record=?1 WHERE run_id='run-1'",
                [serde_json::to_string(&record).unwrap()]
            )
            .unwrap(),
            1
        );
        drop(db);
        assert!(
            matches!(
                runtime
                    .stage_integration(&pending, &preview.approval_digest)
                    .await,
                Err(RuntimeError::Verification)
            ),
            "accepted changed {mutation} verification"
        );
        assert!(
            git(
                &fixture.root,
                &["for-each-ref", "refs/agent-router/staged/"]
            )
            .is_empty()
        );
        assert!(
            !runtime
                .events()
                .unwrap()
                .iter()
                .any(|e| e.kind == "integration.staged.claimed")
        );
    }
}

// The bounded text-only staging slice must refuse binary patches before publication.
#[tokio::test]
async fn captured_binary_patch_is_refused_before_claim_or_publication() {
    let fixture = Fixture::new(true).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("printf '\\000\\001' > src/binary.bin"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        fs::read(Path::new(out.run.worktree.as_ref().unwrap()).join("src/binary.bin")).unwrap(),
        [0, 1]
    );
    // Explicit fixture verifier accepts the produced bytes; staging has its own text-only limit.
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "binary refusal fixture".into(),
                evidence_sha256: digest(&[0, 1]),
                artifact_sha256: out.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    assert!(matches!(
        runtime.staged_integration_preview(&pending).await,
        Err(RuntimeError::Invalid(
            "binary staged patches are not supported"
        ))
    ));
    assert!(
        git(
            &fixture.root,
            &["for-each-ref", "refs/agent-router/staged/"]
        )
        .is_empty()
    );
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind == "integration.staged.claimed")
    );
    assert!(!fixture.root.join("src/binary.bin").exists());
}

// A tiny textual delta must not let cached apply materialize an oversized base blob.
#[tokio::test]
async fn oversized_base_blob_is_refused_before_creating_git_objects() {
    let mut fixture = Fixture::new(true).await;
    let mut large = b"old\ncontext\ncontext\ncontext\ncontext\ncontext\n".to_vec();
    large.extend(std::iter::repeat_n(b'a', 16 * 1024 * 1024));
    large.push(b'\n');
    fs::write(fixture.root.join("src/large.txt"), &large).unwrap();
    git(&fixture.root, &["add", "src/large.txt"]);
    git(
        &fixture.root,
        &[
            "-c",
            "user.name=Runtime Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "--no-gpg-sign",
            "-m",
            "oversized synthetic blob",
        ],
    );
    fixture.request.source_state = source_state(&fixture.root).await.unwrap();
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let out = runtime
        .execute(
            approved,
            &FakeAdapter("printf new | dd of=src/large.txt bs=1 count=3 conv=notrunc 2>/dev/null"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    let changed =
        fs::read(Path::new(out.run.worktree.as_ref().unwrap()).join("src/large.txt")).unwrap();
    assert_eq!(&changed[..3], b"new");
    assert_eq!(&changed[3..], &large[3..]);
    assert!(
        fs::metadata(runtime.storage_root().join("artifacts/run-1.patch"))
            .unwrap()
            .len()
            < 1024
    );
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "large base refusal fixture".into(),
                evidence_sha256: digest(&changed),
                artifact_sha256: out.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    let before = git(&fixture.root, &["count-objects", "-v"]);
    assert!(matches!(
        runtime.staged_integration_preview(&pending).await,
        Err(RuntimeError::Limit)
    ));
    assert_eq!(
        git(&fixture.root, &["count-objects", "-v"]),
        before,
        "rejection occurred after writing oversized Git objects"
    );
    assert!(
        git(
            &fixture.root,
            &["for-each-ref", "refs/agent-router/staged/"]
        )
        .is_empty()
    );
}
