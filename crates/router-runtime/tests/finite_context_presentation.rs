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

use router_runtime::finite_tools::{OutputViewInfo, present_terminal, strip_terminal_sgr};
fn info(raw: &ToolResult, shown: usize) -> OutputViewInfo {
    OutputViewInfo {
        method: "terminal_sgr_v1".into(),
        raw_handle: "a".repeat(64),
        raw_content_bytes: raw.stdout.len() + raw.stderr.len(),
        shown_content_bytes: shown,
    }
}

#[test]
fn unlisted_json_source_and_errors_remain_exact() {
    let source = "// keep \x1b[31m literal\r\n";
    for policy in [None, Some(options(false))] {
        let (_temp, mut file) = fixture(policy, limits(), source);
        let value = raw(
            "{\"n\":9007199254740993,\"label\":\"\\u001b[31m\"}",
            "diagnostic\r\n",
            1,
        );
        let actual = execute(&mut file, &value, &mut 0);
        assert_eq!(actual, serde_json::to_value(&value).unwrap());
        let actual = file
            .dispatch("read_file", &json!({"path":"input.txt"}), &mut |_, _, _| {
                panic!("source read executed")
            })
            .unwrap();
        assert_eq!(actual["stdout"], source);
        assert!(actual.get("view").is_none());
    }
    let (_temp, mut listed) = fixture(Some(options(true)), limits(), source);
    let no_sgr = raw("{\"n\":9007199254740993}", "plain error\r\n", 1);
    assert_eq!(
        execute(&mut listed, &no_sgr, &mut 0),
        serde_json::to_value(no_sgr).unwrap()
    );
    let read = listed
        .dispatch(
            "read_file",
            &json!({"path":"input.txt","start_line":1,"max_lines":1}),
            &mut |_, _, _| panic!("read executed"),
        )
        .unwrap();
    assert_eq!(read["stdout"], source);
    assert!(read.get("view").is_none());
}

#[test]
fn sgr_preserves_whitespace_streams_exit_and_termination() {
    assert_eq!(
        strip_terminal_sgr("\x1b[m  é\t\r\n\x1b[38;5;196mRED\x1b[0m\n").unwrap(),
        "  é\t\r\nRED\n"
    );
    let value = colored();
    let shown = "  échec\r\n\tcontext\n";
    let projected =
        present_terminal(&value, info(&value, shown.len() + value.stderr.len())).unwrap();
    assert_eq!(projected.result.stdout, shown);
    assert_eq!(projected.result.stderr, "detail\r\n");
    assert_eq!(projected.result.exit_code, 1);
    assert_eq!(projected.result.termination, "exited");
    assert_eq!(projected.result.elapsed_ms, 1000);
    assert!(!projected.result.truncated);
    assert_eq!(
        projected.serialized_bytes,
        serde_json::to_vec(&projected.result).unwrap().len()
    );
    assert!(projected.serialized_bytes < serde_json::to_vec(&value).unwrap().len());
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let mut calls = 0;
    let actual = execute(&mut file, &value, &mut calls);
    assert_eq!(actual["stdout"], shown);
    assert_eq!(actual["stderr"], "detail\r\n");
    assert_eq!(actual["exit_code"], 1);
    assert_eq!(actual["view"]["method"], "terminal_sgr_v1");
    assert_eq!(calls, 1);
    assert!(!file.halted());
    let metrics = serde_json::to_value(&file.journal()[0]).unwrap();
    assert_eq!(
        metrics["output_bytes"],
        value.stdout.len() + value.stderr.len()
    );
    assert_eq!(
        metrics["context"]["raw_content_bytes"],
        value.stdout.len() + value.stderr.len()
    );
    assert_eq!(
        metrics["context"]["shown_content_bytes"],
        shown.len() + value.stderr.len()
    );
    assert_eq!(metrics["context"]["recalled_content_bytes"], 0);
    assert_eq!(
        metrics["context"]["serialized_bytes"],
        serde_json::to_vec(&actual).unwrap().len()
    );
    let journal = serde_json::to_string(file.journal()).unwrap();
    assert!(
        !journal.contains("échec") && !journal.contains("detail") && !journal.contains("check.py")
    );
}

#[test]
fn unknown_osc_or_cursor_escape_returns_raw() {
    for control in [
        "\x1b]0;title\x07",
        "\x1b[2J",
        "\x1b[1A",
        "\x1b[?25l",
        "\x1b[38:5:1m",
        "\x1b",
        "\x1b[31",
        "\x1b[31X",
    ] {
        assert!(strip_terminal_sgr(control).is_err());
        let mut value = colored();
        value.stderr.push_str(control);
        let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
        let mut calls = 0;
        assert_eq!(
            execute(&mut file, &value, &mut calls),
            serde_json::to_value(value).unwrap()
        );
        assert_eq!(calls, 1);
    }
}

#[test]
fn hint_and_metadata_can_cancel_the_gain() {
    let value = raw("\x1b[31merror\x1b[0m", "", 1);
    assert!(present_terminal(&value, info(&value, 5)).is_err());
    let empty = raw(&"\x1b[0m".repeat(100), "", 0);
    assert!(present_terminal(&empty, info(&empty, 0)).is_err());
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    assert_eq!(
        execute(&mut file, &value, &mut 0),
        serde_json::to_value(value).unwrap()
    );
}

#[test]
fn projection_failure_does_not_rerun_the_tool() {
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let mut value = colored();
    value.stderr.push_str("\x1b]bad");
    let mut calls = 0;
    let actual = execute(&mut file, &value, &mut calls);
    assert_eq!(calls, 1);
    assert_eq!(actual, serde_json::to_value(value).unwrap());
}

