//! Inert file windows: no model, provider, worker or process execution.
use router_adapter_api::content_digest;
use router_runtime::finite_tools::{
    FilePolicy, FiniteContextPolicy, Limits, LineWindow, ToolResult, select_line_window,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

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
fn context(enabled: bool) -> FiniteContextPolicy {
    FiniteContextPolicy {
        schema_version: 1,
        read_windows: enabled,
        terminal_argument_sha256: BTreeSet::new(),
    }
}
fn fixture(
    text: &str,
    policy: Option<FiniteContextPolicy>,
    budgets: Limits,
) -> (tempfile::TempDir, FilePolicy) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&scratch).unwrap();
    fs::write(root.join("input.txt"), text).unwrap();
    let public = BTreeMap::from([("input.txt".into(), content_digest(text.as_bytes()))]);
    let file = match policy {
        None => FilePolicy::open(&root, &scratch, public, "output.txt", None, budgets),
        Some(policy) => FilePolicy::open_with_context(
            &root,
            &scratch,
            public,
            "output.txt",
            None,
            budgets,
            policy,
        ),
    }
    .unwrap();
    (temp, file)
}
fn read(
    policy: &mut FilePolicy,
    args: Value,
) -> Result<Value, router_runtime::finite_tools::FiniteToolError> {
    policy.dispatch("read_file", &args, &mut |_, _, _| {
        panic!("reading must not execute")
    })
}

#[test]
fn window_preserves_exact_crlf_utf8_and_unterminated_last_line() {
    let text = "a\r\né\r\nc";
    let w = select_line_window(
        text,
        LineWindow {
            start_line: 2,
            max_lines: 1,
        },
    )
    .unwrap();
    assert_eq!(
        (
            w.text,
            w.first_line,
            w.returned_lines,
            w.total_lines,
            w.has_more
        ),
        ("é\r\n", 2, 1, 3, true)
    );
    let last = select_line_window(
        text,
        LineWindow {
            start_line: 3,
            max_lines: 32768,
        },
    )
    .unwrap();
    assert_eq!(
        (last.text, last.returned_lines, last.has_more),
        ("c", 1, false)
    );
    for (text, start, count, want, total, returned, more) in [
        ("", 1, 1, "", 0, 0, false),
        ("\n", 1, 1, "\n", 1, 1, false),
        ("a\n\n", 2, 1, "\n", 2, 1, false),
        (" a\t\r\nb\n", 1, 1, " a\t\r\n", 2, 1, true),
        ("a\n", 2, 1, "", 1, 0, false),
        ("a", 2, 1, "", 1, 0, false),
        ("\r", 1, 1, "\r", 1, 1, false),
    ] {
        let w = select_line_window(
            text,
            LineWindow {
                start_line: start,
                max_lines: count,
            },
        )
        .unwrap();
        assert_eq!(
            (
                w.text,
                w.first_line,
                w.total_lines,
                w.returned_lines,
                w.has_more
            ),
            (want, start, total, returned, more)
        );
    }
    for (start, count) in [
        (0, 1),
        (1, 0),
        (3, 1),
        (32769, 1),
        (1, 32769),
        (u32::MAX, u32::MAX),
    ] {
        assert!(
            select_line_window(
                "a",
                LineWindow {
                    start_line: start,
                    max_lines: count
                }
            )
            .is_err()
        );
    }
    let all_lines = "\n".repeat(32768);
    let w = select_line_window(
        &all_lines,
        LineWindow {
            start_line: 32768,
            max_lines: 32768,
        },
    )
    .unwrap();
    assert_eq!((w.text, w.returned_lines, w.total_lines), ("\n", 1, 32768));
}

