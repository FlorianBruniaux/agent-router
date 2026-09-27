//! Explicit native canaries. The executable speaks a synthetic Claude protocol;
//! no provider, model, credential or external network service is used.
#![allow(dead_code)] // Shared fixture consumers intentionally exercise different methods.

use router_adapter_api::{HostObservation, SnapshotValidation, ValidatedHostObservation};
use router_catalog::Catalog;
use router_policy::PhaseRouter;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const RUN_ID: &str = "native-cli-fixture";
pub(crate) const PROMPT: &str = "synthetic-private-prompt-never-exported";
pub(crate) const STREAM: &str = concat!(
    "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"configured-only-model\"}\n",
    "{\"type\":\"assistant\",\"message\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"verified=true; synthetic-private-output\"}]}}\n",
    "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"synthetic-private-result\"}\n",
);
const ARGUMENT_CHECK: &str = r#"#!/bin/sh
set -eu
tools=missing
permission=missing
prompts=missing
while [ "$#" -gt 0 ]; do
    case "$1" in
        --tools) [ "$#" -ge 2 ] && [ -z "$2" ] || exit 41; tools=disabled; shift 2 ;;
        --permission-mode) permission="$2"; shift 2 ;;
        --permission-prompts) prompts="$2"; shift 2 ;;
        --model) [ "$2" = fixture-best ] || exit 42; shift 2 ;;
        --effort) [ "$2" = medium ] || exit 43; shift 2 ;;
        --output-format) [ "$2" = stream-json ] || exit 44; shift 2 ;;
        --input-format) [ "$2" = text ] || exit 45; shift 2 ;;
        --print|--verbose|--no-session-persistence) shift ;;
        *) exit 46 ;;
    esac
done
[ "$tools" = disabled ] && [ "$permission" = manual ] && [ "$prompts" = none ] || exit 47
/bin/cat >/dev/null
"#;

pub(crate) fn command() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.env_clear().env("PATH", "/usr/bin:/bin");
    command
}
pub(crate) fn checked(command: &mut Command, code: i32) -> Value {
    let output = command.output().expect("launch local CLI");
    parsed(output, code)
}
pub(crate) fn parsed(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI must produce its JSON contract")
}
pub(crate) fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

// Synthetic test contracts only. Native tests below establish OS containment,
// not provider authentication, model limits or live model availability.
#[path = "../../../router-policy/tests/support/asset_fixture.rs"]
mod asset_fixture;

pub(crate) fn asset_inputs(
    request: &PhaseRouteRequest,
    snapshot: &ValidatedHostObservation,
    catalog: &Catalog,
    workspace: &Path,
    source_state: &str,
    now: u64,
) -> router_policy::PhaseAssetInputs {
    let mut historical = snapshot.observation().clone();
    historical.observed_at_unix_seconds = 100;
    for evidence in &mut historical.evidence {
        evidence.observed_at_unix_seconds = 100;
    }
    let historical = ValidatedHostObservation::from_observation(
        historical.seal().unwrap(),
        SnapshotValidation {
            now_unix_seconds: 100,
            max_age_seconds: 300,
        },
    )
    .unwrap();
    let mut inputs = asset_fixture::inputs(
        request,
        &historical,
        catalog,
        router_policy::PhaseExecutionScope {
            workspace: workspace.to_str().unwrap().into(),
            source_state: source_state.into(),
            scope: vec!["src".into()],
        },
        &std::collections::BTreeMap::new(),
        4000,
    );
    let mut availability = inputs.availability.document().clone();
    availability.snapshot_hash = snapshot.hash().into();
    availability.observed_at_unix_seconds = now;
    availability.expires_at_unix_seconds = now + 300;
    for evidence in &mut availability.evidence {
        evidence.observed_at_unix_seconds = now;
    }
    let availability = router_asset_contracts::seal_availability(availability, snapshot).unwrap();
    inputs.availability = router_asset_contracts::ValidatedAvailability::from_json(
        &serde_json::to_vec(&availability).unwrap(),
        snapshot,
        &availability.content_hash,
        now,
    )
    .unwrap();
    inputs.policy.maximum_evidence_age_seconds = 300;
    for candidate in &request.candidates {
        let extension = inputs.requests.get_mut(&candidate.id).unwrap();
        extension
            .required_capabilities
            .extend(&request.task.required_capabilities);
        if candidate.sandbox == PhaseSandbox::WorkspaceWrite {
            extension
                .required_capabilities
                .insert(Capability::WorkspaceWrite);
            extension
                .required_mutations
                .insert(Mutation::WorkspaceWrite);
        }
    }
    inputs
}

