//! Real OS sandbox tests use only local fake version executables, never a model.
use router_adapter_api::content_digest;
use router_runtime::{
    Cancellation, DenySandbox, ResourceLimits, ResourceSupervisor, Result, RuntimeError,
    SandboxPolicy, SandboxSupervisor, probe_native_version,
};
use router_types::*;
use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn prepared(root: &Path) -> PreparedRun {
    PreparedRun {
        schema_version: 1,
        receipt_id: "a".repeat(64),
        source_state: "b".repeat(40),
        scope: vec!["src".into()],
        authorization_digest: "c".repeat(64),
        snapshot_hash: "d".repeat(64),
        host: HarnessId::parse("fixture-host").unwrap(),
        requested_model: ModelId::parse("fixture-model").unwrap(),
        requested_effort: EffortLevel::Medium,
        program: "fake-version-host".into(),
        argv: vec!["run".into(), "PRIVATE_PROMPT_ARGUMENT".into()],
        cwd: root.to_str().unwrap().into(),
        stdin: PreparedInput::new(b"PRIVATE_PROMPT_INPUT".to_vec()),
        mode: ExecutionMode::Execute,
        target: ExecutionTarget::ExternalWorker,
        sandbox: ExecutionSandbox::WorkspaceWrite,
        timeout_ms: 60000,
        max_output_bytes: 1024 * 1024,
        degradations: vec![],
        missing_runtime_evidence: vec![],
        parent_model_changed: false,
    }
}
fn private_dir() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    temp
}
struct InspectSupervisor {
    resources: bool,
    lifetime: bool,
    calls: AtomicUsize,
}
impl SandboxSupervisor for InspectSupervisor {
    fn resource_limits_enforced(&self) -> bool {
        self.resources
    }
    fn parent_lifetime_enforced(&self) -> bool {
        self.lifetime
    }
    fn command(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy,
    ) -> Result<tokio::process::Command> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(run.argv, ["--version"]);
        assert!(run.stdin.as_bytes().is_empty());
        assert!(run.scope.is_empty());
        assert!(!policy.network);
        assert_eq!(run.sandbox, ExecutionSandbox::ReadOnly);
        assert_eq!(run.timeout_ms, 5000);
        assert_eq!(run.max_output_bytes, 8192);
        assert!(fs::read_dir(&run.cwd).unwrap().next().is_none());
        assert_eq!(
            fs::metadata(&run.cwd).unwrap().permissions().mode() & 0o777,
            0o700
        );
        Err(RuntimeError::Invalid("inspected without launch"))
    }
}
#[tokio::test]
async fn version_probe_requires_resource_and_lifetime_supervision_before_creating_files() {
    for (resources, lifetime) in [(false, false), (true, false), (false, true)] {
        let temp = private_dir();
        let root = fs::canonicalize(temp.path()).unwrap();
        let supervisor = InspectSupervisor {
            resources,
            lifetime,
            calls: AtomicUsize::new(0),
        };
        assert!(matches!(
            probe_native_version(
                &supervisor,
                &prepared(&root),
                &root,
                &Cancellation::default()
            )
            .await,
            Err(RuntimeError::Sandbox)
        ));
        assert_eq!(supervisor.calls.load(Ordering::SeqCst), 0);
        assert!(fs::read_dir(&root).unwrap().next().is_none());
    }
    let temp = private_dir();
    let root = fs::canonicalize(temp.path()).unwrap();
    assert!(matches!(
        probe_native_version(
            &DenySandbox,
            &prepared(&root),
            &root,
            &Cancellation::default()
        )
        .await,
        Err(RuntimeError::Sandbox)
    ));
}
#[tokio::test]
async fn version_probe_replaces_prompt_arguments_scope_cwd_and_requested_limits() {
    let temp = private_dir();
    let root = fs::canonicalize(temp.path()).unwrap();
    let original = prepared(&root);
    let supervisor = InspectSupervisor {
        resources: true,
        lifetime: true,
        calls: AtomicUsize::new(0),
    };
    assert!(matches!(
        probe_native_version(&supervisor, &original, &root, &Cancellation::default()).await,
        Err(RuntimeError::Invalid("inspected without launch"))
    ));
    assert_eq!(supervisor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(original.stdin.as_bytes(), b"PRIVATE_PROMPT_INPUT");
    assert_eq!(original.argv[1], "PRIVATE_PROMPT_ARGUMENT");
    assert_eq!(original.cwd, root.to_str().unwrap());
}
#[tokio::test]
async fn version_probe_rejects_public_or_symlink_private_directory() {
    let temp = private_dir();
    let root = fs::canonicalize(temp.path()).unwrap();
    let private = root.join("private");
    fs::create_dir(&private).unwrap();
    let supervisor = InspectSupervisor {
        resources: true,
        lifetime: true,
        calls: AtomicUsize::new(0),
    };
    fs::set_permissions(&private, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        probe_native_version(
            &supervisor,
            &prepared(&root),
            &private,
            &Cancellation::default()
        )
        .await,
        Err(RuntimeError::Integrity("version probe directory"))
    ));
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&private, root.join("link")).unwrap();
    assert!(
        probe_native_version(
            &supervisor,
            &prepared(&root),
            &root.join("link"),
            &Cancellation::default()
        )
        .await
        .is_err()
    );
    assert_eq!(supervisor.calls.load(Ordering::SeqCst), 0);
}
struct UnprovedCleanup;
impl SandboxSupervisor for UnprovedCleanup {
    fn resource_limits_enforced(&self) -> bool {
        true
    }
    fn parent_lifetime_enforced(&self) -> bool {
        true
    }
    fn command(&self, run: &PreparedRun, _: &SandboxPolicy) -> Result<tokio::process::Command> {
        let mut c = tokio::process::Command::new("/bin/sh");
        // Deliberately faulty trusted integration, not a production sandbox.
        c.args(["-c","printf 'router-runtime-lease-v1:active\\n' > \"$ROUTER_RUNTIME_LEASE\"; printf '1.2.3\\n'"]).env_clear().current_dir(&run.cwd);
        Ok(c)
    }
}
#[tokio::test]
async fn version_bytes_without_cleanup_evidence_never_count_as_a_probe() {
    let temp = private_dir();
    let root = fs::canonicalize(temp.path()).unwrap();
    assert!(matches!(
        probe_native_version(
            &UnprovedCleanup,
            &prepared(&root),
            &root,
            &Cancellation::default()
        )
        .await,
        Err(RuntimeError::CleanupUnknown)
    ));
}