#[test]
fn legacy_read_and_catalog_remain_byte_identical() {
    let text = "// keep\r\n  é\nlast";
    let (_temp, mut old) = fixture(text, None, limits());
    let (_temp2, mut opted) = fixture(text, Some(context(true)), limits());
    for policy in [&mut old, &mut opted] {
        let result = read(policy, json!({"path":"input.txt"})).unwrap();
        assert_eq!(result["stdout"], text);
        assert_eq!(result.as_object().unwrap().len(), 6);
        assert!(result.get("window").is_none());
        assert_eq!(policy.journal()[0].file_sha256, None);
    }
    let literal = r#"{"stdout":"a\r\né","stderr":"","exit_code":0,"termination":"exited","truncated":false,"elapsed_ms":0}"#;
    let decoded: ToolResult = serde_json::from_str(literal).unwrap();
    assert!(decoded.window.is_none());
    assert_eq!(serde_json::to_string(&decoded).unwrap(), literal);
}

#[test]
fn window_arguments_require_both_fields_and_enabled_policy() {
    for enabled in [None, Some(context(false)), Some(context(true))] {
        for args in [
            json!({"path":"input.txt","start_line":1}),
            json!({"path":"input.txt","max_lines":1}),
            json!({"path":"input.txt","start_line":0,"max_lines":1}),
            json!({"path":"input.txt","start_line":1,"max_lines":32769}),
            json!({"path":"input.txt","start_line":-1,"max_lines":1}),
            json!({"path":"input.txt","start_line":1.0,"max_lines":1}),
            json!({"path":"input.txt","start_line":"1","max_lines":1}),
            json!({"path":"input.txt","start_line":null,"max_lines":1}),
            json!({"path":"input.txt","start_line":u64::MAX,"max_lines":1}),
            json!({"path":"input.txt","start_line":1,"max_lines":1,"extra":true}),
        ] {
            let (_temp, mut policy) = fixture("a\nb", enabled.clone(), limits());
            assert!(read(&mut policy, args).is_err());
        }
    }
    for policy in [None, Some(context(false))] {
        let (_temp, mut file) = fixture("a", policy, limits());
        assert!(
            read(
                &mut file,
                json!({"path":"input.txt","start_line":1,"max_lines":1})
            )
            .is_err()
        );
    }
}

#[test]
fn window_source_hash_matches_full_file() {
    let text = "first\r\né\r\nlast";
    let (_temp, mut policy) = fixture(text, Some(context(true)), limits());
    let value = read(
        &mut policy,
        json!({"path":"input.txt","start_line":2,"max_lines":1}),
    )
    .unwrap();
    assert_eq!(value["stdout"], "é\r\n");
    assert_eq!(
        value["window"],
        json!({"source_sha256":content_digest(text.as_bytes()),"first_line":2,"returned_lines":1,"total_lines":3,"has_more":true})
    );
    assert_eq!(
        policy.journal()[0].file_sha256.as_deref(),
        Some(content_digest(text.as_bytes()).as_str())
    );
    let journal = serde_json::to_string(policy.journal()).unwrap();
    assert!(!journal.contains("first") && !journal.contains("input.txt"));
    let eof = read(
        &mut policy,
        json!({"path":"input.txt","start_line":4,"max_lines":1}),
    )
    .unwrap();
    assert_eq!(eof["stdout"], "");
    assert_eq!(eof["window"]["returned_lines"], 0);
    assert_eq!(eof["window"]["has_more"], false);
    assert!(
        read(
            &mut policy,
            json!({"path":"input.txt","start_line":5,"max_lines":1})
        )
        .is_err()
    );
}

#[test]
fn oversized_selected_line_keeps_budget_stop() {
    let text = format!("short\n{}", "é".repeat(5000));
    let (_temp, mut policy) = fixture(&text, Some(context(true)), limits());
    let value = read(
        &mut policy,
        json!({"path":"input.txt","start_line":2,"max_lines":1}),
    )
    .unwrap();
    assert_eq!(
        value["stdout"].as_str().unwrap().as_bytes(),
        "é".repeat(4096).as_bytes()
    );
    assert_eq!(value["truncated"], true);
    assert!(value.get("window").is_none());
    assert!(policy.halted());
    assert_eq!(
        policy.journal()[0].file_sha256,
        Some(content_digest(text.as_bytes()))
    );
    assert!(read(&mut policy, json!({"path":"input.txt"})).is_err());
}

