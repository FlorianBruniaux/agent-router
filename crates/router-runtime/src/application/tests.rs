//! Real disposable owner processes; no provider or native-model execution.
use super::*;
use crate::*;
use router_adapter_api::{
    AdapterError, HarnessAdapter, HostObservation, SnapshotValidation, ValidatedExecutionRequest,
    prepared_run,
};
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_types::*;
use router_workflow::{PlanLimits, build_plan};
use std::{
    fs,
    io::Write,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "../../../../tests/support/runtime_fixture.rs"]
#[allow(dead_code, clippy::duplicate_mod)]
mod fixture;
#[path = "../../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)]
mod fixture_proofs;
use fixture::*;

const CHILD: &str = "application::tests::application_process_child";
const ROOT_ENV: &str = "AR_APPLICATION_PUBLIC_FIXTURE";
const STAGE_ENV: &str = "AR_APPLICATION_CUT_STAGE";
const CHANGE: &str = "printf 'changed\\n' > src/base.txt; head -c 32768 /dev/zero | tr '\\000' x > src/z.txt; printf '\\n' >> src/z.txt";

struct Cut {
    stage: String,
    marker: PathBuf,
    before: AtomicUsize,
    after: AtomicUsize,
    file_limit: Mutex<Option<rustix::process::Rlimit>>,
}
impl Cut {
    fn park(&self) {
        let temporary = self.marker.with_extension("pending");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .unwrap();
        file.write_all(self.stage.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);
        fs::rename(temporary, &self.marker).unwrap();
        loop {
            std::thread::park();
        }
    }
}
impl Drop for Cut {
    fn drop(&mut self) {
        if let Ok(slot) = self.file_limit.get_mut() {
            if let Some(old) = slot.take() {
                let _ = rustix::process::setrlimit(rustix::process::Resource::Fsize, old);
            }
        }
    }
}
impl workspace::CaptureObserver for Cut {
    fn before_capture(&self) -> Result<()> {
        // The first capture reads Git config. The second is the actual apply.
        if self.before.fetch_add(1, Ordering::SeqCst) == 1 {
            if self.stage == "before" {
                self.park();
            }
            if self.stage == "partial" {
                use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
                let old = getrlimit(Resource::Fsize);
                setrlimit(
                    Resource::Fsize,
                    Rlimit {
                        current: Some(4096),
                        maximum: old.maximum,
                    },
                )
                .map_err(std::io::Error::from)?;
                *self.file_limit.lock().unwrap() = Some(old);
            }
        }
        Ok(())
    }
    fn after_capture(&self) -> Result<()> {
        if self.after.fetch_add(1, Ordering::SeqCst) == 1 {
            if let Some(old) = self.file_limit.lock().unwrap().take() {
                rustix::process::setrlimit(rustix::process::Resource::Fsize, old)
                    .map_err(std::io::Error::from)?;
            }
            // The Git child has been collected. In partial, its real writes
            // hit the file-size limit; never limit the suite's parent process.
            if matches!(self.stage.as_str(), "after" | "partial") {
                self.park();
            }
        }
        Ok(())
    }
}

