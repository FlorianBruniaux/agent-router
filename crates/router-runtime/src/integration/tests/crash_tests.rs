// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{Child, ExitStatus, Stdio},
    time::{Duration, Instant},
};

const CHILD_TEST: &str = "integration::tests::crash_tests::publication_process_child";
const FIXTURE_ENV: &str = "AR_PUBLICATION_CRASH_FIXTURE";
const STAGE_ENV: &str = "AR_PUBLICATION_CRASH_STAGE";

// This owns only the test child. Failure or timeout cannot leave it parked.
struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().expect("owned fixture child")
    }
    fn wait_ready(&mut self, marker: &Path, stage: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if marker.exists() {
                assert_eq!(fs::read_to_string(marker).unwrap(), stage);
                return;
            }
            assert!(
                self.child().try_wait().unwrap().is_none(),
                "child exited before {stage}"
            );
            assert!(Instant::now() < deadline, "child did not reach {stage}");
            // Polling is only a deadline mechanism; the marker and ref state
            // below, rather than elapsed time, prove the publication boundary.
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn finish(&mut self, kill: bool) -> ExitStatus {
        if kill {
            self.child().kill().unwrap(); // SIGKILL of this exact owned child.
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child().try_wait().unwrap() {
                self.0.take(); // Reaped; Drop must not signal it again.
                return status;
            }
            assert!(Instant::now() < deadline, "child did not terminate");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct Pause {
    stage: String,
    marker: PathBuf,
    before: AtomicUsize,
    after: AtomicUsize,
}
impl Pause {
    fn park(&self) {
        let pending = self.marker.with_extension("pending");
        let mut marker = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .unwrap();
        marker.write_all(self.stage.as_bytes()).unwrap();
        marker.sync_all().unwrap();
        drop(marker);
        fs::rename(pending, &self.marker).unwrap();
        loop {
            std::thread::park();
        }
    }
}
impl workspace::CaptureObserver for Pause {
    fn before_capture(&self) -> Result<()> {
        // git_observed first captures config, then the update-ref command.
        if self.stage == "before" && self.before.fetch_add(1, Ordering::SeqCst) == 1 {
            self.park();
        }
        Ok(())
    }
    fn after_capture(&self) -> Result<()> {
        if self.stage == "after" && self.after.fetch_add(1, Ordering::SeqCst) == 1 {
            self.park();
        }
        Ok(())
    }
}

#[tokio::test]
async fn publication_process_child() {
    let Some(path) = std::env::var_os(FIXTURE_ENV) else {
        return; // Only the parent test enables this subprocess fixture.
    };
    let path = PathBuf::from(path);
    let stage = std::env::var(STAGE_ENV).unwrap();
    assert!(["before", "after", "complete"].contains(&stage.as_str()));
    let receipt: PatchPublicationReceipt =
        serde_json::from_slice(&fs::read(path.join("receipt.json")).unwrap()).unwrap();
    let runtime = Runtime::open(&path.join("runtime"), RuntimeLimits::default()).unwrap();
    let pause = Pause {
        stage,
        marker: path.join("ready"),
        before: AtomicUsize::new(0),
        after: AtomicUsize::new(0),
    };
    complete_publication(&runtime, &path.join("repo"), &receipt, Some(&pause))
        .await
        .unwrap();
}

fn spawn_child(path: &Path, stage: &str) -> OwnedChild {
    fs::create_dir(path.join("home")).unwrap();
    fs::create_dir(path.join("tmp")).unwrap();
    OwnedChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .current_dir(path)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", path.join("home"))
            .env("TMPDIR", path.join("tmp"))
            .env(FIXTURE_ENV, path)
            .env(STAGE_ENV, stage)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ))
}

async fn exercise(stage: &str) {
    let (temp, root, receipt) = fixture();
    let path = fs::canonicalize(temp.path()).unwrap();
    let storage = path.join("runtime");
    fs::write(
        path.join("receipt.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    let initial_events = {
        let owner = runtime(&storage, &receipt);
        owner.events().unwrap()
    }; // Release the parent store before starting the separate owner process.
    let mut child = spawn_child(&path, stage);
    if stage != "complete" {
        child.wait_ready(&path.join("ready"), stage);
    }
    // Concurrent user edits include both staged and unstaged data. These are
    // confined to the disposable repository, never a user's real checkout.
    fs::write(root.join("first"), "unstaged user edit").unwrap();
    fs::write(root.join("second"), "staged user edit").unwrap();
    git(&root, &["add", "second"]);
    fs::write(root.join("untracked"), "untracked user edit").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let head = fs::read(root.join(".git/HEAD")).unwrap();
    let source = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(source, receipt.source_revision);

    let status = child.finish(stage != "complete");
    if stage == "complete" {
        assert!(status.success(), "positive control failed: {status}");
    } else {
        assert_eq!(status.signal(), Some(9), "owned child was not SIGKILLed");
    }
    // Opening only after wait/reap proves OS lock release rather than Drop.
    let reopened = Runtime::open(&storage, RuntimeLimits::default()).unwrap();
    let before = reopened.events().unwrap();
    assert_eq!(&before[..initial_events.len()], initial_events);
    assert_eq!(
        before
            .iter()
            .filter(|e| e.kind == "integration.intent")
            .count(),
        1
    );
    assert_eq!(
        before
            .iter()
            .filter(|e| e.kind == "integration.published")
            .count(),
        usize::from(stage == "complete")
    );
    let observed = observe_publication(&reopened, &receipt.run_id, &source, &root)
        .await
        .unwrap();
    assert_eq!(
        observed.state,
        if stage == "complete" {
            PatchPublicationState::Published
        } else {
            PatchPublicationState::Unknown
        }
    );
    assert_eq!(observed.receipt.as_ref(), Some(&receipt));
    assert_eq!(
        observed.observed_revision,
        (stage != "before").then(|| receipt.result_revision.clone())
    );
    assert!(matches!(
        reopened
            .store
            .claim_integration(&receipt.run_id, &serde_json::to_string(&receipt).unwrap()),
        Err(RuntimeError::Integrity("integration already attempted"))
    ));
    assert_eq!(
        reopened.events().unwrap(),
        before,
        "observation or refused replay changed history"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/HEAD")).unwrap(), head);
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), source);
    assert_eq!(
        fs::read_to_string(root.join("first")).unwrap(),
        "unstaged user edit"
    );
    assert_eq!(
        fs::read_to_string(root.join("second")).unwrap(),
        "staged user edit"
    );
    assert_eq!(
        fs::read_to_string(root.join("untracked")).unwrap(),
        "untracked user edit"
    );
    if stage != "before" {
        assert_eq!(
            git(&root, &["show", &format!("{}:first", receipt.target_ref)]),
            "after"
        );
        assert_eq!(
            git(&root, &["show", &format!("{}:second", receipt.target_ref)]),
            "after"
        );
    }
}

#[tokio::test]
async fn killed_publication_owner_preserves_unknown_and_user_edits() {
    for stage in ["before", "after"] {
        exercise(stage).await;
    }
}

#[tokio::test]
async fn completed_publication_remains_published_after_process_restart() {
    exercise("complete").await;
}