async fn native_supervisor(root: &Path, script: &str) -> Box<dyn SandboxSupervisor> {
    let program = root.join("fake-version-host");
    fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("router-sandbox-helper");
    #[cfg(target_os = "macos")]
    let wrapper = PathBuf::from("/usr/bin/sandbox-exec");
    #[cfg(target_os = "linux")]
    let wrapper = PathBuf::from("/usr/bin/bwrap");
    let hashes = [helper.clone(), program.clone(), wrapper.clone()]
        .into_iter()
        .map(|p| {
            let hash = content_digest(&fs::read(&p).unwrap());
            (p, hash)
        })
        .collect();
    #[cfg(target_os = "macos")]
    let sandbox = router_runtime::SeatbeltSandbox::probe_expected(
        root,
        BTreeMap::from([("fake-version-host".into(), program)]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .unwrap();
    #[cfg(target_os = "linux")]
    let sandbox = router_runtime::BubblewrapSandbox::probe_expected(
        root,
        &wrapper,
        BTreeMap::from([("fake-version-host".into(), program)]),
        vec![],
        BTreeMap::new(),
        &hashes,
    )
    .await
    .unwrap();
    Box::new(
        ResourceSupervisor::probe_expected(
            &helper,
            ResourceLimits {
                address_space_bytes: None,
                max_processes: 4096,
                cpu_seconds: 10,
                ..ResourceLimits::default()
            },
            sandbox,
            &hashes,
        )
        .await
        .unwrap(),
    )
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
#[tokio::test]
#[ignore = "requires native OS sandbox, netcat and built resource helper; no model"]
async fn native_version_probe_exposes_only_version_in_empty_readonly_offline_workspace() {
    let temp = private_dir();
    let root = fs::canonicalize(temp.path()).unwrap();
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("PRIVATE_CONFIG"), "secret").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let script = format!(
        r#"
[ "$#" -eq 1 ] && [ "$1" = '--version' ] || exit 21
if IFS= read -r input; then exit 22; fi
[ -z "$(/bin/ls -A .)" ] || exit 23
[ "$PWD" != {project} ] || exit 24
if (printf illegal > ./WRITE_DENIED) 2>/dev/null; then exit 25; fi
if (printf illegal > {project}/WRITE_DENIED) 2>/dev/null; then exit 26; fi
if /bin/cat {project}/PRIVATE_CONFIG >/dev/null 2>&1; then exit 27; fi
if /usr/bin/nc -z -w 1 127.0.0.1 {port} >/dev/null 2>&1; then exit 28; fi
printf '1.2.3\n'
"#,
        project = quote(&project)
    );
    let supervisor = native_supervisor(&root, &script).await;
    let result = probe_native_version(
        &*supervisor,
        &prepared(&project),
        &root,
        &Cancellation::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.stdout(), b"1.2.3\n");
    assert_eq!(result.output_sha256, content_digest(b"1.2.3\n"));
    assert_eq!(result.outcome.termination, TransportTermination::Exited);
    assert_eq!(result.outcome.exit_code, Some(0));
    #[cfg(target_os = "macos")]
    assert_eq!(result.cleanup, ProcessCleanupEvidence::GroupTerminated);
    #[cfg(target_os = "linux")]
    assert_eq!(result.cleanup, ProcessCleanupEvidence::NamespaceTerminated);
    assert!(!project.join("WRITE_DENIED").exists());
}

#[tokio::test]
#[ignore = "requires native OS sandbox and built resource helper; no model"]
async fn native_version_probe_rejects_timeout_output_overflow_stderr_and_cancellation() {
    for scenario in ["timeout", "overflow", "stderr", "cancel"] {
        let temp = private_dir();
        let root = fs::canonicalize(temp.path()).unwrap();
        let script = match scenario {
            "timeout" | "cancel" => "exec /bin/sleep 20",
            "overflow" => {
                "while :; do printf '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'; done"
            }
            _ => "printf 'unexpected diagnostic' >&2; printf '1.2.3\\n'",
        };
        let supervisor = native_supervisor(&root, script).await;
        let cancel = Cancellation::default();
        let start = Instant::now();
        let request = prepared(&root);
        let future = probe_native_version(&*supervisor, &request, &root, &cancel);
        let result = if scenario == "cancel" {
            let (result, ()) = tokio::join!(future, async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancel.cancel();
            });
            result
        } else {
            future.await
        };
        let error = result.err().expect("unsafe probe accepted");
        if scenario == "overflow" {
            assert!(matches!(error, RuntimeError::Limit), "{error}");
        } else if scenario == "cancel" {
            assert!(matches!(error, RuntimeError::Cancelled), "{error}");
        } else {
            assert!(
                matches!(error, RuntimeError::Process),
                "{scenario}: {error}"
            );
        }
        if scenario == "timeout" {
            assert!(start.elapsed() >= Duration::from_secs(5));
        }
        assert!(
            start.elapsed() < Duration::from_secs(9),
            "{scenario} was not bounded"
        );
    }
}
