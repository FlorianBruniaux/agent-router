//! Opt-in copied-client OS diagnostic. Never stores a capability proof or admits a product run.
use super::*;
use router_types::*;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const NATIVE: &str = "113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3";
fn pin(path: &Path) -> FiniteClientFile {
    let path = path.canonicalize().unwrap();
    let mut bytes = Vec::new();
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&path)
        .unwrap();
    assert!(file.metadata().unwrap().len() < 256 * 1024 * 1024);
    file.take(256 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .unwrap();
    FiniteClientFile {
        path: path.to_str().unwrap().into(),
        sha256: content_digest(&bytes),
    }
}
fn listener() -> TcpListener {
    TcpListener::bind(("127.0.0.1", 0)).unwrap()
}
fn serve(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
) -> thread::JoinHandle<()> {
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut s, _)) => {
                    count.fetch_add(1, Ordering::SeqCst);
                    s.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
                    let _ = s.write_all(b"F12-local-canary\n");
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(_) => break,
            }
        }
    })
}
fn reachable(port: u16) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut b = [0];
    s.read_exact(&mut b).unwrap();
    assert_eq!(b, b"F"[..]);
}
#[test]
#[ignore = "explicit Linux Docker diagnostic with pinned native/client/helper/wrapper and owned heartbeat volume; never part of automatic native batch"]
fn copied_client_os_qualification() {
    assert_eq!(
        std::env::var("AR_LINUX_DIAGNOSTIC").unwrap(),
        "owned-copies-v1"
    );
    let case = std::env::var("AR_LINUX_CASE").unwrap();
    assert!(["normal", "deadline", "eof", "helper-kill"].contains(&case.as_str()));
    let root = Path::new("/work/task");
    fs::DirBuilder::new().mode(0o700).create(root).unwrap();
    let work = root.join("work");
    fs::create_dir(&work).unwrap();
    let workspace = work.join("workspace");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .unwrap();
    let store = root.join("store");
    fs::DirBuilder::new().mode(0o700).create(&store).unwrap();
    let denied_read = root.join("denied-read");
    fs::write(&denied_read, b"inert-private-store-control").unwrap();
    fs::write(workspace.join("read-control"), b"F12-readable").unwrap();
    let allowed = workspace.join("patch.txt");
    fs::write(&allowed, b"before").unwrap();
    // Outside-policy positive controls: same UID can change a 0400 file on this writable tmpfs.
    let control = root.join("mode-control");
    fs::write(&control, b"before").unwrap();
    fs::set_permissions(&control, fs::Permissions::from_mode(0o400)).unwrap();
    fs::set_permissions(&control, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&control, b"after").unwrap();
    fs::remove_file(&control).unwrap();
    let broker = OwnedBroker::from_listener(listener()).unwrap();
    let broker_port = broker.listener.local_addr().unwrap().port();
    let other = listener();
    let other_port = other.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let broker_count = Arc::new(AtomicUsize::new(0));
    let other_count = Arc::new(AtomicUsize::new(0));
    let server = serve(
        broker.listener.try_clone().unwrap(),
        stop.clone(),
        broker_count.clone(),
    );
    let denied_server = serve(other, stop.clone(), other_count.clone());
    reachable(broker_port);
    reachable(other_port);
    let source = Path::new("/work/input");
    let native = pin(&source.join("native"));
    assert_eq!(native.sha256, NATIVE);
    let client = pin(&source.join("client"));
    let helper = pin(&source.join("helper"));
    let wrapper = pin(&source.join("wrapper"));
    let profile_path = root.join("profile.json");
    fs::write(&profile_path,b"{\"diagnostic\":\"landlock-abi6-seccomp-aarch64-v1\",\"network\":\"ipv4-tcp-exact-port\",\"address_space_bytes\":8589934592}\n").unwrap();
    fs::set_permissions(&profile_path, fs::Permissions::from_mode(0o400)).unwrap();
    let profile = pin(&profile_path);
    let run = PreparedRun {
        schema_version: 1,
        receipt_id: "a".repeat(64),
        source_state: "b".repeat(40),
        scope: vec!["patch.txt".into()],
        authorization_digest: "c".repeat(64),
        snapshot_hash: "d".repeat(64),
        host: HarnessId::parse("codex").unwrap(),
        requested_model: ModelId::parse("gpt-6.1-sol").unwrap(),
        requested_effort: EffortLevel::Low,
        program: "finite-client".into(),
        argv: vec!["--worker".into()],
        cwd: workspace.to_str().unwrap().into(),
        stdin: PreparedInput::new(b"Linux copied-client qualification only".to_vec()),
        mode: ExecutionMode::Execute,
        target: ExecutionTarget::ExternalWorker,
        sandbox: ExecutionSandbox::WorkspaceWrite,
        timeout_ms: 25000,
        max_output_bytes: 65536,
        degradations: vec![],
        missing_runtime_evidence: vec![],
        parent_model_changed: false,
    };
    let limits = ResourceLimits::default();
    assert_eq!(limits.address_space_bytes, Some(8589934592));
    let state = broker.prepare_codex_state(&store, &run).unwrap();
    let home = state.path().to_str().unwrap().to_string();
    // Codex 0.159.2 opens CODEX_HOME/installation_id read-write at startup.
    // Keep the config parent immutable: this fixture-only alias resolves to
    // mutable state in the existing private tmp grant, never to task input.
    let installation = state.path().join("tmp/installation_id");
    fs::write(&installation, b"00000000-0000-4000-8000-000000000012").unwrap();
    fs::set_permissions(&installation, fs::Permissions::from_mode(0o644)).unwrap();
    let installation_alias = state.path().join("installation_id");
    std::os::unix::fs::symlink("tmp/installation_id", &installation_alias).unwrap();

    let manifest = FiniteClientManifest {
        schema_version: 1,
        task_id: "linux-copied-client-diagnostic".into(),
        host_snapshot_id: "not-a-host-proof".into(),
        client: client.clone(),
        native: native.clone(),
        helper: helper.clone(),
        wrapper: wrapper.clone(),
        profile: profile.clone(),
        configuration: state.configuration().clone(),
        private_state: home.clone(),
        native_version: "0.159.2-linux-arm64-musl".into(),
        broker_endpoint: broker.endpoint().into(),
        broker_instance_sha256: broker.instance_sha256().into(),
    };
    let review =
        FiniteClientReview::with_codex_state(manifest, &run, broker, limits.clone(), state)
            .unwrap();
    let digest = review.approval_digest().to_owned();
    let approved = review.approve(&digest).unwrap(); // preparation-only, never Runtime::approve
    let r = &approved.reviewed;
    r.check_files().unwrap();
    let assets = r.owned_assets.as_ref().unwrap();
    let pins = &assets.pins;
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 60;
    let payload=serde_json::to_vec(&json!({"native":pins[1].path,"native_sha256":NATIVE,"home":home,"workspace":workspace,"config_sha256":pins[5].sha256,"broker_port":broker_port,"denied_port":other_port,"expires_at":expires,"linux_canary":{"assets":pins,"heartbeat":"/evidence/heartbeat","allowed_file":allowed,"denied_read":denied_read,"hold":case!="normal"}})).unwrap();
    assert!(payload.len() <= 8192);
    let payload_digest = content_digest(&payload);
    let lease = root.join("lease");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&lease)
        .unwrap()
        .write_all(b"router-runtime-lease-v1:pending\n")
        .unwrap();
    let mut command = Command::new(&pins[2].path);
    command.args(limits.args().unwrap()).args([
        "--supervise-v1",
        &pins[3].path,
        "--restrict",
        &broker_port.to_string(),
    ]);
    for p in pins {
        command.arg(Path::new(&p.path).parent().unwrap());
    }
    command.args([
        Path::new(&home).join("tmp"),
        allowed.clone(),
        "/evidence/heartbeat".into(),
    ]);
    command.args([&pins[0].path, "--worker", &payload_digest]);
    command
        .current_dir(&workspace)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", Path::new(&home).join("tmp"))
        .env("ROUTER_RUNTIME_SUPERVISION", "framed-v1")
        .env("ROUTER_RUNTIME_LEASE", &lease)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(root.join("worker-errors"))
                .unwrap(),
        ));
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(b"ARLIFE01").unwrap();
    input
        .write_all(&(payload.len() as u64).to_be_bytes())
        .unwrap();
    input.write_all(&25000u64.to_be_bytes()).unwrap();
    input.write_all(&10u64.to_be_bytes()).unwrap();
    input.write_all(&payload).unwrap();
    input.flush().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap().take(65537));
    let mut ready = String::new();
    assert!(output.read_line(&mut ready).unwrap() < 65536);
    assert!(
        ready.starts_with("AR_LINUX_READY:"),
        "copied worker did not reach ready: {ready}"
    );
    let canary: Value =
        serde_json::from_str(ready.trim().strip_prefix("AR_LINUX_READY:").unwrap()).unwrap();
    assert_eq!(canary["memory"], "8GiB");
    assert_eq!(canary["protected_assets"], 6);
    let input = if case == "eof" {
        drop(input);
        None
    } else if case == "helper-kill" {
        child.kill().unwrap();
        drop(input);
        None
    } else {
        Some(input)
    };
    let status = child.wait().unwrap();
    drop(input);
    if case == "normal" {
        assert!(status.success());
    } else if case == "deadline" {
        assert_eq!(status.code(), Some(124));
    } else if case == "eof" {
        assert_eq!(status.code(), Some(125));
    } else {
        assert!(!status.success());
    }
    let state = fs::read_to_string(&lease).unwrap();
    assert_eq!(
        state,
        if case == "helper-kill" {
            "router-runtime-lease-v1:active\n"
        } else {
            "router-runtime-lease-v1:group-clean\n"
        }
    );
    r.check_files().unwrap();
    assert_eq!(
        fs::read_link(&installation_alias).unwrap(),
        Path::new("tmp/installation_id")
    );
    assert_eq!(
        fs::read(&installation).unwrap(),
        b"00000000-0000-4000-8000-000000000012"
    );
    assert_eq!(fs::read(&allowed).unwrap(), b"scope-ok");
    assert_eq!(
        fs::read(&denied_read).unwrap(),
        b"inert-private-store-control"
    );
    assert_eq!(other_count.load(Ordering::SeqCst), 1);
    assert_eq!(broker_count.load(Ordering::SeqCst), 3);
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
    denied_server.join().unwrap();
    println!(
        "AR_LINUX_REPORT:{}",
        json!({"diagnostic_only":true,"f12a":"NO_GO","case":case,"address_space_bytes":8589934592u64,"preparation_digest":digest,"copy_hashes":pins.iter().map(|p|&p.sha256).collect::<Vec<_>>(),"canary":canary,"native_rpc":"initialize-and-thread-start-only","provider_turns":0,"other_listener_connections":other_count.load(Ordering::SeqCst),"broker_connections":broker_count.load(Ordering::SeqCst),"helper_exit_code":status.code(),"lease":state.trim(),"protected_identities_after":"unchanged","controller_still_owns_copies":true,"namespace_cleanup":"PENDING_EXTERNAL_OWNER"})
    );
    std::io::stdout().flush().unwrap();
    // Keep review/copies, state and listener alive until the external container owner
    // confirms namespace teardown. This bounded diagnostic never releases a run lease.
    thread::sleep(Duration::from_secs(30));
    panic!("external diagnostic owner did not terminate the namespace");
}
