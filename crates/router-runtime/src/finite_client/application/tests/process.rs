//! Real owner death and reopen; this unit fixture does not execute a native worker.
use super::*;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CHILD: &str = "finite_client::application::tests::process::owner_child";
const STORE: &str = "AR_FINITE_APPLICATION_TEST_STORE";

struct Cut {
    stage: String,
    marker: PathBuf,
}
impl Cut {
    fn stop(&self, stage: &str) -> Result<()> {
        if self.stage == stage {
            use std::os::unix::fs::OpenOptionsExt;
            let pending = self.marker.with_extension("pending");
            let mut marker = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&pending)?;
            marker.write_all(stage.as_bytes())?;
            marker.sync_all()?;
            // Publish only complete bytes, without replacing an existing cut.
            // The temporary fixture retains both links until owner cleanup.
            fs::hard_link(&pending, &self.marker)?;
            loop {
                std::thread::park();
            }
        }
        Ok(())
    }
}
impl workspace::CaptureObserver for Cut {
    fn before_capture(&self) -> Result<()> {
        self.stop("before")
    }
    fn after_capture(&self) -> Result<()> {
        self.stop("after")
    }
}

#[tokio::test]
async fn owner_child() {
    let Some(root) = std::env::var_os(STORE) else {
        return;
    };
    let root = PathBuf::from(root);
    let runtime = Runtime::open(&root, Default::default()).unwrap();
    let source = root.parent().unwrap().join("source");
    let revision = source_state(&source).await.unwrap();
    let h = handle(&runtime, &source, &revision);
    let review = h.review().await.unwrap();
    let cut = Cut {
        stage: std::env::var("AR_FINITE_APPLICATION_TEST_STAGE").unwrap(),
        marker: root.parent().unwrap().join("cut-ready"),
    };
    h.apply_observed(&review, &review.approval_digest, Some(&cut))
        .await
        .unwrap();
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn exercise(stage: &str) {
    let (_temp, runtime, source, revision) = fixture().await;
    let root = runtime.store.root.clone();
    let review = handle(&runtime, &source, &revision).review().await.unwrap();
    let index = workspace::git(&source, &["ls-files", "--stage"], vec![])
        .await
        .unwrap();
    drop(runtime);
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env(STORE, &root)
            .env("AR_FINITE_APPLICATION_TEST_STAGE", stage)
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let marker = root.parent().unwrap().join("cut-ready");
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(stage, "complete", "owner exited before cut: {status}");
            break status;
        }
        if stage != "complete" && marker.exists() {
            assert_eq!(fs::read(&marker).unwrap(), stage.as_bytes());
            child.0.kill().unwrap();
            break child.0.wait().unwrap();
        }
        assert!(Instant::now() < deadline, "owner checkpoint timed out");
        std::thread::sleep(Duration::from_millis(10));
    };
    if stage == "complete" {
        assert!(status.success());
    } else {
        assert_eq!(status.signal(), Some(9));
    }
    assert_eq!(
        fs::read(source.join("lib.rs")).unwrap(),
        if stage == "before" {
            b"before\n".as_slice()
        } else {
            b"after\n".as_slice()
        }
    );
    let reopened = Runtime::open(&root, Default::default()).unwrap();
    assert_eq!(
        reopened.summary("finite").unwrap().state,
        RunState::Verified
    );
    assert_eq!(applied(&reopened), usize::from(stage == "complete"));
    let events = serde_json::to_value(reopened.events().unwrap()).unwrap();
    assert_eq!(
        events
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "integration.intent")
            .count(),
        1
    );
    assert_eq!(
        events
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "verification.attested.v1")
            .count(),
        2
    );
    // A reconstructed test-only handle cannot reuse consumed approval, even
    // when the source is still the original preimage (the before-write case).
    let result = handle(&reopened, &source, &revision)
        .accept_and_apply(&review, &review.approval_digest)
        .await;
    assert!(matches!(result, Err(RuntimeError::Verification)));
    drop(reopened);
    fs::write(source.join("lib.rs"), b"later authored user edit\n").unwrap();
    fs::set_permissions(source.join("lib.rs"), fs::Permissions::from_mode(0o640)).unwrap();
    let reopened = Runtime::open(&root, Default::default()).unwrap();
    assert!(matches!(
        handle(&reopened, &source, &revision)
            .accept_and_apply(&review, &review.approval_digest)
            .await,
        Err(RuntimeError::Verification)
    ));
    assert_eq!(
        fs::read(source.join("lib.rs")).unwrap(),
        b"later authored user edit\n"
    );
    assert_eq!(
        fs::metadata(source.join("lib.rs")).unwrap().mode() & 0o777,
        0o640
    );
    assert_eq!(
        oid(workspace::git(&source, &["rev-parse", "HEAD"], vec![])
            .await
            .unwrap())
        .unwrap(),
        revision
    );
    assert_eq!(
        workspace::git(&source, &["ls-files", "--stage"], vec![])
            .await
            .unwrap(),
        index
    );
    assert_eq!(
        serde_json::to_value(reopened.events().unwrap()).unwrap(),
        events
    );
    reopened.verify_integrity().unwrap();
}

#[tokio::test]
async fn killed_before_write_reopens_without_replay() {
    exercise("before").await;
}
#[tokio::test]
async fn killed_after_sync_reopens_without_claiming_applied() {
    exercise("after").await;
}
#[tokio::test]
async fn completed_owner_reopens_without_second_effect() {
    exercise("complete").await;
}