#[tokio::test]
async fn application_process_child() {
    let Some(outer) = std::env::var_os(ROOT_ENV) else {
        return;
    };
    let outer = PathBuf::from(outer);
    let stage = std::env::var(STAGE_ENV).unwrap();
    assert!(["before", "after", "partial", "complete"].contains(&stage.as_str()));
    let fixture = Fixture::new_at(true, tempfile::tempdir_in(&outer).unwrap()).await;
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let result = runtime
        .execute(
            approved,
            &FakeAdapter(CHANGE),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "authored interruption fixture".into(),
                evidence_sha256: "b".repeat(64),
                artifact_sha256: result.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    let preview = runtime.integration_preview(&pending).await.unwrap();
    let revision = runtime.review_publication(&pending).await.unwrap();
    fs::write(
        outer.join("layout.json"),
        serde_json::to_vec(&serde_json::json!({
            "root":&fixture.root, "storage":fixture.temp.path().join("runtime"),
            "phase_request":fixture.context.request(), "observation":fixture.context.snapshot(),
            "request":&fixture.request, "plan":&fixture.plan, "preview":preview, "review":revision,
        }))
        .unwrap(),
    )
    .unwrap();
    let cut = Cut {
        stage,
        marker: outer.join("ready"),
        before: AtomicUsize::new(0),
        after: AtomicUsize::new(0),
        file_limit: Mutex::new(None),
    };
    runtime
        .apply_observed(&pending, &preview.approval_digest, Some(&cut))
        .await
        .unwrap();
    drop(runtime);
    // The parent owns the enclosing TempDir and must inspect these actual files
    // after normal child completion as well as after SIGKILL.
    std::mem::forget(fixture);
}

struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn wait_ready(&mut self, marker: &Path, stage: &str) {
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            if marker.exists() {
                assert_eq!(fs::read_to_string(marker).unwrap(), stage);
                return;
            }
            assert!(
                self.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "child exited before {stage}"
            );
            assert!(Instant::now() < end, "missing {stage} boundary");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn finish(&mut self, kill: bool) {
        let child = self.0.as_mut().unwrap();
        if kill {
            child.kill().unwrap();
        }
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                if kill {
                    assert_eq!(status.signal(), Some(9));
                } else {
                    assert!(status.success());
                }
                self.0.take();
                return;
            }
            assert!(Instant::now() < end, "child did not stop");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let end = Instant::now() + Duration::from_secs(1);
            while Instant::now() < end {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("/usr/bin/git")
        .args(["-c", "core.hooksPath=/dev/null", "-C"])
        .arg(root)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>, u32)> {
    [
        ".git/HEAD",
        ".git/index",
        "src/base.txt",
        "src/z.txt",
        "user.txt",
    ]
    .into_iter()
    .filter_map(|name| {
        let p = root.join(name);
        p.exists().then(|| {
            (
                p.clone(),
                fs::read(&p).unwrap(),
                fs::metadata(&p).unwrap().permissions().mode(),
            )
        })
    })
    .collect()
}
async fn exercise(stage: &str) {
    let original_limit = rustix::process::getrlimit(rustix::process::Resource::Fsize);
    let temp = tempfile::tempdir().unwrap();
    let outer = fs::canonicalize(temp.path()).unwrap();
    fs::create_dir(outer.join("home")).unwrap();
    fs::create_dir(outer.join("tmp")).unwrap();
    let mut child = OwnedChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--test-threads=1", "--nocapture"])
            .current_dir(&outer)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", outer.join("home"))
            .env("TMPDIR", outer.join("tmp"))
            .env(ROOT_ENV, &outer)
            .env(STAGE_ENV, stage)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(outer.join("child.stderr")).unwrap())
            .spawn()
            .unwrap(),
    ));
    if stage != "complete" {
        child.wait_ready(&outer.join("ready"), stage);
    }
    child.finish(stage != "complete");
    assert_eq!(
        rustix::process::getrlimit(rustix::process::Resource::Fsize),
        original_limit
    );
    let layout: serde_json::Value =
        serde_json::from_slice(&fs::read(outer.join("layout.json")).unwrap()).unwrap();
    let root = PathBuf::from(layout["root"].as_str().unwrap());
    let storage = PathBuf::from(layout["storage"].as_str().unwrap());
    let preview: IntegrationPreview = serde_json::from_value(layout["preview"].clone()).unwrap();
    let first = fs::read(root.join("src/base.txt")).unwrap();
    if stage == "before" {
        assert_eq!(first, b"base\n");
        assert!(!root.join("src/z.txt").exists());
    } else {
        assert_eq!(first, b"changed\n");
        assert_eq!(
            fs::metadata(root.join("src/z.txt")).unwrap().len(),
            if stage == "partial" { 4096 } else { 32769 }
        );
    }
    // Concurrent user edits are authored here, after the old owner is dead.
    fs::write(root.join("src/base.txt"), b"staged user version\n").unwrap();
    git(&root, &["add", "src/base.txt"]);
    fs::write(root.join("src/base.txt"), b"unstaged user version\n").unwrap();
    fs::write(root.join("user.txt"), b"untracked user version\n").unwrap();
    fs::set_permissions(root.join("user.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    let before = snapshot(&root);
    let runtime = Runtime::open(&storage, RuntimeLimits::default())
        .unwrap()
        .with_capability_proof_verifier(std::sync::Arc::new(fixture_proofs::FixtureVerifier));
    let events = runtime.events().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "integration.intent")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "integration.applied")
            .count(),
        usize::from(stage == "complete")
    );
    let observation: HostObservation =
        serde_json::from_value(layout["observation"].clone()).unwrap();
    let snapshot_authority = fixture_proofs::from_observation(
        observation,
        SnapshotValidation {
            now_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            max_age_seconds: 300,
        },
    )
    .unwrap();
    let router = PhaseRouter::stable_v2();
    let catalog = Catalog::from_json(include_bytes!(
        "../../../../fixtures/examples/phase-catalog.json"
    ))
    .unwrap();
    let context = router
        .context(
            serde_json::from_value(layout["phase_request"].clone()).unwrap(),
            &snapshot_authority,
            &catalog,
        )
        .unwrap();
    let pending = runtime
        .reopen(
            "run-1",
            &router,
            &context,
            serde_json::from_value(layout["request"].clone()).unwrap(),
            serde_json::from_value(layout["plan"].clone()).unwrap(),
            "execute",
        )
        .unwrap();
    // This existing status is about ref publication. A legacy in-place intent
    // cannot be promoted to published, including when bytes happen to match.
    assert_eq!(
        runtime.publication_status(&pending).await.unwrap().state,
        PatchPublicationState::Unknown
    );
    assert!(
        runtime
            .apply(&pending, &preview.approval_digest)
            .await
            .is_err()
    );
    assert_eq!(snapshot(&root), before);
    assert_eq!(runtime.events().unwrap(), events);
    // Only the test restores its own disposable preimage, to prove that the
    // consumed intent still refuses replay even after source checks pass.
    git(&root, &["reset", "--hard", &preview.source_state]);
    for name in ["src/z.txt", "user.txt"] {
        let path = root.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(source_state(&root).await.unwrap(), preview.source_state);
    let restored = snapshot(&root);
    assert!(matches!(
        runtime.apply(&pending, &preview.approval_digest).await,
        Err(RuntimeError::Integrity("integration already attempted"))
    ));
    assert_eq!(snapshot(&root), restored);
    assert_eq!(runtime.events().unwrap(), events);
    runtime.verify_integrity().unwrap();
}
#[tokio::test]
async fn owner_killed_before_effect_retains_intent_without_replay() {
    exercise("before").await;
}
#[tokio::test]
async fn owner_killed_after_writes_never_fabricates_terminal_evidence() {
    exercise("after").await;
}
#[tokio::test]
async fn real_git_partial_write_remains_uncertain_and_preserves_user_edits() {
    exercise("partial").await;
}
#[tokio::test]
async fn completed_application_survives_reopen_without_second_effect() {
    exercise("complete").await;
}
