// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::*;
mod presentation;
mod process;
mod resume;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
};

const PATCH: &[u8] =
    b"diff --git a/lib.rs b/lib.rs\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-before\n+after\n";

async fn fixture() -> (tempfile::TempDir, Runtime, PathBuf, String) {
    let (temp, runtime) = super::super::verification::tests::fixture_artifact(true, Some(PATCH));
    let source = temp.path().canonicalize().unwrap().join("source");
    fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
    fs::write(source.join("lib.rs"), b"before\n").unwrap();
    fs::set_permissions(source.join("lib.rs"), fs::Permissions::from_mode(0o600)).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "lib.rs"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "--no-gpg-sign",
            "-m",
            "fixture",
        ],
    ] {
        workspace::git(&source, &args, vec![]).await.unwrap();
    }
    let revision = source_state(&source).await.unwrap();
    runtime
        .record_verification_for(
            "finite",
            "worker",
            true,
            VerificationRecord {
                verifier: "technical-checker".into(),
                evidence_sha256: "e".repeat(64),
                artifact_sha256: Some(content_digest(PATCH)),
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Unknown,
            },
        )
        .unwrap();
    (temp, runtime, source, revision)
}
fn handle<'a>(
    runtime: &'a Runtime,
    source: &std::path::Path,
    revision: &str,
) -> FiniteClientApplication<'a> {
    FiniteClientApplication {
        runtime,
        run_id: "finite".into(),
        definition: "a".repeat(64),
        execution_approval: "b".repeat(64),
        owner: "worker".into(),
        independent: true,
        source: source.into(),
        directory: directory(source.to_str().unwrap(), true).unwrap(),
        revision: revision.into(),
        relative: "lib.rs".into(),
        preimage: content_digest(b"before\n"),
    }
}
fn applied(runtime: &Runtime) -> usize {
    runtime
        .events()
        .unwrap()
        .iter()
        .filter(|e| e.kind == "integration.applied")
        .count()
}
#[tokio::test]
async fn exact_controller_approval_applies_private_candidate_once_and_retains_history() {
    let (_temp, runtime, source, revision) = fixture().await;
    let h = handle(&runtime, &source, &revision);
    let review = h.review().await.unwrap();
    assert_eq!(review.source_revision, revision);
    assert_ne!(review.source_tree, review.candidate_tree);
    assert_eq!(fs::read(source.join("lib.rs")).unwrap(), b"before\n");
    assert!(h.accept_and_apply(&review, &"0".repeat(64)).await.is_err());
    h.accept_and_apply(&review, &review.approval_digest)
        .await
        .unwrap();
    assert_eq!(fs::read(source.join("lib.rs")).unwrap(), b"after\n");
    assert_eq!(
        fs::metadata(source.join("lib.rs"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        oid(workspace::git(&source, &["rev-parse", "HEAD"], vec![])
            .await
            .unwrap())
        .unwrap(),
        revision
    );
    assert!(
        workspace::git(&source, &["diff", "--cached", "--exit-code"], vec![])
            .await
            .is_ok()
    );
    assert_eq!(
        runtime
            .events()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "verification.attested.v1")
            .count(),
        2
    );
    assert_eq!(applied(&runtime), 1);
    assert!(
        h.accept_and_apply(&review, &review.approval_digest)
            .await
            .is_err()
    );
    runtime.verify_integrity().unwrap();
}
#[tokio::test]
async fn stale_source_receipt_candidate_and_review_refuse_without_writing() {
    for variant in 0..6 {
        let (_temp, runtime, source, revision) = fixture().await;
        let h = handle(&runtime, &source, &revision);
        let mut review = h.review().await.unwrap();
        match variant {
            0 => {
                fs::write(source.join("lib.rs"), b"user edit\n").unwrap();
            }
            1 => {
                review.candidate_tree = "a".repeat(40);
            }
            2 => {
                workspace::git(
                    &source,
                    &[
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.invalid",
                        "commit",
                        "--allow-empty",
                        "--no-gpg-sign",
                        "-m",
                        "new identity",
                    ],
                    vec![],
                )
                .await
                .unwrap();
            }
            3 => {
                fs::set_permissions(source.join("lib.rs"), fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
            4 => {
                fs::remove_file(source.join("lib.rs")).unwrap();
                std::os::unix::fs::symlink("missing", source.join("lib.rs")).unwrap();
            }
            5 => {
                runtime
                    .record_verification_for(
                        "finite",
                        "worker",
                        true,
                        VerificationRecord {
                            verifier: "changed-checker".into(),
                            evidence_sha256: "e".repeat(64),
                            artifact_sha256: Some(content_digest(PATCH)),
                            validation: VerificationOutcome::Passed,
                            objective: VerificationOutcome::Unknown,
                        },
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            h.accept_and_apply(&review, &review.approval_digest)
                .await
                .is_err(),
            "variant {variant}"
        );
        assert_eq!(applied(&runtime), 0);
        assert_eq!(
            runtime.summary("finite").unwrap().state,
            RunState::Verifying
        );
    }
}
struct Interrupt(bool);
impl workspace::CaptureObserver for Interrupt {
    fn before_capture(&self) -> Result<()> {
        if !self.0 {
            Err(RuntimeError::Invalid("authored before-write interruption"))
        } else {
            Ok(())
        }
    }
    fn after_capture(&self) -> Result<()> {
        Err(RuntimeError::Invalid("authored after-write interruption"))
    }
}

struct ReplaceBeforeWrite {
    source: PathBuf,
    link: bool,
}
impl workspace::CaptureObserver for ReplaceBeforeWrite {
    fn before_capture(&self) -> Result<()> {
        if self.link {
            fs::remove_file(self.source.join("lib.rs"))?;
            std::os::unix::fs::symlink(self.source.join("outside"), self.source.join("lib.rs"))?;
        } else {
            fs::write(self.source.join("lib.rs"), b"concurrent user edit\n")?;
        }
        Ok(())
    }
    fn after_capture(&self) -> Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn source_edit_or_symlink_after_intent_is_not_overwritten() {
    for link in [false, true] {
        let (_temp, runtime, source, revision) = fixture().await;
        let h = handle(&runtime, &source, &revision);
        let review = h.review().await.unwrap();
        let observer = ReplaceBeforeWrite {
            source: source.clone(),
            link,
        };
        assert!(
            h.apply_observed(&review, &review.approval_digest, Some(&observer))
                .await
                .is_err()
        );
        if link {
            assert!(
                fs::symlink_metadata(source.join("lib.rs"))
                    .unwrap()
                    .is_symlink()
            );
            assert!(!source.join("outside").exists());
        } else {
            assert_eq!(
                fs::read(source.join("lib.rs")).unwrap(),
                b"concurrent user edit\n"
            );
        }
        assert_eq!(applied(&runtime), 0);
        assert_eq!(
            runtime
                .events()
                .unwrap()
                .iter()
                .filter(|e| e.kind == "integration.intent")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn changed_technical_record_cannot_be_overwritten_by_stale_atomic_acceptance() {
    let (_temp, runtime, _source, _revision) = fixture().await;
    let old = runtime.verification_record("finite").unwrap().unwrap();
    let mut changed = old.clone();
    changed.verifier = "new technical verifier".into();
    runtime
        .store
        .record_verification("finite", &changed, None)
        .unwrap();
    let mut accepted = old.clone();
    accepted.objective = VerificationOutcome::Passed;
    assert!(
        runtime
            .store
            .record_verification_if_current(
                "finite",
                &accepted,
                Some((RunState::Verified, None)),
                Some(&old)
            )
            .is_err()
    );
    assert_eq!(
        runtime.verification_record("finite").unwrap(),
        Some(changed)
    );
    assert_eq!(
        runtime.summary("finite").unwrap().state,
        RunState::Verifying
    );
}
#[tokio::test]
async fn interrupted_application_never_claims_applied_or_replays_after_reopen() {
    for after in [false, true] {
        let (_temp, runtime, source, revision) = fixture().await;
        let h = handle(&runtime, &source, &revision);
        let review = h.review().await.unwrap();
        assert!(
            h.apply_observed(&review, &review.approval_digest, Some(&Interrupt(after)))
                .await
                .is_err()
        );
        assert_eq!(
            fs::read(source.join("lib.rs")).unwrap(),
            if after {
                b"after\n".as_slice()
            } else {
                b"before\n".as_slice()
            }
        );
        assert_eq!(applied(&runtime), 0);
        assert_eq!(
            runtime.summary("finite").unwrap().state,
            RunState::Verified,
            "candidate accepted does not mean application completed"
        );
        let store = runtime.store.root.clone();
        drop(h);
        drop(runtime);
        fs::write(source.join("lib.rs"), b"later user edit\n").unwrap();
        let reopened = Runtime::open(&store, Default::default()).unwrap();
        let h = handle(&reopened, &source, &revision);
        assert!(
            h.accept_and_apply(&review, &review.approval_digest)
                .await
                .is_err()
        );
        assert_eq!(
            fs::read(source.join("lib.rs")).unwrap(),
            b"later user edit\n"
        );
        assert_eq!(applied(&reopened), 0);
    }
}