#[test]
fn session_budget_truncation_removes_window_metadata() {
    let mut budgets = limits();
    budgets.max_total_output_bytes = 5;
    let (_temp, mut policy) = fixture("é\né\n", Some(context(true)), budgets);
    let args = json!({"path":"input.txt","start_line":1,"max_lines":1});
    let first = read(&mut policy, args.clone()).unwrap();
    assert_eq!(first["stdout"], "é\n");
    assert!(first.get("window").is_some());
    let second = read(&mut policy, args).unwrap();
    assert_eq!(second["stdout"], "é");
    assert_eq!(second["truncated"], true);
    assert!(second.get("window").is_none());
    assert!(policy.halted());
}

#[test]
fn windows_keep_call_file_scope_and_preimage_limits() {
    let (temp, mut policy) = fixture("a\nb", Some(context(true)), limits());
    assert!(
        read(
            &mut policy,
            json!({"path":"../input.txt","start_line":1,"max_lines":1})
        )
        .is_err()
    );
    fs::write(temp.path().join("candidate/input.txt"), "changed").unwrap();
    assert!(
        read(
            &mut policy,
            json!({"path":"input.txt","start_line":1,"max_lines":1})
        )
        .is_err()
    );
    assert!(policy.halted());
    let (_temp, mut policy) = fixture("a\nb", Some(context(true)), limits());
    for _ in 0..8 {
        read(
            &mut policy,
            json!({"path":"input.txt","start_line":1,"max_lines":1}),
        )
        .unwrap();
    }
    assert!(
        read(
            &mut policy,
            json!({"path":"input.txt","start_line":1,"max_lines":1})
        )
        .is_err()
    );
    assert!(policy.halted());
}

#[test]
fn invalid_context_policy_is_rejected_before_filesystem_access() {
    for policy in [
        FiniteContextPolicy {
            schema_version: 2,
            ..context(true)
        },
        FiniteContextPolicy {
            terminal_argument_sha256: BTreeSet::from(["invalid".into()]),
            ..context(true)
        },
        FiniteContextPolicy {
            terminal_argument_sha256: (0..9).map(|n| format!("{n:064x}")).collect(),
            ..context(true)
        },
    ] {
        let err = FilePolicy::open_with_context(
            std::path::Path::new("/absent"),
            std::path::Path::new("/absent"),
            BTreeMap::new(),
            "out",
            None,
            limits(),
            policy,
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("context_policy"));
    }
    assert!(serde_json::from_value::<FiniteContextPolicy>(json!({"schema_version":1,"read_windows":true,"terminal_argument_sha256":[],"extra":true})).is_err());
}

#[test]
fn context_policy_is_bound_without_changing_legacy_preparation() {
    let (temp, legacy) = fixture("a", None, limits());
    let root = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    let public = BTreeMap::from([("input.txt".into(), content_digest(b"a"))]);
    let old_hash = legacy.preparation_binding().unwrap();
    let again = FilePolicy::open(
        &root,
        &scratch,
        public.clone(),
        "output.txt",
        None,
        limits(),
    )
    .unwrap();
    assert_eq!(again.preparation_binding().unwrap(), old_hash);
    let enabled = FilePolicy::open_with_context(
        &root,
        &scratch,
        public.clone(),
        "output.txt",
        None,
        limits(),
        context(true),
    )
    .unwrap();
    let disabled = FilePolicy::open_with_context(
        &root,
        &scratch,
        public,
        "output.txt",
        None,
        limits(),
        context(false),
    )
    .unwrap();
    assert_ne!(enabled.preparation_binding().unwrap(), old_hash);
    assert_ne!(
        disabled.preparation_binding().unwrap(),
        enabled.preparation_binding().unwrap()
    );
}
