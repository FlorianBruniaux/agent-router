use super::*;
use router_adapter_api::content_digest;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
};
fn policy() -> FiniteContextPolicy {
    FiniteContextPolicy {
        schema_version: 1,
        read_windows: true,
        terminal_argument_sha256: BTreeSet::from([exec_presentation_key(
            &["python3".into()],
            None,
        )
        .unwrap()]),
    }
}
fn limits() -> Limits {
    Limits {
        max_calls: 8,
        max_file_bytes: 32768,
        max_output_bytes: 8192,
        max_total_output_bytes: 32768,
        max_exec_ms: 20000,
        max_total_exec_ms: 60000,
        max_session_ms: 90000,
        max_scratch_entries: 64,
        max_scratch_bytes: 16777216,
    }
}
fn boot() -> ToolOwnerBoot {
    let pin = json!({"path":"/not-invoked","sha256":"a".repeat(64)});
    serde_json::from_value(json!({"sandbox":{"namespace":pin,"helper":pin,"guard":pin,"compiler":pin,"root":"/not-invoked/root","scratch":"/router-tools/scratch","control":"/router-tools/control","public":{"input.txt":"b".repeat(64)},"output":"out.txt"},"limits":limits(),"preimage":null,"binding":"c".repeat(64),"client":pin,"home":"/not-invoked/home","broker_port":12345,"reads":["/one","/two","/three","/four","/five"],"context":"{}","probe":null})).unwrap()
}
fn contextual_boot() -> ToolOwnerBoot {
    let mut boot = boot();
    let policy = policy();
    boot.context_policy_sha256 = Some(policy.digest().unwrap());
    boot.catalog_sha256 = Some(mcp_catalog_digest(Some(&policy)).unwrap());
    boot.context_policy = Some(policy);
    boot
}
#[test]
fn altered_context_bootstrap_refuses_dispatch() {
    let boot = contextual_boot();
    assert!(boot.validate_context().is_ok());
    let mut bad = boot.clone();
    bad.context_policy.as_mut().unwrap().read_windows = false;
    assert_eq!(bad.validate_context(), Err("owner.context_binding"));
    let mut bad = boot.clone();
    bad.context_policy
        .as_mut()
        .unwrap()
        .terminal_argument_sha256
        .clear();
    assert!(bad.validate_context().is_err());
    let mut bad = boot.clone();
    bad.context_policy_sha256 = None;
    assert!(bad.validate_context().is_err());
    let mut bad = boot.clone();
    bad.context_policy = None;
    assert!(bad.validate_context().is_err());
    // Identity is sealed by the parent's retained bootstrap hash, even if an
    // attacker changes all descriptive digests to be internally consistent.
    let expected = content_digest(&serde_json::to_vec(&boot).unwrap());
    let mut bad = boot;
    bad.context_policy = Some(FiniteContextPolicy {
        terminal_argument_sha256: BTreeSet::new(),
        ..policy()
    });
    bad.context_policy_sha256 = Some(bad.context_policy.as_ref().unwrap().digest().unwrap());
    bad.catalog_sha256 = Some(mcp_catalog_digest(bad.context_policy.as_ref()).unwrap());
    assert!(
        owner::validate_bootstrap_bytes(&serde_json::to_vec(&bad).unwrap(), &expected).is_err()
    );
}
#[test]
fn changed_catalog_digest_refuses_before_session_dispatch() {
    let mut boot = contextual_boot();
    boot.catalog_sha256 = Some("0".repeat(64));
    assert_eq!(boot.validate_context(), Err("owner.context_binding"));
    boot.catalog_sha256 = None;
    assert!(boot.validate_context().is_err());
}
#[test]
fn legacy_boot_and_terminal_stay_compatible() {
    let legacy = boot();
    legacy.validate_context().unwrap();
    let fields = [
        "sandbox",
        "limits",
        "preimage",
        "binding",
        "client",
        "home",
        "broker_port",
        "reads",
        "context",
        "probe",
    ];
    let value = serde_json::to_value(&legacy).unwrap();
    let ordered = |object: &Value, names: &[&str]| {
        format!(
            "{{{}}}",
            names
                .iter()
                .map(|name| format!("{}:{}", serde_json::to_string(name).unwrap(), object[name]))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let sandbox = ordered(
        &value["sandbox"],
        &[
            "namespace",
            "helper",
            "guard",
            "compiler",
            "root",
            "scratch",
            "control",
            "public",
            "output",
        ],
    );
    let budgets = ordered(
        &value["limits"],
        &[
            "max_calls",
            "max_file_bytes",
            "max_output_bytes",
            "max_total_output_bytes",
            "max_exec_ms",
            "max_total_exec_ms",
            "max_session_ms",
            "max_scratch_entries",
            "max_scratch_bytes",
        ],
    );
    let expected = format!(
        "{{{}}}",
        fields
            .iter()
            .map(|name| format!(
                "{}:{}",
                serde_json::to_string(name).unwrap(),
                match *name {
                    "sandbox" => sandbox.clone(),
                    "limits" => budgets.clone(),
                    _ => value[name].to_string(),
                }
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(serde_json::to_string(&legacy).unwrap(), expected);
    assert_eq!(value.as_object().unwrap().len(), 10);
    let text = format!(
        "{{\"schema_version\":1,\"binding\":\"{}\",\"complete\":false,\"halted\":false,\"output\":null,\"journal\":[]}}",
        "a".repeat(64)
    );
    let terminal: ToolTerminal = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&terminal).unwrap(), text);
    terminal.validate_context(None).unwrap();
}
#[test]
fn terminal_telemetry_cannot_grant_context_authority() {
    let old = json!({"schema_version":1,"binding":"a".repeat(64),"complete":false,"halted":false,"output":null,"journal":[]});
    let mut terminal: ToolTerminal = serde_json::from_value(old).unwrap();
    terminal.context_policy_sha256 = Some(policy().digest().unwrap());
    terminal.catalog_sha256 = Some(mcp_catalog_digest(Some(&policy())).unwrap());
    assert!(terminal.validate_context(None).is_err());
    terminal.schema_version = 2;
    assert!(terminal.validate_context(None).is_err());
    terminal.validate_context(Some(&policy())).unwrap();
    let altered = FiniteContextPolicy {
        terminal_argument_sha256: BTreeSet::new(),
        ..policy()
    };
    assert!(terminal.validate_context(Some(&altered)).is_err());
    terminal.catalog_sha256 = Some("f".repeat(64));
    assert!(terminal.validate_context(Some(&policy())).is_err());
}
#[test]
fn contextual_session_serves_reviewed_catalog_and_exact_windows_across_connections() {
    let temp = tempfile::tempdir().unwrap();
    let candidate = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    let control = temp.path().join("control");
    for path in [&candidate, &scratch, &control] {
        fs::create_dir(path).unwrap();
    }
    let source = "a\r\né\r\nc";
    fs::write(candidate.join("input.txt"), source).unwrap();
    let pin = router_types::FiniteClientFile {
        path: "/not-invoked".into(),
        sha256: "a".repeat(64),
    };
    let sandbox = ExecSandbox {
        namespace: pin.clone(),
        helper: pin.clone(),
        guard: pin.clone(),
        compiler: pin,
        root: candidate,
        scratch,
        control,
        public: BTreeMap::from([("input.txt".into(), content_digest(source.as_bytes()))]),
        output: "out.txt".into(),
    };
    let policy = policy();
    let mut session =
        ToolSession::open_with_context(sandbox, None, limits(), policy.clone()).unwrap();
    fn request(session: &mut ToolSession, method: &str, params: Value) -> Vec<Value> {
        let (mut caller, server) = UnixStream::pair().unwrap();
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":method,"params":params}),
        ] {
            serde_json::to_writer(&mut caller, &message).unwrap();
            caller.write_all(b"\n").unwrap();
        }
        caller.shutdown(std::net::Shutdown::Write).unwrap();
        session.serve(server).unwrap();
        BufReader::new(caller)
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect()
    }
    let catalog = request(&mut session, "tools/list", json!({}));
    assert_eq!(
        content_digest(&serde_json::to_vec(&catalog[1]["result"]["tools"]).unwrap()),
        mcp_catalog_digest(Some(&policy)).unwrap()
    );
    let read = request(
        &mut session,
        "tools/call",
        json!({"name":"read_file","arguments":{"path":"input.txt","start_line":2,"max_lines":1}}),
    );
    let result: Value =
        serde_json::from_str(read[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["stdout"], "é\r\n");
    assert_eq!(
        result["window"]["source_sha256"],
        content_digest(source.as_bytes())
    );
    let terminal = session.terminal(&"c".repeat(64), false).unwrap();
    assert_eq!(terminal.schema_version, 2);
    terminal.validate_context(Some(&policy)).unwrap();
    assert!(terminal.validate_context(None).is_err());
}
#[test]
fn context_session_cancelled_before_dispatch_has_no_effects() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    let control = temp.path().join("control");
    for p in [&root, &scratch, &control] {
        fs::create_dir(p).unwrap();
    }
    fs::write(root.join("input.txt"), "public").unwrap();
    let pin = router_types::FiniteClientFile {
        path: "/not-invoked".into(),
        sha256: "a".repeat(64),
    };
    let sandbox = ExecSandbox {
        namespace: pin.clone(),
        helper: pin.clone(),
        guard: pin.clone(),
        compiler: pin,
        root: root.clone(),
        scratch,
        control,
        public: BTreeMap::from([("input.txt".into(), content_digest(b"public"))]),
        output: "out.txt".into(),
    };
    let mut session = ToolSession::open_with_context(sandbox, None, limits(), policy()).unwrap();
    session
        .stop_signal()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (mut caller, server) = UnixStream::pair().unwrap();
    caller.write_all(b"{}\n").unwrap();
    caller.shutdown(std::net::Shutdown::Write).unwrap();
    assert!(session.serve(server).is_err());
    assert!(session.halted());
    assert!(!root.join("out.txt").exists());
    assert!(session.journal().is_empty());
}

fn cancellation_fixture() -> (tempfile::TempDir, ExecSandbox) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    let control = temp.path().join("control");
    for path in [&root, &scratch, &control] {
        fs::create_dir(path).unwrap();
    }
    fs::write(root.join("input.txt"), "public").unwrap();
    let pin = router_types::FiniteClientFile {
        path: "/not-invoked".into(),
        sha256: "a".repeat(64),
    };
    let sandbox = ExecSandbox {
        namespace: pin.clone(),
        helper: pin.clone(),
        guard: pin.clone(),
        compiler: pin,
        root,
        scratch,
        control,
        public: BTreeMap::from([("input.txt".into(), content_digest(b"public"))]),
        output: "out.txt".into(),
    };
    (temp, sandbox)
}
#[test]
fn legacy_stop_notification_keeps_the_schema_one_terminal_exact() {
    let (_temp, sandbox) = cancellation_fixture();
    let session = ToolSession::open(sandbox, None, limits()).unwrap();
    session
        .stop_signal()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let terminal = session.terminal(&"a".repeat(64), false).unwrap();
    let expected = format!(
        "{{\"schema_version\":1,\"binding\":\"{}\",\"complete\":false,\"halted\":false,\"output\":null,\"journal\":[]}}",
        "a".repeat(64)
    );
    assert_eq!(serde_json::to_string(&terminal).unwrap(), expected);
}
#[test]
fn context_cancel_between_messages_prevents_the_next_effect() {
    let (temp, sandbox) = cancellation_fixture();
    let mut session = ToolSession::open_with_context(sandbox, None, limits(), policy()).unwrap();
    let stop = session.stop_signal();
    let (mut caller, server) = UnixStream::pair().unwrap();
    caller
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    let worker = std::thread::spawn(move || {
        let result = session.serve(server);
        (result, session)
    });
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cancel-fixture","version":"1"}}});
    serde_json::to_writer(&mut caller, &init).unwrap();
    caller.write_all(b"\n").unwrap();
    let mut reader = BufReader::new(caller.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("protocolVersion"));
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    for value in [
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"write_file","arguments":{"path":"out.txt","content":"forbidden","create_new":true}}}),
    ] {
        serde_json::to_writer(&mut caller, &value).unwrap();
        caller.write_all(b"\n").unwrap();
    }
    caller.shutdown(std::net::Shutdown::Write).unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["result"]["isError"], true);
    let (result, session) = worker.join().unwrap();
    assert!(result.is_ok());
    assert!(session.halted());
    assert!(session.journal().is_empty());
    assert!(!temp.path().join("candidate/out.txt").exists());
}
