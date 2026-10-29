// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Public disposable fixtures. No native model, provider or user source files.
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

const CHANGE: &str = "printf 'changed\\n' > src/base.txt; printf '#!/bin/sh\\nexit 0\\n' > src/tool.sh; chmod 755 src/tool.sh";

async fn ready(objective: VerificationOutcome) -> (Fixture, Runtime, PendingRun) {
    let fixture = Fixture::new(true).await;
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
                verifier: "authored application fixture".into(),
                evidence_sha256: "b".repeat(64),
                artifact_sha256: result.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective,
            },
        )
        .unwrap();
    (fixture, runtime, pending)
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

fn unchanged(fixture: &Fixture, runtime: &Runtime) {
    assert_eq!(
        fs::read(fixture.root.join("src/base.txt")).unwrap(),
        b"base\n"
    );
    assert!(!fixture.root.join("src/tool.sh").exists());
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind.starts_with("integration."))
    );
}

#[tokio::test]
async fn applies_the_exact_reviewed_revision_bytes_and_executable_mode() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
    let review = runtime.review_publication(&pending).await.unwrap();
    let preview = runtime.integration_preview(&pending).await.unwrap();
    assert_eq!(preview.artifact_sha256, review.artifact_sha256);
    assert_eq!(review.source_revision, fixture.request.source_state);
    assert_eq!(
        git(
            &fixture.root,
            &["rev-parse", &format!("{}^", review.result_revision)]
        ),
        format!("{}\n", review.source_revision).as_bytes()
    );
    let index = fs::read(fixture.root.join(".git/index")).unwrap();
    runtime
        .apply(&pending, &preview.approval_digest)
        .await
        .unwrap();
    for file in &review.files {
        let expected = git(
            &fixture.root,
            &["show", &format!("{}:{}", review.result_revision, file.path)],
        );
        assert_eq!(fs::read(fixture.root.join(&file.path)).unwrap(), expected);
        let executable = fs::metadata(fixture.root.join(&file.path))
            .unwrap()
            .permissions()
            .mode()
            & 0o111
            != 0;
        assert_eq!(executable, file.after.as_ref().unwrap().mode == "100755");
    }
    assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), index);
    assert_eq!(
        git(&fixture.root, &["rev-parse", "HEAD"]),
        format!("{}\n", review.source_revision).as_bytes()
    );
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
        1
    );
    assert!(!events.iter().any(|e| e.kind == "integration.published"));
}

#[tokio::test]
async fn a_different_commit_with_identical_source_bytes_refuses_the_old_approval() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
    let preview = runtime.integration_preview(&pending).await.unwrap();
    git(
        &fixture.root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-m",
            "new parent",
        ],
    );
    assert!(matches!(
        runtime.apply(&pending, &preview.approval_digest).await,
        Err(RuntimeError::SourceChanged)
    ));
    unchanged(&fixture, &runtime);
}

#[tokio::test]
async fn substituted_candidate_bytes_modes_and_paths_never_reach_application() {
    for change in ["bytes", "mode", "path"] {
        let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
        let preview = runtime.integration_preview(&pending).await.unwrap();
        let worktree = PathBuf::from(runtime.summary("run-1").unwrap().worktree.unwrap());
        match change {
            "bytes" => fs::write(worktree.join("src/base.txt"), b"substitution\n").unwrap(),
            "mode" => fs::set_permissions(
                worktree.join("src/tool.sh"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap(),
            "path" => fs::write(worktree.join("src/extra.txt"), b"extra\n").unwrap(),
            _ => unreachable!(),
        }
        assert!(
            runtime
                .apply(&pending, &preview.approval_digest)
                .await
                .is_err(),
            "{change}"
        );
        unchanged(&fixture, &runtime);
    }
}

#[tokio::test]
async fn replaced_source_symlink_does_not_write_its_external_target() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
    let preview = runtime.integration_preview(&pending).await.unwrap();
    let other = fixture.temp.path().join("public-external.txt");
    fs::write(&other, b"independent public fixture\n").unwrap();
    fs::remove_file(fixture.root.join("src/base.txt")).unwrap();
    symlink(&other, fixture.root.join("src/base.txt")).unwrap();
    assert!(
        runtime
            .apply(&pending, &preview.approval_digest)
            .await
            .is_err()
    );
    assert_eq!(fs::read(other).unwrap(), b"independent public fixture\n");
    assert!(
        fs::symlink_metadata(fixture.root.join("src/base.txt"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !runtime
            .events()
            .unwrap()
            .iter()
            .any(|e| e.kind.starts_with("integration."))
    );
}

#[tokio::test]
async fn concurrent_apply_attempts_consume_authority_once() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
    let preview = runtime.integration_preview(&pending).await.unwrap();
    let (first, second) = tokio::join!(
        runtime.apply(&pending, &preview.approval_digest),
        runtime.apply(&pending, &preview.approval_digest)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(
        fs::read(fixture.root.join("src/base.txt")).unwrap(),
        b"changed\n"
    );
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
        1
    );
}

#[tokio::test]
async fn unknown_objective_never_authorizes_application() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Unknown).await;
    assert!(matches!(
        runtime.integration_preview(&pending).await,
        Err(RuntimeError::Verification)
    ));
    assert!(matches!(
        runtime.apply(&pending, &"a".repeat(64)).await,
        Err(RuntimeError::Verification)
    ));
    assert_eq!(runtime.summary("run-1").unwrap().state, RunState::Verifying);
    unchanged(&fixture, &runtime);
}

