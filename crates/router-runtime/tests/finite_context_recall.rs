// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use router_adapter_api::content_digest;
use router_runtime::finite_tools::{
    FilePolicy, FiniteContextPolicy, Limits, ToolResult, exec_presentation_key,
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
fn argv() -> Vec<String> {
    ["python3", "-B", "check.py"].map(str::to_owned).to_vec()
}
fn options(listed: bool) -> FiniteContextPolicy {
    FiniteContextPolicy {
        schema_version: 1,
        read_windows: true,
        terminal_argument_sha256: if listed {
            BTreeSet::from([exec_presentation_key(&argv(), None).unwrap()])
        } else {
            BTreeSet::new()
        },
    }
}
fn raw(stdout: &str, stderr: &str, code: i32) -> ToolResult {
    ToolResult {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_code: code,
        termination: "exited".into(),
        truncated: false,
        elapsed_ms: 1000,
        window: None,
        view: None,
    }
}
fn colored() -> ToolResult {
    raw(
        &format!("{}  échec\r\n\tcontext\n\x1b[0m", "\x1b[31m".repeat(80)),
        "detail\r\n",
        1,
    )
}
fn fixture(
    policy: Option<FiniteContextPolicy>,
    budgets: Limits,
    source: &str,
) -> (tempfile::TempDir, FilePolicy) {
    let temp = tempfile::tempdir().unwrap();
    let candidate = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    fs::create_dir(&candidate).unwrap();
    fs::create_dir(&scratch).unwrap();
    fs::write(candidate.join("input.txt"), source).unwrap();
    let public = BTreeMap::from([("input.txt".into(), content_digest(source.as_bytes()))]);
    let file = match policy {
        Some(p) => {
            FilePolicy::open_with_context(&candidate, &scratch, public, "out.txt", None, budgets, p)
        }
        None => FilePolicy::open(&candidate, &scratch, public, "out.txt", None, budgets),
    }
    .unwrap();
    (temp, file)
}
fn execute(file: &mut FilePolicy, value: &ToolResult, calls: &mut usize) -> Value {
    file.dispatch("exec", &json!({"argv":argv()}), &mut |args, _, _| {
        assert_eq!(args, argv());
        *calls += 1;
        Ok(value.clone())
    })
    .unwrap()
}

use router_runtime::finite_tools::RecallStore;

#[test]
fn recall_is_complete_and_session_scoped() {
    let value = colored();
    let mut a = RecallStore::new("a".repeat(64)).unwrap();
    let mut b = RecallStore::new("b".repeat(64)).unwrap();
    let handle = a.insert(1, &"c".repeat(64), &value).unwrap();
    assert_eq!(
        serde_json::to_value(a.get(&handle).unwrap()).unwrap(),
        serde_json::to_value(&value).unwrap()
    );
    let other = b.insert(1, &"c".repeat(64), &value).unwrap();
    assert_ne!(handle, other);
    assert!(b.get(&handle).is_err());
    let mut changed = RecallStore::new("a".repeat(64)).unwrap();
    let mut changed_raw = value.clone();
    changed_raw.exit_code = 2;
    assert_ne!(
        handle,
        changed.insert(1, &"c".repeat(64), &changed_raw).unwrap()
    );
    let mut c = RecallStore::new("a".repeat(64)).unwrap();
    assert_ne!(handle, c.insert(1, &"d".repeat(64), &value).unwrap());
    assert!(a.insert(1, &"c".repeat(64), &value).is_err());
    let next = a.insert(2, &"c".repeat(64), &value).unwrap();
    assert_ne!(next, handle);
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let mut calls = 0;
    let shown = execute(&mut file, &value, &mut calls);
    let handle = shown["view"]["raw_handle"].as_str().unwrap();
    let got = file
        .dispatch("read_output", &json!({"handle":handle}), &mut |_, _, _| {
            panic!("recall executed")
        })
        .unwrap();
    assert_eq!(got, serde_json::to_value(&value).unwrap());
    assert_eq!(calls, 1);
    assert!(got.get("window").is_none() && got.get("view").is_none());
}

#[test]
fn reopening_same_policy_does_not_share_handles() {
    let value = colored();
    let (temp, mut first) = fixture(Some(options(true)), limits(), "public");
    let candidate = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    let public = BTreeMap::from([("input.txt".into(), content_digest(b"public"))]);
    let mut second = FilePolicy::open_with_context(
        &candidate,
        &scratch,
        public,
        "out.txt",
        None,
        limits(),
        options(true),
    )
    .unwrap();
    assert_eq!(
        first.preparation_binding().unwrap(),
        second.preparation_binding().unwrap()
    );
    let a = execute(&mut first, &value, &mut 0);
    let b = execute(&mut second, &value, &mut 0);
    assert_ne!(a["view"]["raw_handle"], b["view"]["raw_handle"]);
    assert!(
        second
            .dispatch(
                "read_output",
                &json!({"handle":a["view"]["raw_handle"]}),
                &mut |_, _, _| panic!("foreign handle executed")
            )
            .is_err()
    );
}

#[test]
fn unknown_handle_never_dispatches() {
    for policy in [None, Some(options(false)), Some(options(true))] {
        let (_temp, mut file) = fixture(policy, limits(), "public");
        for args in [
            json!({"handle":"a".repeat(64)}),
            json!({"handle":"../input.txt"}),
            json!({"handle":true}),
            json!({"handle":"a".repeat(64),"path":"input.txt"}),
        ] {
            assert!(
                file.dispatch("read_output", &args, &mut |_, _, _| panic!(
                    "unknown recall executed"
                ))
                .is_err()
            );
        }
    }
}

#[test]
fn full_cache_preserves_existing_raw_without_partial_entries() {
    let value = raw(&"x".repeat(8192), "", 0);
    let mut store = RecallStore::new("a".repeat(64)).unwrap();
    let mut handles = vec![];
    for sequence in 1..=4 {
        handles.push(store.insert(sequence, &"b".repeat(64), &value).unwrap());
    }
    assert!(store.insert(5, &"b".repeat(64), &raw("x", "", 0)).is_err());
    for h in handles {
        assert_eq!(store.get(&h).unwrap().stdout, value.stdout);
    }
    let mut store = RecallStore::new("a".repeat(64)).unwrap();
    for sequence in 1..=8 {
        store
            .insert(sequence, &"b".repeat(64), &raw("", "", 0))
            .unwrap();
    }
    assert!(store.insert(9, &"b".repeat(64), &raw("", "", 0)).is_err());
    assert!(RecallStore::new("bad".into()).is_err());
    let mut store = RecallStore::new("a".repeat(64)).unwrap();
    assert!(store.insert(0, &"b".repeat(64), &value).is_err());
    assert!(store.insert(1, "bad", &value).is_err());
    let mut bad = value.clone();
    bad.stdout.push('x');
    assert!(store.insert(1, &"b".repeat(64), &bad).is_err());
    bad = value.clone();
    bad.truncated = true;
    assert!(store.insert(1, &"b".repeat(64), &bad).is_err());
    bad = value.clone();
    bad.termination = "timeout".into();
    assert!(store.insert(1, &"b".repeat(64), &bad).is_err());
    assert!(store.get("../input.txt").is_err());
}

#[test]
fn recall_consumes_calls_and_output_budget() {
    let value = colored();
    let bytes = value.stdout.len() + value.stderr.len();
    let mut budgets = limits();
    budgets.max_total_output_bytes = bytes * 2;
    let (_temp, mut file) = fixture(Some(options(true)), budgets, "public");
    let shown = execute(&mut file, &value, &mut 0);
    let handle = shown["view"]["raw_handle"].as_str().unwrap();
    let got = file
        .dispatch("read_output", &json!({"handle":handle}), &mut |_, _, _| {
            panic!("recall executed")
        })
        .unwrap();
    assert_eq!(got, serde_json::to_value(&value).unwrap());
    assert!(file.halted());
    assert_eq!(
        file.journal().iter().map(|r| r.output_bytes).sum::<usize>(),
        bytes * 2
    );
    let metrics = serde_json::to_value(&file.journal()[1]).unwrap();
    assert_eq!(metrics["tool"], "read_output");
    assert_eq!(metrics["context"]["recalled_content_bytes"], bytes);
    assert_eq!(metrics["context"]["raw_content_bytes"], 0);
    assert!(
        file.dispatch(
            "read_output",
            &json!({"handle":handle}),
            &mut |_, _, _| panic!("exhausted recall executed")
        )
        .is_err()
    );
    let mut budgets = limits();
    budgets.max_calls = 2;
    let (_temp, mut file) = fixture(Some(options(true)), budgets, "public");
    let shown = execute(&mut file, &value, &mut 0);
    let handle = shown["view"]["raw_handle"].as_str().unwrap();
    file.dispatch("read_output", &json!({"handle":handle}), &mut |_, _, _| {
        panic!("recall executed")
    })
    .unwrap();
    assert!(
        file.dispatch(
            "read_output",
            &json!({"handle":handle}),
            &mut |_, _, _| panic!("ninth call executed")
        )
        .is_err()
    );
}

#[test]
fn later_consumption_refuses_partial_recall() {
    let value = colored();
    let bytes = value.stdout.len() + value.stderr.len();
    let mut budgets = limits();
    budgets.max_total_output_bytes = bytes * 2;
    let (_temp, mut file) = fixture(Some(options(true)), budgets, "1234567890");
    let shown = execute(&mut file, &value, &mut 0);
    let handle = shown["view"]["raw_handle"].as_str().unwrap();
    file.dispatch("read_file", &json!({"path":"input.txt"}), &mut |_, _, _| {
        panic!("read executed")
    })
    .unwrap();
    assert!(
        file.dispatch(
            "read_output",
            &json!({"handle":handle}),
            &mut |_, _, _| panic!("partial recall executed")
        )
        .is_err()
    );
    assert!(file.halted());
    assert_eq!(file.journal().last().unwrap().output_bytes, 0);
}

#[test]
fn capture_limit_is_not_rescued_by_projection() {
    for mode in ["budget", "truncated", "termination"] {
        let mut value = colored();
        let mut budgets = limits();
        if mode == "budget" {
            budgets.max_output_bytes = 100;
        } else if mode == "truncated" {
            value.truncated = true;
        } else {
            value.termination = "timed_out".into();
        }
        let (_temp, mut file) = fixture(Some(options(true)), budgets, "public");
        let mut calls = 0;
        let actual = execute(&mut file, &value, &mut calls);
        assert_eq!(calls, 1);
        assert!(actual.get("view").is_none());
        assert!(file.halted());
        if mode == "budget" {
            assert_eq!(actual["stdout"].as_str().unwrap().len(), 100);
            assert_eq!(actual["truncated"], true);
        } else {
            assert_eq!(actual, serde_json::to_value(value).unwrap());
        }
    }
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let error = file.dispatch("exec", &json!({"argv":argv()}), &mut |_, _, _| {
        Err(router_runtime::finite_tools::FiniteToolError::Execution)
    });
    assert!(error.is_err());
    assert!(file.halted());
}

#[test]
fn stdout_and_stderr_remain_separate_with_order_within_each() {
    let stdout = format!("{}one\r\n  two\n\x1b[0m", "\x1b[31m".repeat(80));
    let stderr = format!("{}E1\nE2\r\n\x1b[0m", "\x1b[32m".repeat(80));
    let value = raw(&stdout, &stderr, 7);
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let shown = execute(&mut file, &value, &mut 0);
    assert_eq!(shown["stdout"], "one\r\n  two\n");
    assert_eq!(shown["stderr"], "E1\nE2\r\n");
    assert_eq!(shown["exit_code"], 7);
    let recovered = file
        .dispatch(
            "read_output",
            &json!({"handle":shown["view"]["raw_handle"]}),
            &mut |_, _, _| panic!("recall executed"),
        )
        .unwrap();
    assert_eq!(recovered, serde_json::to_value(value).unwrap());
}
