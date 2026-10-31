// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::super::resume::FiniteResumeState;
use super::*;
struct Stop;
impl workspace::CaptureObserver for Stop {
    fn after_capture(&self) -> Result<()> {
        Ok(())
    }
    fn before_capture(&self) -> Result<()> {
        Err(RuntimeError::Cancelled)
    }
}
async fn interrupted() -> (tempfile::TempDir, Runtime, PathBuf) {
    let (t, r, s, rev) = fixture().await;
    let staged = workspace::git(&s, &["ls-files", "--stage"], vec![])
        .await
        .unwrap();
    workspace::git(&s, &["update-index", "--index-info"], staged)
        .await
        .unwrap();
    let h = handle(&r, &s, &rev);
    let v = h.review().await.unwrap();
    assert!(matches!(
        h.apply_observed(&v, &v.approval_digest, Some(&Stop)).await,
        Err(RuntimeError::Cancelled)
    ));
    let root = r.store.root.clone();
    drop(h);
    drop(r);
    let r = Runtime::open(&root, Default::default()).unwrap();
    (t, r, s)
}
#[tokio::test]
async fn fresh_reopened_approval_writes_once_and_preserves_original_history() {
    let (_t, r, s) = interrupted().await;
    let before = r.events().unwrap();
    let index = fs::read(s.join(".git/index")).unwrap();
    let old = r.finite_resume("finite").unwrap();
    let stale = old.review().await.unwrap();
    drop(old);
    let h = r.finite_resume("finite").unwrap();
    let v = h.review().await.unwrap();
    assert_eq!(v.state, FiniteResumeState::PreimageIntact);
    assert_eq!(fs::read(s.join(".git/index")).unwrap(), index);
    assert_ne!(v.approval_digest, stale.approval_digest);
    assert!(
        h.apply(&stale, stale.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert!(h.apply(&v, &v.original_approval).await.is_err());
    assert_eq!(r.events().unwrap(), before);
    h.apply(&v, v.approval_digest.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"after\n");
    assert_eq!(&r.events().unwrap()[..before.len()], &before);
    assert_eq!(
        h.review().await.unwrap().state,
        FiniteResumeState::Completed
    );
    assert!(
        h.apply(&v, v.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        r.events()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "integration.resume_applied.v1")
            .count(),
        1
    );
    assert_eq!(applied(&r), 0);
    assert_eq!(fs::read(s.join(".git/index")).unwrap(), index);
    r.verify_integrity().unwrap();
}
#[tokio::test]
async fn later_edits_replacements_modes_partial_candidate_and_git_changes_never_write() {
    for case in 0..11 {
        let (_t, r, s) = interrupted().await;
        let h = r.finite_resume("finite").unwrap();
        let v = h.review().await.unwrap();
        let path = s.join("lib.rs");
        match case {
            0 => fs::write(&path, b"later authored edit\n").unwrap(),
            1 => {
                fs::rename(&path, s.join("old")).unwrap();
                fs::write(&path, b"before\n").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            2 => fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap(),
            3 => fs::write(&path, b"aftore\n").unwrap(),
            4 => fs::write(&path, b"after\n").unwrap(),
            5 => {
                fs::write(&path, b"changed\n").unwrap();
                fs::write(&path, b"before\n").unwrap();
            }
            6 => {
                fs::hard_link(&path, s.join("other")).unwrap();
            }
            7 => {
                fs::rename(&path, s.join("old")).unwrap();
                std::os::unix::fs::symlink("old", &path).unwrap();
            }
            9 => {
                fs::write(
                    s.join(".git/config"),
                    b"[include]\npath=/private/not-authorized\n",
                )
                .unwrap();
            }
            10 => {
                fs::rename(s.join(".git"), s.join("metadata")).unwrap();
                std::os::unix::fs::symlink("metadata", s.join(".git")).unwrap();
            }
            _ => {
                fs::write(s.join("new.rs"), b"new\n").unwrap();
                workspace::git(&s, &["add", "new.rs"], vec![])
                    .await
                    .unwrap();
            }
        }
        let bytes = fs::read(&path).unwrap();
        let meta = fs::symlink_metadata(&path).unwrap().mode();
        let events = r.events().unwrap();
        assert!(
            h.apply(&v, v.approval_digest.as_ref().unwrap())
                .await
                .is_err(),
            "case {case}"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::symlink_metadata(&path).unwrap().mode(), meta);
        assert_eq!(r.events().unwrap(), events);
        if case == 4 {
            assert_eq!(
                h.review().await.unwrap().state,
                FiniteResumeState::CandidateObserved
            );
        }
    }
}
#[tokio::test]
async fn changed_history_refuses_stale_approval() {
    let (_t, r, s) = interrupted().await;
    let h = r.finite_resume("finite").unwrap();
    let v = h.review().await.unwrap();
    r.store.event("finite", "test.later", "later").unwrap();
    assert!(
        h.apply(&v, v.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
}
#[tokio::test]
async fn second_interruption_consumes_resume_without_replay() {
    let (_t, r, s) = interrupted().await;
    let h = r.finite_resume("finite").unwrap();
    let v = h.review().await.unwrap();
    assert!(matches!(
        h.apply_observed(&v, v.approval_digest.as_ref().unwrap(), Some(&Stop))
            .await,
        Err(RuntimeError::Cancelled)
    ));
    let root = r.store.root.clone();
    drop(h);
    drop(r);
    let r = Runtime::open(&root, Default::default()).unwrap();
    let h = r.finite_resume("finite").unwrap();
    let v = h.review().await.unwrap();
    assert_eq!(v.state, FiniteResumeState::Consumed);
    assert!(v.approval_digest.is_none());
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
}
#[tokio::test]
async fn legacy_intent_without_context_cannot_infer_a_source() {
    let (_t, r, s, rev) = fixture().await;
    let h = handle(&r, &s, &rev);
    let v = h.review().await.unwrap();
    r.store
        .record_verification_if_current(
            "finite",
            &VerificationRecord {
                verifier: "controller".into(),
                evidence_sha256: "e".repeat(64),
                artifact_sha256: Some(v.artifact_sha256.clone()),
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
            Some((RunState::Verified, None)),
            None,
        )
        .unwrap();
    r.store
        .claim_integration("finite", &serde_json::to_string(&v).unwrap())
        .unwrap();
    assert!(matches!(
        r.finite_resume("finite"),
        Err(RuntimeError::Integrity("missing original resume context"))
    ));
}

#[tokio::test]
async fn expired_handle_and_competing_handle_cannot_write() {
    let (_t, r, s) = interrupted().await;
    let mut expired = r.finite_resume("finite").unwrap();
    let stale = expired.review().await.unwrap();
    expired.expire();
    assert!(expired.review().await.unwrap().approval_digest.is_none());
    assert!(
        expired
            .apply(&stale, stale.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
    let a = r.finite_resume("finite").unwrap();
    let b = r.finite_resume("finite").unwrap();
    let av = a.review().await.unwrap();
    let bv = b.review().await.unwrap();
    a.apply(&av, av.approval_digest.as_ref().unwrap())
        .await
        .unwrap();
    let history = r.events().unwrap();
    assert!(
        b.apply(&bv, bv.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert_eq!(history, r.events().unwrap());
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"after\n");
}

#[tokio::test]
async fn resumed_display_is_bound_to_original_and_new_approval() {
    let (_t, r, s) = interrupted().await;
    let h = r.finite_resume("finite").unwrap();
    let packet = h.presentation().await.unwrap();
    assert_eq!(packet.diff.as_bytes(), PATCH);
    assert_eq!(
        packet.original.approval_digest,
        packet.review.original_approval
    );
    let before = r.events().unwrap();
    for variant in 0..3 {
        let mut bad = packet.clone();
        match variant {
            0 => bad.diff = bad.diff.replace("+after", "+other"),
            1 => bad.original.output = "other.rs".into(),
            2 => bad.review.history_sha256 = "0".repeat(64),
            _ => unreachable!(),
        }
        assert!(h.validate_presentation(&bad).await.is_err());
        assert!(
            h.apply_presented(&bad, packet.review.approval_digest.as_ref().unwrap())
                .await
                .is_err()
        );
        assert_eq!(r.events().unwrap(), before);
        assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
    }
    h.apply_presented(&packet, packet.review.approval_digest.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"after\n");
}

#[tokio::test]
async fn resumed_display_refuses_artifact_replacement_after_presentation() {
    let (_t, r, s) = interrupted().await;
    let h = r.finite_resume("finite").unwrap();
    let packet = h.presentation().await.unwrap();
    let before = r.events().unwrap();
    fs::write(r.store.root.join("artifacts/finite.patch"), b"replaced").unwrap();
    assert!(h.presentation().await.is_err());
    assert!(
        h.apply_presented(&packet, packet.review.approval_digest.as_ref().unwrap())
            .await
            .is_err()
    );
    assert_eq!(r.events().unwrap(), before);
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
}