#[test]
fn metadata_validation_and_exact_argument_keys_cannot_expand_authority() {
    // Independent SHA-256 over the specified compact JSON tuple, not the key helper.
    assert_eq!(
        exec_presentation_key(&argv(), None).unwrap(),
        "c3a28b0b3432fef38f3e1850d5cc14c723e97f8bd217e62424a13261d43f6f55"
    );
    assert_ne!(
        exec_presentation_key(&argv(), None).unwrap(),
        exec_presentation_key(&argv(), Some(20000)).unwrap()
    );
    assert_ne!(
        exec_presentation_key(&["python3".into(), "a b".into()], None).unwrap(),
        exec_presentation_key(&["python3".into(), "a".into(), "b".into()], None).unwrap()
    );
    for args in [
        vec![],
        vec!["".into()],
        vec!["x\0y".into()],
        vec!["a".repeat(4097)],
        vec!["a".into(); 33],
        vec!["a".repeat(4096); 5],
    ] {
        assert!(exec_presentation_key(&args, None).is_err());
    }
    for timeout in [0, 20001, u64::MAX] {
        assert!(exec_presentation_key(&argv(), Some(timeout)).is_err());
    }
    let value = colored();
    for bad in [
        OutputViewInfo {
            method: "other".into(),
            ..info(&value, 0)
        },
        OutputViewInfo {
            raw_handle: "bad".into(),
            ..info(&value, 0)
        },
        info(&value, usize::MAX),
    ] {
        assert!(present_terminal(&value, bad).is_err());
    }
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let actual = file
        .dispatch(
            "exec",
            &json!({"argv":argv(),"timeout_ms":20000}),
            &mut |_, _, _| Ok(value.clone()),
        )
        .unwrap();
    assert!(
        actual.get("view").is_none(),
        "absent timeout key must not authorize an explicit timeout"
    );
    let mut other = options(false);
    other
        .terminal_argument_sha256
        .insert(exec_presentation_key(&["rtk".into()], None).unwrap());
    let (_temp, mut file) = fixture(Some(other), limits(), "public");
    assert!(
        file.dispatch("exec", &json!({"argv":["rtk"]}), &mut |_, _, _| panic!(
            "digest admitted a program"
        ))
        .is_err()
    );
}

#[test]
fn no_raw_recovery_capacity_means_no_view() {
    let value = colored();
    let bytes = value.stdout.len() + value.stderr.len();
    let mut budgets = limits();
    budgets.max_total_output_bytes = bytes * 2 - 1;
    let (_temp, mut file) = fixture(Some(options(true)), budgets, "public");
    assert_eq!(
        execute(&mut file, &value, &mut 0),
        serde_json::to_value(value.clone()).unwrap()
    );
    let mut budgets = limits();
    budgets.max_calls = 1;
    let (_temp, mut file) = fixture(Some(options(true)), budgets, "public");
    assert_eq!(
        execute(&mut file, &value, &mut 0),
        serde_json::to_value(value).unwrap()
    );
}

#[test]
fn postcondition_failure_precedes_presentation_and_recovery() {
    let (temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let mut calls = 0;
    assert!(
        file.dispatch("exec", &json!({"argv":argv()}), &mut |_, _, _| {
            calls += 1;
            fs::write(temp.path().join("candidate/input.txt"), "changed").unwrap();
            Ok(colored())
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    assert!(file.halted());
    assert_eq!(
        file.journal().last().unwrap().status,
        "postcondition_failed"
    );
    assert!(
        !serde_json::to_string(file.journal())
            .unwrap()
            .contains("échec")
    );
}

#[test]
fn unrecoverable_raw_json_never_advertises_a_handle() {
    // Content fits 8192 bytes but JSON escaping exceeds the existing MCP limit.
    let value = raw(
        &format!("{}{}error", "\x1b[31m".repeat(200), "\0".repeat(5200)),
        "",
        1,
    );
    assert!(value.stdout.len() < 8192);
    assert!(serde_json::to_vec(&value).unwrap().len() > 32768);
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let actual = execute(&mut file, &value, &mut 0);
    assert!(
        actual.get("view").is_none(),
        "a view must not promise a raw result the codec cannot return"
    );
}

#[test]
fn callback_metadata_cannot_claim_a_view_or_a_source_window() {
    use router_runtime::finite_tools::ReadWindowInfo;
    let (_temp, mut file) = fixture(Some(options(true)), limits(), "public");
    let mut value = colored();
    value.view = Some(info(&value, 1));
    value.window = Some(ReadWindowInfo {
        source_sha256: "b".repeat(64),
        first_line: 100,
        returned_lines: 1,
        total_lines: 100,
        has_more: false,
    });
    let actual = execute(&mut file, &value, &mut 0);
    assert!(actual.get("window").is_none());
    assert_eq!(actual["stdout"], "  échec\r\n\tcontext\n");
    assert_ne!(actual["view"]["raw_handle"], "a".repeat(64));
}

#[test]
fn terminal_policy_digest_validation_is_bounded() {
    let mut policy = options(false);
    policy.terminal_argument_sha256 = (0..8).map(|n| format!("{n:064x}")).collect();
    let (_temp, _file) = fixture(Some(policy), limits(), "public");
    for hashes in [
        BTreeSet::from(["A".repeat(64)]),
        BTreeSet::from(["g".repeat(64)]),
        BTreeSet::from(["a".repeat(65)]),
        (0..9).map(|n| format!("{n:064x}")).collect(),
    ] {
        let mut policy = options(false);
        policy.terminal_argument_sha256 = hashes;
        assert!(
            FilePolicy::open_with_context(
                std::path::Path::new("/absent"),
                std::path::Path::new("/absent"),
                BTreeMap::new(),
                "out",
                None,
                limits(),
                policy
            )
            .is_err()
        );
    }
}
