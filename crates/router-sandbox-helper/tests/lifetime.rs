//! Real SIGKILL regression: the outer test kills a separate supervisor process.
use rustix::fs::{FlockOperation, flock};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PENDING: &[u8] = b"router-runtime-lease-v1:pending\n";
fn pending(root: &Path) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join("lease"))
        .unwrap();
    file.write_all(PENDING).unwrap();
    file.sync_all().unwrap();
}
fn command(root: &Path, script: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_router-sandbox-helper"));
    command
        .args([
            "--cpu-seconds",
            "10",
            "--address-space-bytes",
            "none",
            "--max-open-files",
            "64",
            "--max-processes",
            "4096",
            "--max-file-bytes",
            "1048576",
            "--supervise-v1",
            "/bin/sh",
            "-c",
            script,
        ])
        .env("ROUTER_RUNTIME_SUPERVISION", "framed-v1")
        .env("ROUTER_RUNTIME_LEASE", root.join("lease"))
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}
fn frame(child: &mut Child, payload: &[u8]) -> std::process::ChildStdin {
    let mut input = child.stdin.take().unwrap();
    input.write_all(b"ARLIFE01").unwrap();
    for value in [payload.len() as u64, 30_000, 20] {
        input.write_all(&value.to_be_bytes()).unwrap();
    }
    input.write_all(payload).unwrap();
    input
}
fn await_file(path: &Path) {
    let start = Instant::now();
    while !fs::metadata(path).is_ok_and(|m| m.len() > 0) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "fixture did not start: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn bounded_wait(child: &mut Child) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("helper failed to exit");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn supervisor_process_fixture() {
    let Some(root) = std::env::var_os("ROUTER_LIFETIME_FIXTURE") else {
        return;
    };
    let root = Path::new(&root);
    let namespace = std::env::var_os("ROUTER_LIFETIME_NAMESPACE").is_some();
    let script = "trap '' TERM; /bin/sh -c 'trap \"\" TERM; while :; do printf x >> heartbeat; sleep 0.01; done' & wait";
    let mut launch = command(root, script);
    if namespace {
        let prefix: Vec<_> = launch
            .get_args()
            .take(11)
            .map(|value| value.to_owned())
            .collect();
        launch = Command::new(env!("CARGO_BIN_EXE_router-sandbox-helper"));
        launch.args(prefix).args(["/usr/bin/bwrap", "--die-with-parent", "--new-session", "--unshare-all", "--ro-bind", "/", "/", "--bind"])
            .arg(root).arg(root).args(["--proc", "/proc", "--dev", "/dev", "--chdir"]).arg(root)
            .args(["--", "/bin/sh", "-c", "setsid /bin/sh -c 'trap \"\" TERM; while :; do printf x >> heartbeat; sleep 0.01; done' & wait"])
            .env("ROUTER_RUNTIME_SUPERVISION", "framed-v1")
            .env("ROUTER_RUNTIME_LEASE", root.join("lease"))
            .env("ROUTER_RUNTIME_CONTAINMENT", "pid-namespace-v1")
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    }
    let mut child = launch.spawn().unwrap();
    let _life = frame(&mut child, b"");
    // An unrelated exec after pipe creation must not inherit its write end.
    let mut unrelated = Command::new("/bin/sleep").arg("2").spawn().unwrap();
    await_file(&root.join("heartbeat"));
    fs::write(root.join("supervisor-ready"), b"ready").unwrap();
    let _ = child.wait();
    let _ = unrelated.wait();
}

#[test]
fn sigkill_supervisor_stops_live_descendants_but_does_not_claim_exhaustive_cleanup() {
    sigkill_case(false);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires installed bubblewrap and working user namespaces; required by native CI"]
fn sigkill_bubblewrap_parent_stops_setsid_descendant() {
    sigkill_case(true);
}

fn sigkill_case(namespace: bool) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let mut launch = Command::new(std::env::current_exe().unwrap());
    if namespace {
        launch.env("ROUTER_LIFETIME_NAMESPACE", "1");
    }
    let mut supervisor = launch
        .args(["--exact", "supervisor_process_fixture", "--nocapture"])
        .env("ROUTER_LIFETIME_FIXTURE", &root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    await_file(&root.join("supervisor-ready"));
    let before = fs::metadata(root.join("heartbeat")).unwrap().len();
    let growth_start = Instant::now();
    while fs::metadata(root.join("heartbeat")).unwrap().len() <= before {
        if growth_start.elapsed() > Duration::from_secs(4) {
            let _ = supervisor.kill();
            let _ = supervisor.wait();
            panic!("live descendant heartbeat did not advance before SIGKILL");
        }
        thread::sleep(Duration::from_millis(10));
    }
    // Child::kill is SIGKILL on Unix; Drop cannot run in the victim process.
    supervisor.kill().unwrap();
    supervisor.wait().unwrap();
    let start = Instant::now();
    let lease = File::open(root.join("lease")).unwrap();
    loop {
        if flock(&lease, FlockOperation::NonBlockingLockExclusive).is_ok() {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "watchdog failed to finish bounded cleanup"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let stopped = fs::metadata(root.join("heartbeat")).unwrap().len();
    thread::sleep(Duration::from_millis(120));
    assert_eq!(
        fs::metadata(root.join("heartbeat")).unwrap().len(),
        stopped,
        "a descendant kept writing after watchdog cleanup"
    );
    assert_eq!(
        fs::read(root.join("lease")).unwrap(),
        if namespace {
            b"router-runtime-lease-v1:namespace-clean\n".as_slice()
        } else {
            b"router-runtime-lease-v1:group-clean\n".as_slice()
        },
        "group termination must not claim namespace containment"
    );
}

#[test]
fn eof_before_spawn_and_recovered_pending_never_launch_a_worker() {
    for recovered in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        pending(&root);
        if recovered {
            fs::write(root.join("lease"), b"router-runtime-lease-v1:recovered\n").unwrap();
        }
        let mut child = command(&root, "printf unsafe > worker-started")
            .spawn()
            .unwrap();
        if !recovered {
            let input = frame(&mut child, b"");
            drop(input);
        } else {
            drop(child.stdin.take());
        }
        assert!(!bounded_wait(&mut child).success());
        assert!(!root.join("worker-started").exists());
    }
}

#[test]
fn worker_receives_only_payload_and_no_control_environment() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let mut child = command(
        &root,
        "test -z \"$ROUTER_RUNTIME_SUPERVISION$ROUTER_RUNTIME_LEASE$ROUTER_RUNTIME_CONTAINMENT\" && cat > payload",
    )
    .spawn()
    .unwrap();
    let _life = frame(&mut child, b"payload, not protocol");
    assert!(bounded_wait(&mut child).success());
    assert_eq!(
        fs::read(root.join("payload")).unwrap(),
        b"payload, not protocol"
    );
}

#[test]
fn held_lease_prevents_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let lease = File::open(root.join("lease")).unwrap();
    flock(&lease, FlockOperation::LockExclusive).unwrap();
    let mut child = command(&root, "printf unsafe > worker-started")
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    assert!(!bounded_wait(&mut child).success());
    assert!(!root.join("worker-started").exists());
}