pub(crate) fn write_asset_authority(root: &Path, inputs: &router_policy::PhaseAssetInputs) {
    write(
        &root.join("asset-contracts.json"),
        inputs.contracts.document(),
    );
    write(
        &root.join("asset-availability.json"),
        inputs.availability.document(),
    );
    write(&root.join("asset-policy.json"), &inputs.policy);
    write(&root.join("asset-requests.json"), &inputs.requests);
    write(
        &root.join("asset-authority.json"),
        &json!({"schema_version":1,
        "contracts":{"path":"asset-contracts.json","expected_hash":inputs.contracts.hash()},
        "availability":{"path":"asset-availability.json","expected_hash":inputs.availability.hash()},
        "policy":"asset-policy.json","requests":"asset-requests.json","execution_scope":inputs.execution_scope,
        "approved_review_digests":inputs.approved_review_digests,"metadata":null,"body_root":null}),
    );
}

pub(crate) struct Fixture {
    _temp: tempfile::TempDir,
    pub(crate) root: PathBuf,
    pub(crate) repo: PathBuf,
    pub(crate) store: PathBuf,
    pub(crate) feedback: PathBuf,
}
impl Fixture {
    pub(crate) fn new(body: &str, timeout_ms: u64) -> Self {
        Self::build(body, timeout_ms, true)
    }
    /// Review-only fixture. This dummy helper is never eligible for dispatch.
    /// Native containment tests must always use `new` instead.
    pub(crate) fn metadata(body: &str, timeout_ms: u64) -> Self {
        Self::build(body, timeout_ms, false)
    }
    fn build(body: &str, timeout_ms: u64, native_sandbox: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(repo.join("src")).unwrap();
        fs::write(repo.join("src/base.txt"), b"base\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "src"],
            vec![
                "-c",
                "user.name=Native Fixture",
                "-c",
                "user.email=native@example.invalid",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "fixture",
            ],
        ] {
            let status = Command::new("/usr/bin/git")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let source = checked(
            command()
                .args(["source-state", "--workspace"])
                .arg(&repo)
                .arg("--json"),
            0,
        );
        let mut request: PhaseRouteRequest = serde_json::from_str(include_str!(
            "../../../../fixtures/examples/phase-request.json"
        ))
        .unwrap();
        request.phase = StageKind::Execute;
        request.task.budget.max_tokens = 10_000;
        request.restrictions.max_tokens = Some(10_000);
        request.restrictions.policy_hash.clear();
        request.restrictions.policy_hash =
            router_adapter_api::content_digest(&serde_json::to_vec(&request.restrictions).unwrap());
        request.candidates[0].estimated_tokens = 4500;
        request.task.budget.max_elapsed_ms = 30_000;
        request.task.allowed_mutations.remove(&Mutation::Network);
        request.candidates.truncate(1);
        request.candidates[0].sandbox = PhaseSandbox::WorkspaceWrite;
        let plan = build_plan(
            &request.task,
            WorkflowPreset::DirectSafe,
            PlanLimits::default(),
        )
        .unwrap();
        let mut snapshot: HostObservation = serde_json::from_str(include_str!(
            "../../../../fixtures/examples/phase-snapshot.json"
        ))
        .unwrap();
        snapshot.host.harness = HarnessId::parse("claude-code").unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        snapshot.observed_at_unix_seconds = now;
        for evidence in &mut snapshot.evidence {
            evidence.observed_at_unix_seconds = now;
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
        let catalog_bytes = include_bytes!("../../../../fixtures/examples/phase-catalog.json");
        let catalog = Catalog::from_json(catalog_bytes).unwrap();
        let router = PhaseRouter::stable_v2();
        let assets = asset_inputs(
            &request,
            &validated,
            &catalog,
            &repo,
            source["source_state"].as_str().unwrap(),
            now,
        );
        let context = router
            .context_with_assets(request.clone(), &validated, &catalog, assets.clone(), now)
            .unwrap();
        write_asset_authority(&root, &assets);
        let receipt = router.route(&context).unwrap();
        let selected = receipt.selected.as_ref().unwrap();
        let execution = ExecutionRequest {
            schema_version: 1,
            receipt_id: receipt.receipt_id.clone(),
            candidate_id: selected.candidate.id.clone(),
            phase: StageKind::Execute,
            host: validated.host().harness.clone(),
            model: selected.candidate.model.clone(),
            effort: selected.candidate.effort,
            workspace: repo.to_str().unwrap().into(),
            source_state: source["source_state"].as_str().unwrap().into(),
            scope: vec!["src".into()],
            sandbox: ExecutionSandbox::WorkspaceWrite,
            required_capabilities: router.execution_capabilities(&context, selected).unwrap(),
            tools: ToolSelection::Disabled,
            agent: None,
            skills: vec![],
            prompt: PROMPT.into(),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            allow_degradation: false,
            timeout_ms,
            max_output_bytes: 16384,
        };
        assert!(
            !execution
                .required_capabilities
                .contains(&Capability::Network)
        );
        write(&root.join("context.json"), &request);
        write(&root.join("snapshot.json"), &snapshot);
        write(&root.join("request.json"), &execution);
        write(&root.join("plan.json"), &plan);
        fs::write(root.join("catalog.json"), catalog_bytes).unwrap();
        write(
            &root.join("definition.json"),
            &json!({"schema_version":1,"run_id":RUN_ID,"context":"context.json","host_snapshot":"snapshot.json","catalog":"catalog.json","policy":null,"asset_authority":"asset-authority.json","plan":"plan.json","node_id":"execute","request":"request.json"}),
        );
        let fake = root.join("fake-claude");
        let secret = root.join("denied-secret");
        fs::write(&secret, b"synthetic-denied-read").unwrap();
        let script = format!(
            "{ARGUMENT_CHECK}\nif (printf escaped > outside.txt) 2>/dev/null; then exit 48; fi\nif /bin/cat {} >/dev/null 2>/dev/null; then exit 49; fi\n{body}\n",
            shell_quote(secret.to_str().unwrap())
        );
        fs::write(&fake, script).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let helper = if native_sandbox {
            let helper = Path::new(assert_cmd::cargo::cargo_bin!("agent-router"))
                .parent()
                .unwrap()
                .join("router-sandbox-helper");
            assert!(
                helper.is_file(),
                "build router-sandbox-helper before running native canaries"
            );
            helper
        } else {
            PathBuf::from("/bin/sh")
        };
        let helper = fs::canonicalize(helper).unwrap();
        let feedback = root.join("feedback");
        write(
            &root.join("preferences.json"),
            &json!({"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":3600}),
        );
        checked(
            command()
                .args(["feedback", "--store"])
                .arg(&feedback)
                .args(["configure", "--preferences"])
                .arg(root.join("preferences.json"))
                .arg("--json"),
            0,
        );
        let memory = if cfg!(target_os = "linux") {
            Some(256_u64 * 1024 * 1024)
        } else {
            None
        };
        let bubblewrap = if native_sandbox && cfg!(target_os = "linux") {
            Some(fs::canonicalize("/usr/bin/bwrap").expect("native bubblewrap is required"))
        } else {
            None
        };
        write(
            &root.join("sandbox.json"),
            &json!({"schema_version":1,"programs":{"claude":fake},"read_roots":[],"environment_names":[],"helper":helper,"bubblewrap":bubblewrap,"feedback_store":feedback,"output_retention":{"mode":"private_artifacts","retention_seconds":60},"limits":{"cpu_seconds":10,"address_space_bytes":memory,"max_open_files":128,"max_processes":4096,"max_file_bytes":1048576}}),
        );
        Self {
            _temp: temp,
            store: root.join("store"),
            repo,
            root,
            feedback,
        }
    }
    pub(crate) fn run(&self) -> Command {
        let mut cmd = command();
        cmd.args(["run", "--definition"])
            .arg(self.root.join("definition.json"))
            .arg("--store")
            .arg(&self.store)
            .arg("--sandbox")
            .arg(self.root.join("sandbox.json"))
            .arg("--json");
        cmd
    }
    pub(crate) fn review(&self) -> String {
        let review = checked(&mut self.run(), 0);
        assert_eq!(review["executed"], false);
        assert_eq!(review["run"]["state"], "awaiting_approval");
        assert!(!review.to_string().contains(PROMPT));
        assert!(!self.store.join("worktrees").exists());
        review["approval_digest"].as_str().unwrap().into()
    }
    pub(crate) fn status(&self) -> Value {
        checked(
            command()
                .args(["status", "--store"])
                .arg(&self.store)
                .args(["--run-id", RUN_ID, "--json"]),
            0,
        )
    }
    pub(crate) fn artifacts(&self, action: &str) -> Command {
        let mut cmd = command();
        cmd.args(["artifacts", "--definition"])
            .arg(self.root.join("definition.json"))
            .arg("--store")
            .arg(&self.store)
            .args(["--json", action]);
        cmd
    }
    pub(crate) fn exported_feedback(&self) -> Value {
        checked(
            command()
                .args(["feedback", "--store"])
                .arg(&self.feedback)
                .args(["export", "--json"]),
            0,
        )
    }
}
