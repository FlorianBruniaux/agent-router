// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Opt-in Linux native context component canary. All responses are authored.
use super::*;
use crate::finite_tools::{FiniteContextPolicy, exec_presentation_key, mcp_catalog_digest};
use std::collections::BTreeSet;

fn context_broker(
    listener: TcpListener,
    fixtures: PathBuf,
    evidence: PathBuf,
    expected: String,
) -> std::thread::JoinHandle<bool> {
    std::thread::spawn(move || {
        let mut child = ScriptChild(
            Command::new("/usr/bin/python3")
                .args(["-B", "-u"])
                .arg(fixtures.join("context_responses.py"))
                .arg(&fixtures)
                .arg(expected)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("PYTHONNOUSERSITE", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut input = child.0.stdin.take().unwrap();
        let mut output = BufReader::new(child.0.stdout.take().unwrap());
        listener.set_nonblocking(true).unwrap();
        let mut deadline = Instant::now() + Duration::from_secs(300);
        let mut requests = Vec::new();
        let mut accepted = true;
        let mut results = 0;
        while requests.len() < 6 {
            assert!(Instant::now() < deadline, "native context broker deadline");
            let (mut socket, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("accept {e}"),
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut first = String::new();
            if reader.read_line(&mut first).unwrap() == 0 {
                continue;
            }
            assert!(first.starts_with("POST /v1/responses "));
            let mut length = 0usize;
            let mut headers = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                headers += line.len();
                assert!(headers <= 16384 && !line.is_empty());
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                assert!(!lower.starts_with("authorization:"));
                if let Some(n) = lower.strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
            }
            assert!((1..=262144).contains(&length));
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request = router_adapter_api::strict_json_limited(&body, 262144).unwrap();
            fs::write(
                evidence.join(format!("request-{}.json", requests.len())),
                &body,
            )
            .unwrap();
            writeln!(input, "{}", serde_json::to_string(&request).unwrap()).unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            if let Some(catalog) = response["catalog_json"].as_str() {
                fs::write(evidence.join("observed-native-catalog.json"), catalog).unwrap();
                fs::write(
                    evidence.join("observed-native-catalog.sha256"),
                    response["catalog_digest"].as_str().unwrap(),
                )
                .unwrap();
            }
            requests.push(content_digest(&body));
            deadline = Instant::now() + Duration::from_secs(70);
            if response["refused"] != false {
                accepted = false;
                let text = "{\"error\":{\"message\":\"Authored context contract refused\",\"type\":\"invalid_request_error\"}}";
                write!(socket,"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",text.len(),text).unwrap();
                break;
            }
            results = response["tool_results"].as_u64().unwrap();
            let sse = response["sse"].as_str().unwrap();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",sse.len(),sse).unwrap();
        }
        fs::write(evidence.join("broker.json"),serde_json::to_vec_pretty(&json!({"requests":requests,"authored_only":true,"models_started":0,"auth_header_present":false,"contract_accepted":accepted,"observed_tool_results":results})).unwrap()).unwrap();
        accepted && results == 5
    })
}

#[tokio::test]
#[ignore = "requires Linux ARM64 isolated loopback, pinned native SDK, reviewed observed context catalogue, fixed 8GiB and no swap; no model"]
async fn native_context_windows_and_recall_without_model() {
    assert!(cfg!(target_os = "linux") && std::env::consts::ARCH == "aarch64");
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/memory.max")
            .unwrap()
            .trim(),
        "8589934592"
    );
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/memory.swap.max")
            .unwrap()
            .trim(),
        "0"
    );
    let root = PathBuf::from(std::env::var("AR_CONTEXT_NATIVE_ROOT").unwrap());
    mkdir(&root);
    let programs = PathBuf::from(std::env::var("AR_MCP_PROGRAMS").unwrap());
    assert_eq!(
        pin(&programs.join("native")).sha256,
        "113aa5d5952a6fd3950a88323219747d0c91978ebee948509a540c8a460e59a3"
    );
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/bench/rtk-context-native-qualification");
    let frozen =
        include_bytes!("../../../../docs/bench/rtk-context-native-qualification/fixtures.json");
    assert_eq!(
        fs::read(fixtures.join("fixtures.json")).unwrap(),
        frozen.as_slice()
    );
    let manifest: Value = serde_json::from_slice(frozen).unwrap();
    for row in manifest["files"].as_array().unwrap() {
        let data = fs::read(fixtures.join(row["path"].as_str().unwrap())).unwrap();
        assert_eq!(data.len() as u64, row["bytes"].as_u64().unwrap());
        assert_eq!(content_digest(&data), row["sha256"]);
    }
    let candidate = root.join("candidate");
    let store = root.join("state");
    let evidence = root.join("evidence");
    for path in [&candidate, &store, &evidence] {
        mkdir(path);
    }
    let mut public = BTreeMap::new();
    for name in ["source.txt", "feedback.py"] {
        let data = fs::read(fixtures.join(name)).unwrap();
        fs::write(candidate.join(name), &data).unwrap();
        fs::set_permissions(candidate.join(name), fs::Permissions::from_mode(0o600)).unwrap();
        public.insert(name.into(), content_digest(&data));
    }
    let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
    rustix::net::sockopt::set_socket_reuseaddr(&listener, false).unwrap();
    rustix::net::sockopt::set_socket_reuseport(&listener, false).unwrap();
    let port = listener.local_addr().unwrap().port();
    let owned = OwnedBroker::from_isolated_listener(listener.try_clone().unwrap()).unwrap();
    let policy = FiniteContextPolicy {
        schema_version: 1,
        read_windows: true,
        terminal_argument_sha256: BTreeSet::from([exec_presentation_key(
            &["python3".into(), "-B".into(), "feedback.py".into()],
            None,
        )
        .unwrap()]),
    };
    let run = PreparedRun {
        schema_version: 1,
        receipt_id: "a".repeat(64),
        source_state: "b".repeat(40),
        scope: vec!["out.txt".into()],
        authorization_digest: "c".repeat(64),
        snapshot_hash: "d".repeat(64),
        host: router_types::HarnessId::parse("codex").unwrap(),
        requested_model: router_types::ModelId::parse("gpt-6-luna").unwrap(),
        requested_effort: router_types::EffortLevel::Medium,
        program: "finite-client".into(),
        argv: vec!["--worker".into()],
        cwd: candidate.to_str().unwrap().into(),
        stdin: PreparedInput::new(
            b"RTK-CONTEXT-AUTHORED-V1\nFollow the authored local MCP dialogue only. No model.\n"
                .to_vec(),
        ),
        mode: router_types::ExecutionMode::Execute,
        target: router_types::ExecutionTarget::ExternalWorker,
        sandbox: router_types::ExecutionSandbox::WorkspaceWrite,
        timeout_ms: 60000,
        max_output_bytes: 65536,
        degradations: vec![],
        missing_runtime_evidence: vec![],
        parent_model_changed: false,
    };
    let state = owned
        .prepare_codex_context_tools_state(&store, &run, policy.clone())
        .unwrap();
    let home = state.path();
    let context = json!({"native":programs.join("native"),"native_sha256":pin(&programs.join("native")).sha256,"native_version":"0.159.2-linux-arm64-musl","home":home,"cwd":candidate,"configuration_sha256":state.configuration().sha256,"model":"gpt-6-luna","effort":"medium","stdin_sha256":content_digest(run.stdin.as_bytes()),"max_output_bytes":65536,"replacement":null});
    let binding = content_digest(b"authored-context-component-v1");
    let catalog_sha = mcp_catalog_digest(Some(&policy)).unwrap();
    let boot = ToolOwnerBoot {
        sandbox: ExecSandbox {
            namespace: pin(&programs.join("namespace")),
            helper: pin(&programs.join("helper")),
            guard: pin(&programs.join("guard")),
            compiler: pin(&programs.join("compiler/rustc")),
            root: candidate.clone(),
            scratch: "/router-tools/scratch".into(),
            control: "/router-tools/control".into(),
            public: public.clone(),
            output: "out.txt".into(),
        },
        limits: Limits {
            max_calls: 8,
            max_file_bytes: 32768,
            max_output_bytes: 8192,
            max_total_output_bytes: 32768,
            max_exec_ms: 20000,
            max_total_exec_ms: 60000,
            max_session_ms: 60000,
            max_scratch_entries: 64,
            max_scratch_bytes: 16777216,
        },
        preimage: None,
        binding: binding.clone(),
        client: pin(&programs.join("client")),
        home: home.to_str().unwrap().into(),
        broker_port: port,
        reads: ["client", "native", "helper", "guard", "namespace"]
            .map(|name| programs.join(name).to_str().unwrap().to_owned())
            .to_vec(),
        context: context.to_string(),
        probe: None,
        context_policy_sha256: Some(policy.digest().unwrap()),
        catalog_sha256: Some(catalog_sha),
        context_policy: Some(policy.clone()),
    };
    let boot_path = root.join("boot.json");
    fs::write(&boot_path, serde_json::to_vec(&boot).unwrap()).unwrap();
    fs::set_permissions(&boot_path, fs::Permissions::from_mode(0o400)).unwrap();
    let terminal = root.join("terminal.json");
    fs::write(&terminal, []).unwrap();
    fs::set_permissions(&terminal, fs::Permissions::from_mode(0o600)).unwrap();
    // Zero is a deliberate refusal/observation run, never automatic adoption.
    let expected =
        std::env::var("AR_CONTEXT_NATIVE_CATALOG_SHA256").unwrap_or_else(|_| "0".repeat(64));
    assert!(router_adapter_api::is_digest(&expected));
    let server = context_broker(listener, fixtures, evidence.clone(), expected);
    let (captured, cleanup) = capture_native_owner(
        &programs, &candidate, &boot_path, &terminal, &root, &run, state,
    )
    .await;
    fs::write(evidence.join("stdout.jsonl"), &captured.stdout).unwrap();
    fs::write(evidence.join("stderr.txt"), &captured.stderr).unwrap();
    let accepted = server.join().unwrap();
    fs::write(evidence.join("transport.json"),serde_json::to_vec_pretty(&json!({"exit_code":captured.outcome.exit_code,"termination":captured.outcome.termination,"limited":captured.output_limited,"incomplete":captured.output_incomplete,"memory_bytes":8589934592u64,"swap_bytes":0,"models_started":0,"runtime_approval":false,"cleanup":cleanup,"catalog_contract_accepted":accepted,"profile":"linux-isolated-finite-tools-context-v4"})).unwrap()).unwrap();
    assert!(
        accepted,
        "native context catalogue/result unreviewed; inspect observed bytes, never auto-update a pin"
    );
    assert_eq!(captured.outcome.exit_code, Some(0));
    assert_eq!(captured.outcome.termination, TransportTermination::Exited);
    assert!(!captured.output_limited && !captured.output_incomplete);
    let observed: ToolTerminal = serde_json::from_slice(&fs::read(&terminal).unwrap()).unwrap();
    observed.validate_context(Some(&policy)).unwrap();
    assert_eq!(observed.binding, binding);
    assert!(observed.complete && !observed.halted);
    assert_eq!(observed.journal.len(), 5);
    assert_eq!(
        observed.journal.iter().filter(|e| e.tool == "exec").count(),
        1
    );
    assert_eq!(
        observed
            .journal
            .iter()
            .filter(|e| e.tool == "read_output")
            .count(),
        1
    );
    assert_eq!(
        fs::read(candidate.join("out.txt")).unwrap(),
        b"authored-context-ok\n"
    );
    for (name, hash) in public {
        assert_eq!(
            content_digest(&fs::read(candidate.join(name)).unwrap()),
            hash
        );
    }
    fs::write(
        evidence.join("terminal.json"),
        serde_json::to_vec_pretty(&observed).unwrap(),
    )
    .unwrap();
}
