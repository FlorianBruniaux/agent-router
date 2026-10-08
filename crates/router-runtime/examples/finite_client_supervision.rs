//! Explicit diagnostic candidate, never product admission or a task receipt.
//! The trusted diagnostic profile variation does not enable the runtime's
//! unsupported ControllerBrokerOnly authority. No model turn is submitted.
use router_runtime::{
    NetworkAuthority, ResourceLimits, ResourceSupervisor, Result, RuntimeError, SandboxPolicy,
    SandboxSupervisor, SeatbeltSandbox,
};
use router_types::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const NATIVE_PIN: &str = "16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704";
const MAX_INPUT: usize = 8192;
const MAX_OUTPUT: usize = 65536;
const BANNER: &[u8] = b"F12-local-canary\n";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    native: String,
    native_sha256: String,
    home: String,
    workspace: String,
    config_sha256: String,
    broker_port: u16,
    denied_port: u16,
    expires_at: u64,
}
fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeError::Invalid("diagnostic clock"))?
        .as_secs())
}
fn parse_task(bytes: &[u8], digest: &str, now: u64) -> Result<Task> {
    if bytes.len() > MAX_INPUT || router_adapter_api::content_digest(bytes) != digest {
        return Err(RuntimeError::Integrity("diagnostic task bytes"));
    }
    let value = router_adapter_api::strict_json_limited(bytes, MAX_INPUT)
        .map_err(|_| RuntimeError::Invalid("diagnostic JSON"))?;
    let task: Task = serde_json::from_value(value)?;
    if task.native_sha256 != NATIVE_PIN
        || !router_adapter_api::is_digest(&task.config_sha256)
        || task.broker_port == 0
        || task.denied_port == 0
        || task.broker_port == task.denied_port
        || task.expires_at <= now
        || task.expires_at - now > 60
    {
        return Err(RuntimeError::Invalid("diagnostic binding"));
    }
    Ok(task)
}
fn broker_rule(port: u16) -> Result<String> {
    if port == 0 {
        return Err(RuntimeError::Sandbox);
    }
    Ok(format!(
        "(allow network-outbound (remote tcp \"localhost:{port}\"))\n"
    ))
}
fn fingerprint(run: &PreparedRun) -> Result<String> {
    // PreparedRun's serialized shape omits stdin: bind its bytes separately.
    Ok(router_adapter_api::content_digest(&serde_json::to_vec(&(
        run,
        router_adapter_api::content_digest(run.stdin.as_bytes()),
    ))?))
}
struct DiagnosticBrokerSandbox {
    inner: SeatbeltSandbox,
    binding: String,
    port: u16,
}
impl SandboxSupervisor for DiagnosticBrokerSandbox {
    fn command(&self, run: &PreparedRun, policy: &SandboxPolicy) -> Result<Command> {
        if policy.network != NetworkAuthority::ControllerBrokerOnly
            || policy.host_state.is_some()
            || run.program != "finite-client"
            || fingerprint(run)? != self.binding
        {
            return Err(RuntimeError::Sandbox);
        }
        let inner = self.inner.command(
            run,
            &SandboxPolicy {
                network: NetworkAuthority::Deny,
                host_state: None,
            },
        )?;
        let source = inner.as_std();
        let mut args: Vec<_> = source.get_args().map(|s| s.to_os_string()).collect();
        if args.first().is_none_or(|s| s != "-p") || args.len() < 3 {
            return Err(RuntimeError::Sandbox);
        }
        let mut profile = args[1].to_str().ok_or(RuntimeError::Sandbox)?.to_owned();
        // Explicit candidate variation, not product authority or an approval.
        profile.push_str(&broker_rule(self.port)?);
        profile.push_str("(allow user-preference-read (preference-domain \"com.openai.codex\"))\n");
        profile
            .push_str("(allow ipc-posix-shm-read* (ipc-posix-name-prefix \"apple.cfprefs.\"))\n");
        args[1] = profile.into();
        let mut command = Command::new(source.get_program());
        command.args(args).env_clear();
        for (key, value) in source.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command.current_dir(&run.cwd);
        Ok(command)
    }
}
fn prepared(workspace: &Path, digest: &str, payload: Vec<u8>) -> PreparedRun {
    PreparedRun {
        schema_version: 1,
        receipt_id: "a".repeat(64),
        source_state: "b".repeat(40),
        scope: vec!["home".into()],
        authorization_digest: "c".repeat(64),
        snapshot_hash: "d".repeat(64),
        host: HarnessId::parse("codex").unwrap(),
        requested_model: ModelId::parse("gpt-6.1-sol").unwrap(),
        requested_effort: EffortLevel::Low,
        program: "finite-client".into(),
        argv: vec!["--worker".into(), digest.into()],
        cwd: workspace.to_str().unwrap().into(),
        stdin: PreparedInput::new(payload),
        mode: ExecutionMode::Execute,
        target: ExecutionTarget::ExternalWorker,
        sandbox: ExecutionSandbox::WorkspaceWrite,
        timeout_ms: 45000,
        max_output_bytes: MAX_OUTPUT,
        degradations: vec![],
        missing_runtime_evidence: vec![],
        parent_model_changed: false,
    }
}
fn hash(path: &Path) -> Result<String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 256 * 1024 * 1024 {
        return Err(RuntimeError::Limit);
    }
    let mut bytes = vec![];
    file.take(256 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(RuntimeError::Limit);
    }
    Ok(router_adapter_api::content_digest(&bytes))
}
fn canonical(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() || fs::canonicalize(&p)? != p {
        return Err(RuntimeError::Sandbox);
    }
    Ok(p)
}
fn socket_control(port: u16) -> Result<()> {
    let mut socket =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1))?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut banner = vec![0; BANNER.len()];
    socket.read_exact(&mut banner)?;
    if banner != BANNER {
        return Err(RuntimeError::Integrity("diagnostic broker canary"));
    }
    Ok(())
}
async fn rpc(
    input: &mut tokio::process::ChildStdin,
    output: &mut BufReader<tokio::io::Take<tokio::process::ChildStdout>>,
    request: Value,
) -> Result<Value> {
    let id = request["id"].clone();
    eprintln!("diagnostic-rpc-send-id: {id}");
    let mut bytes = serde_json::to_vec(&request)?;
    bytes.push(b'\n');
    input.write_all(&bytes).await?;
    input.flush().await?;
    for _ in 0..32 {
        let mut line = vec![];
        if output.read_until(b'\n', &mut line).await? == 0 || line.len() > MAX_OUTPUT {
            return Err(RuntimeError::Process);
        }
        let row = router_adapter_api::strict_json_limited(&line, MAX_OUTPUT)
            .map_err(|_| RuntimeError::Invalid("native JSON"))?;
        if row.get("method").is_some() {
            if row.get("id").is_some() || !row["method"].is_string() {
                return Err(RuntimeError::Sandbox);
            }
            continue;
        }
        if row["id"] != id || !row["result"].is_object() || row.get("error").is_some() {
            return Err(RuntimeError::Integrity("native RPC identity"));
        }
        eprintln!("diagnostic-rpc-reply-id: {id}");
        return Ok(row["result"].clone());
    }
    Err(RuntimeError::Limit)
}
fn verify_thread(reply: &Value, workspace: &str) -> Result<()> {
    if reply["model"] != "gpt-6.1-sol"
        || reply["reasoningEffort"] != "low"
        || reply["modelProvider"] != "local_fixture"
        || reply["cwd"] != workspace
        || reply["runtimeWorkspaceRoots"] != json!([workspace])
        || reply["approvalPolicy"] != "on-request"
        || reply["approvalsReviewer"] != "user"
        || reply["sandbox"] != json!({"type":"readOnly", "networkAccess":false})
        || reply["thread"]["ephemeral"] != true
        || !reply["thread"]["id"].is_string()
    {
        return Err(RuntimeError::Integrity("native thread binding"));
    }
    let identity = reply["thread"]["id"]
        .as_str()
        .ok_or(RuntimeError::Integrity("native thread identity"))?;
    if identity.is_empty()
        || identity.len() > 256
        || !identity.is_ascii()
        || identity.bytes().any(|b| b.is_ascii_control())
    {
        return Err(RuntimeError::Integrity("native thread identity"));
    }
    for (key, expected) in [
        ("model", "gpt-6.1-sol"),
        ("reasoningEffort", "low"),
        ("modelProvider", "local_fixture"),
        ("cwd", workspace),
    ] {
        if reply["thread"]
            .get(key)
            .is_some_and(|value| value != expected)
        {
            return Err(RuntimeError::Integrity("native thread contradiction"));
        }
    }
    Ok(())
}
async fn worker(digest: &str) -> Result<Value> {
    let started = std::time::Instant::now();
    let mut bytes = vec![];
    std::io::stdin()
        .take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)?;
    let task = parse_task(&bytes, digest, now()?)?;
    let native = canonical(&task.native)?;
    let home = canonical(&task.home)?;
    let workspace = canonical(&task.workspace)?;
    if hash(&native)? != task.native_sha256
        || hash(&home.join("config.toml"))? != task.config_sha256
    {
        return Err(RuntimeError::Integrity(
            "diagnostic executable or configuration",
        ));
    }
    eprintln!(
        "diagnostic-stage: broker-canary elapsed-ms={}",
        started.elapsed().as_millis()
    );
    socket_control(task.broker_port)?;
    let denied = TcpStream::connect_timeout(
        &([127, 0, 0, 1], task.denied_port).into(),
        Duration::from_secs(1),
    )
    .err()
    .and_then(|e| e.raw_os_error());
    if denied != Some(nix::libc::EPERM) {
        return Err(RuntimeError::Sandbox);
    }
    if fs::read(workspace.join("read-control"))? != b"F12-readable" {
        return Err(RuntimeError::Sandbox);
    }
    let denied_read = fs::read(
        workspace
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("denied-read"),
    );
    if denied_read
        .err()
        .is_none_or(|e| e.kind() != std::io::ErrorKind::PermissionDenied)
    {
        return Err(RuntimeError::Sandbox);
    }
    if fs::write(workspace.join("outside-scope"), b"no")
        .err()
        .is_none_or(|e| e.kind() != std::io::ErrorKind::PermissionDenied)
    {
        return Err(RuntimeError::Sandbox);
    }
    eprintln!(
        "diagnostic-stage: filesystem-canaries elapsed-ms={}",
        started.elapsed().as_millis()
    );
    fs::write(home.join("write-control"), b"F12-writable")?;
    eprintln!(
        "diagnostic-stage: native-spawn elapsed-ms={}",
        started.elapsed().as_millis()
    );
    let mut native_process = Command::new(&native)
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(&workspace)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("CODEX_HOME", &home)
        .env("TMPDIR", home.join("tmp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    fs::write(
        home.join("diagnostic-pids.json"),
        serde_json::to_vec(&json!({"client":std::process::id(),"native":native_process.id()}))?,
    )?;
    let mut input = native_process.stdin.take().ok_or(RuntimeError::Process)?;
    let mut output = BufReader::new(
        native_process
            .stdout
            .take()
            .ok_or(RuntimeError::Process)?
            .take(MAX_OUTPUT as u64 + 1),
    );
    let mut stderr = native_process
        .stderr
        .take()
        .ok_or(RuntimeError::Process)?
        .take(MAX_OUTPUT as u64 + 1);
    let private_errors = home.join("native-errors.private");
    let errors = tokio::spawn(async move {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(private_errors)?;
        let mut bytes = vec![];
        let mut buffer = [0; 4096];
        loop {
            let count = stderr.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])?;
            bytes.extend_from_slice(&buffer[..count]);
        }
        Ok::<_, std::io::Error>(bytes)
    });
    eprintln!(
        "diagnostic-stage: native-rpc elapsed-ms={}",
        started.elapsed().as_millis()
    );
    let exchanged = tokio::time::timeout(Duration::from_secs(6), async {
        rpc(&mut input,&mut output,json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"ar-finite-diagnostic","version":"1.0"},"capabilities":{"experimentalApi":true}}})).await?;
        input.write_all(b"{\"method\":\"initialized\"}\n").await?; input.flush().await?;
        let reply = rpc(&mut input,&mut output,json!({"id":2,"method":"thread/start","params":{
            "model":"gpt-6.1-sol","modelProvider":"local_fixture","cwd":task.workspace,
            "ephemeral":true,"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":"read-only"}})).await?;
        verify_thread(&reply, &task.workspace)
    }).await.map_err(|_| RuntimeError::Limit)?;
    // A failed exchange still closes input. The real outer helper owns group teardown.
    drop(input);
    let native_status =
        match tokio::time::timeout(Duration::from_secs(2), native_process.wait()).await {
            Ok(value) => value?,
            Err(_) => {
                native_process.kill().await?;
                native_process.wait().await?
            }
        };
    let errors = tokio::time::timeout(Duration::from_secs(2), errors)
        .await
        .map_err(|_| RuntimeError::CleanupUnknown)?
        .map_err(|_| RuntimeError::Process)??;
    exchanged?;
    if !native_status.success() {
        return Err(RuntimeError::Process);
    }
    if errors.len() > MAX_OUTPUT {
        return Err(RuntimeError::Limit);
    }
    if hash(&native)? != task.native_sha256
        || hash(&home.join("config.toml"))? != task.config_sha256
    {
        return Err(RuntimeError::Integrity("diagnostic pins changed"));
    }
    Ok(
        json!({"initialize":"observed","subsequent_thread_start":"observed_bound",
        "model":"gpt-6.1-sol","effort":"low","native_network_access":false,
        "rpc_request_ids":[1,2],"turn_submissions":0,"broker_tcp_canary":"observed",
        "other_listener_denied_errno":denied,"filesystem_canaries":"passed",
        "native_stderr_bytes":errors.len(),"native_exit_code":native_status.code(),"stdin_eof_before_native_spawn":true,
        "inherited_nproc_current":rustix::process::getrlimit(rustix::process::Resource::Nproc).current}),
    )
}
fn server(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut s, _)) => {
                    count.fetch_add(1, Ordering::SeqCst);
                    let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
                    let _ = s.write_all(BANNER);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(_) => break,
            }
        }
    })
}
async fn diagnostic() -> Result<Value> {
    if !cfg!(target_os = "macos") {
        return Err(RuntimeError::Sandbox);
    }
    let native = fs::canonicalize(
        std::env::var_os("AR_CODEX_NATIVE").ok_or(RuntimeError::Invalid("native path"))?,
    )?;
    let helper = fs::canonicalize(
        std::env::var_os("AR_SANDBOX_HELPER").ok_or(RuntimeError::Invalid("helper path"))?,
    )?;
    let root = PathBuf::from(
        std::env::var_os("AR_FINITE_PRIVATE").ok_or(RuntimeError::Invalid("private path"))?,
    );
    if !root.is_absolute() || hash(&native)? != NATIVE_PIN {
        return Err(RuntimeError::Integrity("native pin"));
    }
    fs::DirBuilder::new().mode(0o700).create(&root)?;
    let root = fs::canonicalize(root)?;
    fs::create_dir(root.join("worktrees"))?;
    let workspace = root.join("worktrees/finite");
    fs::create_dir(&workspace)?;
    let home = workspace.join("home");
    fs::DirBuilder::new().mode(0o700).create(&home)?;
    fs::create_dir(home.join("tmp"))?;
    let copy = root.join("native");
    fs::copy(&native, &copy)?;
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o500))?;
    fs::write(workspace.join("read-control"), b"F12-readable")?;
    fs::write(root.join("denied-read"), b"synthetic-no-access")?;
    let client = fs::canonicalize(std::env::current_exe()?)?;
    let client_hash = hash(&client)?;
    let helper_hash = hash(&helper)?;
    let broker = TcpListener::bind(("127.0.0.1", 0))?;
    let other = TcpListener::bind(("127.0.0.1", 0))?;
    let broker_port = broker.local_addr()?.port();
    let denied_port = other.local_addr()?.port();
    broker.set_nonblocking(true)?;
    other.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let bc = Arc::new(AtomicUsize::new(0));
    let dc = Arc::new(AtomicUsize::new(0));
    let bt = server(broker, stop.clone(), bc.clone());
    let dt = server(other, stop.clone(), dc.clone());
    let result = async {
        socket_control(broker_port)?; socket_control(denied_port)?;
        let config = format!("model = \"gpt-6.1-sol\"\nmodel_provider = \"local_fixture\"\nmodel_reasoning_effort = \"low\"\napproval_policy = \"on-request\"\napprovals_reviewer = \"user\"\nsandbox_mode = \"read-only\"\nweb_search = \"disabled\"\n[features]\nshell_tool = false\nunified_exec = false\napps = false\n[model_providers.local_fixture]\nname = \"F12 canary only, no model turn\"\nbase_url = \"http://127.0.0.1:{broker_port}/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\nrequest_max_retries = 0\nstream_max_retries = 0\n");
        fs::write(home.join("config.toml"),config.as_bytes())?;
        let task = Task { native:copy.to_str().unwrap().into(),native_sha256:NATIVE_PIN.into(),
            home:home.to_str().unwrap().into(),workspace:workspace.to_str().unwrap().into(),
            config_sha256:hash(&home.join("config.toml"))?,broker_port,denied_port,expires_at:now()?+60 };
        let payload = serde_json::to_vec(&task)?;
        let digest = router_adapter_api::content_digest(&payload);
        let run = prepared(&workspace,&digest,payload);
        let inner = SeatbeltSandbox::probe(&root,BTreeMap::from([("finite-client".into(),client.clone())]),
            vec![copy.clone()],BTreeMap::from([("HOME".into(),home.to_str().unwrap().into())])).await?;
        // Full address-space enforcement remains a product requirement, observed
        // unsupported previously. This explicit diagnostic does not request it.
        let supervisor = ResourceSupervisor::probe(&helper,ResourceLimits {
            address_space_bytes:None, max_processes:2048, ..ResourceLimits::default()
        },DiagnosticBrokerSandbox { inner,binding:fingerprint(&run)?,port:broker_port }).await?;
        let mut command = supervisor.command(&run,&SandboxPolicy {
            network:NetworkAuthority::ControllerBrokerOnly,host_state:None })?;
        let lease=root.join("client.lease");
        let mut f=fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&lease)?;
        f.write_all(b"router-runtime-lease-v1:pending\n")?;f.sync_all()?;
        command.env("ROUTER_RUNTIME_LEASE",&lease).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child=command.spawn()?;
        let mut life=child.stdin.take().ok_or(RuntimeError::Process)?;
        let payload=run.stdin.as_bytes();
        life.write_all(b"ARLIFE01").await?;
        for v in [payload.len() as u64,run.timeout_ms,20] {life.write_all(&v.to_be_bytes()).await?;}
        life.write_all(payload).await?;life.flush().await?;
        let mut stdout=child.stdout.take().ok_or(RuntimeError::Process)?.take(MAX_OUTPUT as u64+1);
        let mut stderr=child.stderr.take().ok_or(RuntimeError::Process)?.take(MAX_OUTPUT as u64+1);
        let out=tokio::spawn(async move {let mut b=vec![];stdout.read_to_end(&mut b).await.map(|_|b)});
        let err=tokio::spawn(async move {let mut b=vec![];stderr.read_to_end(&mut b).await.map(|_|b)});
        let status=match tokio::time::timeout(Duration::from_secs(49),child.wait()).await {
            Ok(value)=>value?,Err(_)=>{drop(life);let _=tokio::time::timeout(Duration::from_secs(3),child.wait()).await;
                return Err(RuntimeError::CleanupUnknown);}
        };
        let output=tokio::time::timeout(Duration::from_secs(2),out).await.map_err(|_|RuntimeError::CleanupUnknown)?
            .map_err(|_|RuntimeError::Process)??;
        let errors=tokio::time::timeout(Duration::from_secs(2),err).await.map_err(|_|RuntimeError::CleanupUnknown)?
            .map_err(|_|RuntimeError::Process)??;
        drop(life);
        if fs::read(&lease)?!=b"router-runtime-lease-v1:group-clean\n" {return Err(RuntimeError::CleanupUnknown);}
        if output.len()>MAX_OUTPUT||errors.len()>MAX_OUTPUT {return Err(RuntimeError::Limit);}
        // Private diagnostic output, never native raw text or account data.
        fs::write(root.join("worker.stdout"),&output)?;fs::write(root.join("worker.stderr"),&errors)?;
        fs::write(root.join("effects.json"),serde_json::to_vec(&json!({"helper_exit_code":status.code(),"broker_connections":bc.load(Ordering::SeqCst),"other_connections":dc.load(Ordering::SeqCst),"lease":"group-clean"}))?)?;
        if !status.success() {return Err(RuntimeError::Process);}
        let observed=router_adapter_api::strict_json_limited(&output,MAX_OUTPUT)
            .map_err(|_|RuntimeError::Invalid("worker output"))?;
        if hash(&native)?!=NATIVE_PIN||hash(&copy)?!=NATIVE_PIN||hash(&client)?!=client_hash||hash(&helper)?!=helper_hash {
            return Err(RuntimeError::Integrity("diagnostic pins changed"));}
        if bc.load(Ordering::SeqCst)!=2||dc.load(Ordering::SeqCst)!=1 {return Err(RuntimeError::Integrity("listener effects"));}
        Ok(json!({"schema_version":1,"evidence_kind":"native_finite_client_supervisor_candidate",
            "candidate":observed,"native_sha256":NATIVE_PIN,"client_sha256":client_hash,"helper_sha256":helper_hash,
            "task_payload_sha256":digest,"profile_scope":"trusted diagnostic TCP variation; not product admission",
            "listener_control_connections":[1,1],"candidate_broker_connections":1,"candidate_other_connections":0,
            "finite_supervision":"framed-v1 unchanged","helper_exit_code":status.code(),
            "lease":"group_clean_not_exhaustive_containment","memory_enforcement":"explicitly_not_requested",
            "diagnostic_max_processes":2048,"process_limit_scope":"per user, not per task",
            "mach_dns_boundary":"UNQUALIFIED","product_broker_authority":"UNSUPPORTED",
            "F12a":"NO_GO","F02b":"NOT_ATTEMPTED","turn_submissions":0,"paid_provider_calls":0}))
    }.await;
    stop.store(true, Ordering::SeqCst);
    bt.join().map_err(|_| RuntimeError::CleanupUnknown)?;
    dt.join().map_err(|_| RuntimeError::CleanupUnknown)?;
    if let Ok(report) = &result {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join("report.json"))?;
        serde_json::to_writer_pretty(&mut f, report)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    result
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = if args.len() == 3 && args[1] == "--worker" {
        worker(&args[2]).await
    } else if args.len() == 1 {
        diagnostic().await
    } else {
        Err(RuntimeError::Invalid("diagnostic arguments"))
    };
    match result {
        Ok(report) => println!("{}", report),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task() -> Task {
        Task {
            native: "/private/tmp/native".into(),
            native_sha256: NATIVE_PIN.into(),
            home: "/private/tmp/home".into(),
            workspace: "/private/tmp/workspace".into(),
            config_sha256: "a".repeat(64),
            broker_port: 12345,
            denied_port: 12346,
            expires_at: 110,
        }
    }
    fn parse(t: &Task, now: u64) -> Result<Task> {
        let bytes = serde_json::to_vec(t).unwrap();
        parse_task(&bytes, &router_adapter_api::content_digest(&bytes), now)
    }
    #[test]
    fn finite_task_accepts_exact_current_binding() {
        assert_eq!(parse(&task(), 100).unwrap().broker_port, 12345);
    }
    #[test]
    fn finite_task_refuses_changed_bytes_and_unknown_fields() {
        let bytes = serde_json::to_vec(&task()).unwrap();
        assert!(parse_task(&bytes, &"b".repeat(64), 100).is_err());
        let mut row = serde_json::to_value(task()).unwrap();
        row["extra_permission"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&row).unwrap();
        assert!(parse_task(&bytes, &router_adapter_api::content_digest(&bytes), 100).is_err());
    }
    #[test]
    fn finite_task_refuses_stale_future_wrong_pin_and_endpoint() {
        assert!(parse(&task(), 111).is_err());
        assert!(parse(&task(), 0).is_err());
        let mut t = task();
        t.native_sha256 = "b".repeat(64);
        assert!(parse(&t, 100).is_err());
        let mut t = task();
        t.broker_port = t.denied_port;
        assert!(parse(&t, 100).is_err());
        let mut t = task();
        t.broker_port = 0;
        assert!(parse(&t, 100).is_err());
    }
    #[test]
    fn finite_task_refuses_oversized_before_json() {
        assert!(parse_task(&vec![b' '; MAX_INPUT + 1], &"a".repeat(64), 100).is_err());
    }
    fn thread_reply() -> Value {
        json!({"model":"gpt-6.1-sol","reasoningEffort":"low","modelProvider":"local_fixture",
            "cwd":"/private/tmp/workspace","runtimeWorkspaceRoots":["/private/tmp/workspace"],
            "approvalPolicy":"on-request","approvalsReviewer":"user",
            "sandbox":{"type":"readOnly","networkAccess":false},
            "thread":{"id":"diagnostic-thread-id","ephemeral":true}})
    }
    #[test]
    fn thread_reply_accepts_exact_binding_and_refuses_permission_changes() {
        let row = thread_reply();
        verify_thread(&row, "/private/tmp/workspace").unwrap();
        for key in ["model", "cwd", "approvalPolicy", "approvalsReviewer"] {
            let mut row = thread_reply();
            row[key] = json!("changed");
            assert!(verify_thread(&row, "/private/tmp/workspace").is_err());
        }
        let mut row = thread_reply();
        row["sandbox"]["networkAccess"] = json!(true);
        assert!(verify_thread(&row, "/private/tmp/workspace").is_err());
    }
    #[test]
    fn thread_reply_refuses_unbounded_or_missing_identity() {
        for id in [json!(""), json!("x".repeat(257)), json!(null)] {
            let mut row = thread_reply();
            row["thread"]["id"] = id;
            assert!(verify_thread(&row, "/private/tmp/workspace").is_err());
        }
    }
    #[test]
    fn thread_reply_refuses_nested_metadata_contradiction() {
        for key in ["model", "reasoningEffort", "modelProvider", "cwd"] {
            let mut row = thread_reply();
            row["thread"][key] = json!("changed");
            assert!(verify_thread(&row, "/private/tmp/workspace").is_err());
        }
    }
    #[test]
    fn prepared_binding_covers_stdin_and_write_scope() {
        let run = prepared(
            Path::new("/private/tmp/workspace"),
            &"a".repeat(64),
            b"original".to_vec(),
        );
        let expected = fingerprint(&run).unwrap();
        let mut changed = run.clone();
        changed.stdin = PreparedInput::new(b"changed".to_vec());
        assert_ne!(expected, fingerprint(&changed).unwrap());
        let mut changed = run;
        changed.scope = vec!["other".into()];
        assert_ne!(expected, fingerprint(&changed).unwrap());
    }
    #[test]
    fn broker_rule_is_tcp_loopback_for_exact_port() {
        assert_eq!(
            broker_rule(12345).unwrap(),
            "(allow network-outbound (remote tcp \"localhost:12345\"))\n"
        );
        assert!(broker_rule(0).is_err());
    }
}
