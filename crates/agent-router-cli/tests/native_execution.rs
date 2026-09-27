//! Explicit native canaries. The executable speaks a synthetic Claude protocol;
//! no provider, model, credential or external network service is used.
#![cfg(all(feature = "claude", any(target_os = "macos", target_os = "linux")))]

use router_adapter_api::{
    HostObservation, SnapshotValidation, ValidatedHostObservation, content_digest,
};
use router_catalog::Catalog;
use router_policy::PhaseRouter;
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const RUN_ID: &str = "native-cli-fixture";
const PROMPT: &str = "synthetic-private-prompt-never-exported";
const STREAM: &str = concat!(
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

fn command() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.env_clear().env("PATH", "/usr/bin:/bin");
    command
}
fn checked(command: &mut Command, code: i32) -> Value {
    let output = command.output().expect("launch local CLI");
    parsed(output, code)
}
fn parsed(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI must produce its JSON contract")
}
fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    store: PathBuf,
    feedback: PathBuf,
}
impl Fixture {
    fn new(body: &str, timeout_ms: u64) -> Self {
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
            "../../../fixtures/examples/phase-request.json"
        ))
        .unwrap();
        request.phase = StageKind::Execute;
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
            "../../../fixtures/examples/phase-snapshot.json"
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
        let catalog_bytes = include_bytes!("../../../fixtures/examples/phase-catalog.json");
        let catalog = Catalog::from_json(catalog_bytes).unwrap();
        let router = PhaseRouter::stable_v2();
        let context = router
            .context(request.clone(), &validated, &catalog)
            .unwrap();
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
            &json!({"schema_version":1,"run_id":RUN_ID,"context":"context.json","host_snapshot":"snapshot.json","catalog":"catalog.json","policy":null,"plan":"plan.json","node_id":"execute","request":"request.json"}),
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
        let helper = Path::new(assert_cmd::cargo::cargo_bin!("agent-router"))
            .parent()
            .unwrap()
            .join("router-sandbox-helper");
        assert!(
            helper.is_file(),
            "build router-sandbox-helper before running native canaries"
        );
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
        let bubblewrap = if cfg!(target_os = "linux") {
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
    fn run(&self) -> Command {
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
    fn review(&self) -> String {
        let review = checked(&mut self.run(), 0);
        assert_eq!(review["executed"], false);
        assert_eq!(review["run"]["state"], "awaiting_approval");
        assert!(!review.to_string().contains(PROMPT));
        assert!(!self.store.join("worktrees").exists());
        review["approval_digest"].as_str().unwrap().into()
    }
    fn status(&self) -> Value {
        checked(
            command()
                .args(["status", "--store"])
                .arg(&self.store)
                .args(["--run-id", RUN_ID, "--json"]),
            0,
        )
    }
    fn artifacts(&self, action: &str) -> Command {
        let mut cmd = command();
        cmd.args(["artifacts", "--definition"])
            .arg(self.root.join("definition.json"))
            .arg("--store")
            .arg(&self.store)
            .args(["--json", action]);
        cmd
    }
    fn exported_feedback(&self) -> Value {
        checked(
            command()
                .args(["feedback", "--store"])
                .arg(&self.feedback)
                .args(["export", "--json"]),
            0,
        )
    }
}

#[test]
#[ignore = "requires native OS sandbox, a built helper and local CLI; no live model"]
fn native_cli_review_run_feedback_verify_and_private_outputs() {
    let body = format!(
        "printf isolated > src/result.txt\nprintf '%s' {}\nprintf 'synthetic-private-stderr\\n' >&2",
        shell_quote(STREAM)
    );
    let fixture = Fixture::new(&body, 3_000);
    let digest = fixture.review();
    let output = checked(fixture.run().args(["--approve", &digest]), 0);
    let completion = &output["completion"];
    assert_eq!(completion["run"]["state"], "verifying");
    assert_eq!(completion["transport"]["termination"], "exited");
    assert_eq!(completion["transport"]["exit_code"], 0);
    assert_eq!(completion["validation"], "unknown");
    assert_eq!(completion["objective"], "unknown");
    assert_eq!(
        completion["memory_enforcement"],
        if cfg!(target_os = "linux") {
            "enforced"
        } else {
            "not_requested"
        }
    );
    assert_eq!(completion["runtime_evidence"]["transport_success"], true);
    assert_eq!(completion["runtime_evidence"]["executed_models"], json!([]));
    assert!(
        completion["runtime_evidence"]["missing_evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "model.runtime_unknown")
    );
    assert_eq!(output["feedback"]["recorded"], true);
    assert_eq!(fixture.status()["run"]["state"], "verifying");
    assert!(!fixture.repo.join("src/result.txt").exists());
    assert!(!fixture.repo.join("outside.txt").exists());
    let worktree = Path::new(completion["run"]["worktree"].as_str().unwrap());
    assert_eq!(
        fs::read(worktree.join("src/result.txt")).unwrap(),
        b"isolated"
    );
    assert!(!worktree.join("outside.txt").exists());
    let artifacts = checked(&mut fixture.artifacts("outputs"), 0);
    let record = &artifacts["outputs"][0];
    assert_eq!(record["state"], "retained");
    assert_eq!(record["truncated"], false);
    for (stream, expected) in [
        ("stdout", STREAM.as_bytes()),
        ("stderr", b"synthetic-private-stderr\n".as_slice()),
    ] {
        let path = Path::new(record[stream]["path"].as_str().unwrap());
        assert_eq!(fs::read(path).unwrap(), expected);
        assert_eq!(record[stream]["sha256"], content_digest(expected));
        assert_eq!(record[stream]["bytes"], expected.len());
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
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
    }
    let feedback = fixture.exported_feedback();
    assert_eq!(feedback["events"].as_array().unwrap().len(), 1);
    let event = &feedback["events"][0]["input"];
    assert_eq!(event["origin"], "automated");
    assert_eq!(event["transport"], "passed");
    assert_eq!(event["validation"], "unknown");
    assert_eq!(event["objective"], "unknown");
    assert_eq!(event["context"]["harness"], "claude-code");
    let events = checked(
        command()
            .args(["events", "--store"])
            .arg(&fixture.store)
            .args(["--run-id", RUN_ID, "--json"]),
        0,
    );
    for preview in [&output, &feedback, &events, &artifacts] {
        let text = preview.to_string();
        for secret in [
            PROMPT,
            "synthetic-private-output",
            "synthetic-private-result",
            "synthetic-private-stderr",
        ] {
            assert!(!text.contains(secret));
        }
    }
    // Independent test assertions above, not the fake host's verified=true,
    // supply this explicit verification evidence.
    let evidence = fixture.root.join("verification.json");
    write(
        &evidence,
        &json!({"verifier":"native-cli-test","evidence_sha256":content_digest(b"isolated path, exact stream bytes and private mode assertions passed"),"artifact_sha256":completion["run"]["artifact_sha256"],"validation":"passed","objective":"passed"}),
    );
    let verified = checked(
        command()
            .args(["verify", "--definition"])
            .arg(fixture.root.join("definition.json"))
            .arg("--store")
            .arg(&fixture.store)
            .arg("--evidence")
            .arg(evidence)
            .arg("--json"),
        0,
    );
    assert_eq!(verified["run"]["state"], "verified");
    assert_eq!(
        fixture.exported_feedback()["events"][0]["input"]["objective"],
        "unknown"
    );
    let purged = checked(
        fixture.artifacts("purge-outputs").args(["--attempt", "1"]),
        0,
    );
    assert_eq!(purged["purged"]["state"], "purged");
    for stream in ["stdout", "stderr"] {
        assert!(!Path::new(record[stream]["path"].as_str().unwrap()).exists());
    }
    assert_eq!(
        checked(&mut fixture.artifacts("outputs"), 0)["outputs"][0]["state"],
        "purged"
    );
}

// Shell builtins keep the signal handler runnable immediately. Waiting on a
// separate sleep process can defer the handler past the runtime's grace period.
const TERM_ZERO: &str =
    "trap 'printf term-handled; exit 0' TERM\nprintf ready > src/ready\nwhile :; do :; done";
fn assert_interrupted(output: &Value, state: &str, termination: &str) {
    let completion = &output["completion"];
    assert_eq!(completion["run"]["state"], state);
    assert_eq!(completion["transport"]["termination"], termination);
    assert_eq!(completion["validation"], "unknown");
    assert_eq!(completion["objective"], "unknown");
    // Seatbelt exec preserves the worker status. Linux's outer namespace
    // supervisor may instead report termination of the wrapper itself.
    #[cfg(target_os = "macos")]
    {
        assert_eq!(
            completion["transport"]["exit_code"],
            0,
            "transport {:?}; stdout {:?}; stderr {:?}; ready {:?}",
            completion["transport"],
            fs::read_to_string(
                completion["output_artifacts"]["stdout"]["path"]
                    .as_str()
                    .unwrap()
            ),
            fs::read_to_string(
                completion["output_artifacts"]["stderr"]["path"]
                    .as_str()
                    .unwrap()
            ),
            fs::read_to_string(
                Path::new(completion["run"]["worktree"].as_str().unwrap()).join("src/ready")
            )
        );
        assert_eq!(
            fs::read(
                completion["output_artifacts"]["stdout"]["path"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
            b"term-handled"
        );
    }
}

#[test]
#[ignore = "requires native OS sandbox and built helper; zero-exit TERM handler regression"]
fn native_cli_timeout_with_zero_exit_handler_is_failure() {
    let fixture = Fixture::new(TERM_ZERO, 1_500);
    let digest = fixture.review();
    let output = checked(fixture.run().args(["--approve", &digest]), 3);
    assert_interrupted(&output, "timed_out", "timed_out");
    let feedback = fixture.exported_feedback();
    assert_eq!(feedback["events"][0]["input"]["transport"], "failed");
    assert_eq!(feedback["events"][0]["input"]["failure"], "timeout");
}

#[test]
#[ignore = "requires native OS sandbox and built helper; second-process cancellation regression"]
fn native_cli_cancel_with_zero_exit_handler_is_failure() {
    let fixture = Fixture::new(TERM_ZERO, 5_000);
    let digest = fixture.review();
    let mut child = fixture
        .run()
        .args(["--approve", &digest])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("worker finished before cancellation: {status}");
        }
        let status = fixture.status();
        if status["run"]["state"] == "running"
            && status["run"]["worktree"]
                .as_str()
                .is_some_and(|path| Path::new(path).join("src/ready").is_file())
        {
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("native fake worker never reached readiness");
        }
        thread::sleep(Duration::from_millis(10));
    }
    checked(
        command()
            .args(["cancel", "--store"])
            .arg(&fixture.store)
            .args(["--run-id", RUN_ID, "--json"]),
        0,
    );
    let output = parsed(child.wait_with_output().unwrap(), 3);
    assert_interrupted(&output, "cancelled", "cancelled");
    assert_eq!(fixture.status()["run"]["state"], "cancelled");
    let feedback = fixture.exported_feedback();
    assert_eq!(feedback["events"][0]["input"]["transport"], "failed");
    assert_eq!(feedback["events"][0]["input"]["failure"], "cancelled");
}