#[tokio::test]
async fn verified_attestation_cannot_be_replaced_after_review() {
    let (fixture, runtime, pending) = ready(VerificationOutcome::Passed).await;
    let preview = runtime.integration_preview(&pending).await.unwrap();
    let before = runtime.verification_record("run-1").unwrap().unwrap();
    let events = runtime.events().unwrap();
    let mut substitution = before.clone();
    substitution.evidence_sha256 = "c".repeat(64);
    assert!(matches!(
        runtime.record_verification(&pending, substitution),
        Err(RuntimeError::Verification)
    ));
    assert_eq!(runtime.verification_record("run-1").unwrap(), Some(before));
    assert_eq!(runtime.events().unwrap(), events);
    assert_eq!(
        runtime.integration_preview(&pending).await.unwrap(),
        preview
    );
    unchanged(&fixture, &runtime);
}

#[tokio::test]
async fn useful_fix_passes_the_same_checks_on_candidate_revision_and_applied_files() {
    let fixture = Fixture::new_at_with_task(true, tempfile::tempdir().unwrap(), true).await;
    assert!(!clamp_acceptance(
        &fs::read_to_string(fixture.root.join("src/clamp.py")).unwrap()
    ));
    let untouched = fs::read(fixture.root.join("src/base.txt")).unwrap();
    let index = fs::read(fixture.root.join(".git/index")).unwrap();
    let runtime = fixture.runtime();
    let pending = fixture.review(&runtime).await;
    let approved = runtime
        .approve(pending.clone(), pending.approval_digest())
        .unwrap();
    let result = runtime
        .execute(
            approved,
            &FakeAdapter("printf 'def clamp(value):\\n    return max(value, 0)\\n' > src/clamp.py"),
            &FakeSupervisor,
            Cancellation::default(),
        )
        .await
        .unwrap();
    let candidate =
        fs::read_to_string(Path::new(result.run.worktree.as_ref().unwrap()).join("src/clamp.py"))
            .unwrap();
    assert!(clamp_acceptance(&candidate));
    runtime
        .record_verification(
            &pending,
            VerificationRecord {
                verifier: "authored useful application fixture".into(),
                evidence_sha256: router_adapter_api::content_digest(
                    b"clamp(-5)=0;clamp(0)=0;clamp(5)=5",
                ),
                artifact_sha256: result.run.artifact_sha256,
                validation: VerificationOutcome::Passed,
                objective: VerificationOutcome::Passed,
            },
        )
        .unwrap();
    let review = runtime.review_publication(&pending).await.unwrap();
    assert_eq!(
        review
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/clamp.py"]
    );
    assert_eq!(
        git(
            &fixture.root,
            &["rev-parse", &format!("{}^", review.result_revision)]
        ),
        format!("{}\n", fixture.request.source_state).as_bytes()
    );
    let reviewed = git(
        &fixture.root,
        &["show", &format!("{}:src/clamp.py", review.result_revision)],
    );
    assert_eq!(reviewed, candidate.as_bytes());
    assert!(clamp_acceptance(std::str::from_utf8(&reviewed).unwrap()));
    let preview = runtime.integration_preview(&pending).await.unwrap();
    assert_eq!(preview.artifact_sha256, review.artifact_sha256);
    runtime
        .apply(&pending, &preview.approval_digest)
        .await
        .unwrap();
    let applied = fs::read(fixture.root.join("src/clamp.py")).unwrap();
    assert_eq!(applied, reviewed);
    assert!(clamp_acceptance(std::str::from_utf8(&applied).unwrap()));
    assert_eq!(
        fs::read(fixture.root.join("src/base.txt")).unwrap(),
        untouched
    );
    assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), index);
    assert_eq!(
        git(&fixture.root, &["rev-parse", "HEAD"]),
        format!("{}\n", fixture.request.source_state).as_bytes()
    );
    let events = runtime.events().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "integration.applied")
            .count(),
        1
    );
    assert!(!events.iter().any(|e| e.kind == "integration.published"));
    assert!(git(&fixture.root, &["for-each-ref", "refs/agent-router/"]).is_empty());
}