#[test]
fn normal_leader_exit_also_cleans_background_group_before_acknowledging() {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let mut child = command(&root, "(/bin/sh -c 'trap \"\" TERM; printf x > heartbeat; while :; do printf x >> heartbeat; sleep 0.01; done') & while test ! -s heartbeat; do :; done")
        .spawn().unwrap();
    let _life = frame(&mut child, b"");
    assert!(bounded_wait(&mut child).success());
    assert_eq!(
        fs::read(root.join("lease")).unwrap(),
        b"router-runtime-lease-v1:group-clean\n"
    );
    let stopped = fs::metadata(root.join("heartbeat")).unwrap().len();
    assert!(stopped > 0);
    thread::sleep(Duration::from_millis(120));
    assert_eq!(fs::metadata(root.join("heartbeat")).unwrap().len(), stopped);
}

// V2 application bytes travel through a socketpair, never through lifetime stdin.
fn interactive_command(
    root: &Path,
    script: &str,
) -> (
    Child,
    std::process::ChildStdin,
    std::os::unix::net::UnixStream,
) {
    use std::os::fd::OwnedFd;
    let (application, child_socket) = std::os::unix::net::UnixStream::pair().unwrap();
    application
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let original = command(root, script);
    let args: Vec<_> = original
        .get_args()
        .map(|arg| {
            if arg == "--supervise-v1" {
                "--supervise-v2".into()
            } else {
                arg.to_owned()
            }
        })
        .collect();
    let mut launch = Command::new(env!("CARGO_BIN_EXE_router-sandbox-helper"));
    launch
        .args(args)
        .env_clear()
        .env("ROUTER_RUNTIME_SUPERVISION", "framed-v2")
        .env("ROUTER_RUNTIME_LEASE", root.join("lease"))
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(OwnedFd::from(child_socket)))
        .stderr(Stdio::piped());
    let mut child = launch.spawn().unwrap();
    drop(launch);
    let mut life = child.stdin.take().unwrap();
    life.write_all(b"ARLIFE02").unwrap();
    for value in [0_u64, 30_000, 10] {
        life.write_all(&value.to_be_bytes()).unwrap();
    }
    (child, life, application)
}

#[test]
fn interactive_application_round_trip_does_not_end_parent_lifetime() {
    use std::io::Read;
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let (mut child, life, mut app) = interactive_command(
        &root,
        "test -z \"${ROUTER_RUNTIME_LEASE-}\" || exit 9; printf ready; read message; printf '%s' \"$message\"",
    );
    let mut ready = [0; 5];
    app.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"ready");
    app.write_all(b"ARLIFE01 is application data\n").unwrap();
    let mut reply = [0; 28];
    app.read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"ARLIFE01 is application data");
    assert!(bounded_wait(&mut child).success());
    assert_eq!(
        fs::read(root.join("lease")).unwrap(),
        b"router-runtime-lease-v1:group-clean\n"
    );
    drop(life);
}

#[test]
fn interactive_controller_cannot_keep_lifetime_alive_by_holding_duplex_fd() {
    use std::io::Read;
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    pending(&root);
    let (mut child, life, mut app) = interactive_command(
        &root,
        "printf ready; trap '' TERM; while :; do printf x >> heartbeat; sleep 0.01; done",
    );
    let mut ready = [0; 5];
    app.read_exact(&mut ready).unwrap();
    await_file(&root.join("heartbeat"));
    drop(life);
    assert_eq!(bounded_wait(&mut child).code(), Some(125));
    let stopped = fs::metadata(root.join("heartbeat")).unwrap().len();
    thread::sleep(Duration::from_millis(80));
    assert_eq!(fs::metadata(root.join("heartbeat")).unwrap().len(), stopped);
    assert_eq!(
        fs::read(root.join("lease")).unwrap(),
        b"router-runtime-lease-v1:group-clean\n"
    );
    let mut byte = [0; 1];
    assert_eq!(
        app.read(&mut byte).unwrap(),
        0,
        "controller's application descriptor must close"
    );
}
