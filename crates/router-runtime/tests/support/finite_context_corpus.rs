//! Test-only authored comparison. No product authority or accounting API.
use router_adapter_api::content_digest;
use router_runtime::finite_tools::{
    FilePolicy, FiniteContextPolicy, Limits, OutputViewInfo, ReadWindowInfo, RecallStore,
    ToolResult, exec_presentation_key, present_terminal, strip_terminal_sgr,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

const FIXTURES: &[u8] = include_bytes!("../../../../tests/fixtures/finite-context/corpus.json");
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
    vec!["python3".into(), "-B".into(), "check.py".into()]
}
fn policy(presentation: bool) -> FiniteContextPolicy {
    FiniteContextPolicy {
        schema_version: 1,
        read_windows: true,
        terminal_argument_sha256: if presentation {
            BTreeSet::from([exec_presentation_key(&argv(), None).unwrap()])
        } else {
            BTreeSet::new()
        },
    }
}
fn fixture(source: &str, presentation: bool, budgets: Limits) -> (tempfile::TempDir, FilePolicy) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("candidate");
    let scratch = temp.path().join("scratch");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&scratch).unwrap();
    fs::write(root.join("input.txt"), source).unwrap();
    let file = FilePolicy::open_with_context(
        &root,
        &scratch,
        BTreeMap::from([("input.txt".into(), content_digest(source.as_bytes()))]),
        "out.txt",
        None,
        budgets,
        policy(presentation),
    )
    .unwrap();
    (temp, file)
}
fn no_exec(
    _: &[String],
    _: u64,
    _: usize,
) -> Result<ToolResult, router_runtime::finite_tools::FiniteToolError> {
    panic!("offline non-exec operation reached the process callback")
}
fn execute(file: &mut FilePolicy, raw: &ToolResult) -> ToolResult {
    let mut calls = 0;
    let value = file
        .dispatch(
            "exec",
            &json!({"argv":argv()}),
            &mut |args, timeout, cap| {
                assert_eq!(args, argv());
                assert_eq!(timeout, 20000);
                assert!(cap <= 8192 && cap >= content(raw));
                calls += 1;
                Ok(raw.clone())
            },
        )
        .unwrap();
    assert_eq!(calls, 1, "presentation must not run the callback twice");
    serde_json::from_value(value).unwrap()
}
fn read(file: &mut FilePolicy, args: Value) -> ToolResult {
    serde_json::from_value(file.dispatch("read_file", &args, &mut no_exec).unwrap()).unwrap()
}
fn size(result: &ToolResult) -> usize {
    serde_json::to_vec(result).unwrap().len()
}
fn content(result: &ToolResult) -> usize {
    result.stdout.len() + result.stderr.len()
}
fn same_result(actual: &ToolResult, expected: &ToolResult) {
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stderr, expected.stderr);
    assert_eq!(actual.exit_code, expected.exit_code);
    assert_eq!(actual.termination, expected.termination);
    assert_eq!(actual.truncated, expected.truncated);
    assert_eq!(actual.window, expected.window);
    assert_eq!(actual.view.is_some(), expected.view.is_some());
}
fn raw(stdout: &str) -> ToolResult {
    ToolResult {
        stdout: stdout.into(),
        stderr: String::new(),
        exit_code: 0,
        termination: "exited".into(),
        truncated: false,
        elapsed_ms: 1000,
        window: None,
        view: None,
    }
}
fn row(
    id: &str,
    mode: &str,
    base: &ToolResult,
    shown: &ToolResult,
    recalls: u64,
    presentation: bool,
) -> Value {
    json!({"fixture_id":id,"policy_sha256":policy(presentation).digest().unwrap(),"mode":mode,
        "raw_content_bytes":content(base),"shown_content_bytes":content(shown),
        "recalled_content_bytes":content(base) as u64 * recalls,
        "raw_json_bytes":size(base),"serialized_bytes":size(shown) as u64 + size(base) as u64 * recalls,
        "fidelity":"pass"})
}
fn source_case(case: &Value, rows: &mut Vec<Value>) {
    let source = case["source"].as_str().unwrap();
    let id = case["id"].as_str().unwrap();
    let expected = &case["expected"];
    let mut base = raw(case["exact"]["stdout"].as_str().unwrap());
    base.truncated = case["exact"]["truncated"].as_bool().unwrap();
    let (_temp, mut file) = fixture(source, false, limits());
    let observed = read(&mut file, json!({"path":"input.txt"}));
    same_result(&observed, &base);
    assert_eq!(file.halted(), base.truncated);
    rows.push(row(id, "exact", &base, &base, 0, false));
    let (_temp, mut file) = fixture(source, false, limits());
    let observed = read(
        &mut file,
        json!({"path":"input.txt","start_line":case["start_line"],"max_lines":case["max_lines"]}),
    );
    let mut selected = raw(expected["stdout"].as_str().unwrap());
    selected.truncated = expected["truncated"].as_bool().unwrap();
    if !selected.truncated {
        selected.window = Some(ReadWindowInfo {
            source_sha256: content_digest(source.as_bytes()),
            first_line: case["start_line"].as_u64().unwrap().try_into().unwrap(),
            returned_lines: case["max_lines"].as_u64().unwrap().try_into().unwrap(),
            total_lines: expected["total_lines"]
                .as_u64()
                .unwrap()
                .try_into()
                .unwrap(),
            has_more: expected["has_more"].as_bool().unwrap(),
        });
    }
    same_result(&observed, &selected);
    assert_eq!(file.halted(), selected.truncated);
    rows.push(row(id, "window", &base, &selected, 0, false));
}
fn output_case(case: &Value, rows: &mut Vec<Value>, fallbacks: &mut Vec<Value>) {
    let id = case["id"].as_str().unwrap();
    let base: ToolResult = serde_json::from_value(case["raw"].clone()).unwrap();
    let (_temp, mut exact) = fixture("public", false, limits());
    same_result(&execute(&mut exact, &base), &base);
    rows.push(row(id, "exact", &base, &base, 0, false));
    // Stable caller-owned binding for pure authored serialization only. Real
    // FilePolicy sessions below retain their random private bindings and timing.
    let mut store =
        RecallStore::new(content_digest(b"authored-offline-context-session:v1")).unwrap();
    let projected = if base.truncated {
        None
    } else {
        let handle = store
            .insert(1, &policy(true).digest().unwrap(), &base)
            .unwrap();
        match (
            strip_terminal_sgr(&base.stdout),
            strip_terminal_sgr(&base.stderr),
        ) {
            (Ok(stdout), Ok(stderr)) => present_terminal(
                &base,
                OutputViewInfo {
                    method: "terminal_sgr_v1".into(),
                    raw_handle: handle,
                    raw_content_bytes: content(&base),
                    shown_content_bytes: stdout.len() + stderr.len(),
                },
            )
            .ok()
            .map(|p| p.result),
            _ => None,
        }
    };
    let shown = projected.unwrap_or_else(|| base.clone());
    let expected = &case["expected"];
    assert_eq!(shown.stdout, expected["stdout"].as_str().unwrap());
    assert_eq!(shown.stderr, expected["stderr"].as_str().unwrap());
    assert_eq!(
        shown.exit_code,
        expected["exit_code"].as_i64().unwrap() as i32
    );
    assert_eq!(shown.termination, expected["termination"].as_str().unwrap());
    assert_eq!(shown.truncated, expected["truncated"].as_bool().unwrap());
    if let Some(reason) = case["fallback"].as_str() {
        assert!(shown.view.is_none());
        same_result(&shown, &base);
        match reason {
            "unsupported_escape" => assert!(
                strip_terminal_sgr(&base.stdout).is_err()
                    || strip_terminal_sgr(&base.stderr).is_err()
            ),
            "incomplete_result" => assert!(base.truncated),
            "metadata_no_gain" => {
                assert!(strip_terminal_sgr(&base.stdout).unwrap().len() < base.stdout.len())
            }
            "no_sgr_gain" => assert_eq!(strip_terminal_sgr(&base.stdout).unwrap(), base.stdout),
            _ => panic!("unrecognized authored fallback reason"),
        }
        if reason != "incomplete_result" {
            fallbacks.push(json!({"fixture_id":id,"reason":reason}));
        }
    } else {
        assert!(shown.view.is_some());
        assert!(size(&shown) < size(&base));
    }
    let (_temp, mut file) = fixture("public", true, limits());
    let actual = execute(&mut file, &base);
    same_result(&actual, &shown);
    assert_eq!(file.halted(), base.truncated);
    let recalls = case["recalls"].as_u64().unwrap();
    assert!(recalls <= 1);
    if let Some(view) = &actual.view {
        assert_eq!(view.method, "terminal_sgr_v1");
        assert_eq!(view.raw_content_bytes, content(&base));
        assert_eq!(view.shown_content_bytes, content(&actual));
        let recovered: ToolResult = serde_json::from_value(
            file.dispatch(
                "read_output",
                &json!({"handle":view.raw_handle}),
                &mut no_exec,
            )
            .unwrap(),
        )
        .unwrap();
        let mut expected_raw = base.clone();
        expected_raw.elapsed_ms = actual.elapsed_ms;
        assert_eq!(
            serde_json::to_value(recovered).unwrap(),
            serde_json::to_value(expected_raw).unwrap()
        );
        // All presented cases exercise recovery for fidelity. Only the authored
        // scenario's requested recalls are included in the comparison row.
        assert_eq!(
            file.journal().iter().filter(|r| r.tool == "exec").count(),
            1
        );
        assert_eq!(
            file.journal()
                .iter()
                .filter(|r| r.tool == "read_output")
                .count(),
            1
        );
    } else {
        assert_eq!(recalls, 0);
    }
    rows.push(row(id, "view", &base, &shown, recalls, true));
}
fn guard_case(case: &Value) -> Value {
    let id = case["id"].as_str().unwrap();
    match case["kind"].as_str().unwrap() {
        "source_replaced" => {
            let (temp, mut file) = fixture("before\r\n", false, limits());
            fs::write(temp.path().join("candidate/input.txt"), b"after\r\n").unwrap();
            assert!(
                file.dispatch(
                    "read_file",
                    &json!({"path":"input.txt","start_line":1,"max_lines":1}),
                    &mut no_exec
                )
                .is_err()
            );
            assert!(file.halted());
            assert_eq!(file.journal().last().unwrap().status, "precondition_failed");
        }
        "foreign_handle" => {
            let base = raw(&format!("{}RED\n\x1b[0m", "\x1b[31m".repeat(80)));
            let (_a, mut first) = fixture("public", true, limits());
            let shown = execute(&mut first, &base);
            let (_b, mut other) = fixture("public", true, limits());
            assert!(
                other
                    .dispatch(
                        "read_output",
                        &json!({"handle":shown.view.unwrap().raw_handle}),
                        &mut no_exec
                    )
                    .is_err()
            );
            assert_eq!(other.journal().last().unwrap().output_bytes, 0);
        }
        "exact_quotas" => {
            let base = raw(&format!("{}RED\n\x1b[0m", "\x1b[31m".repeat(80)));
            let mut budgets = limits();
            budgets.max_calls = 2;
            budgets.max_total_output_bytes = content(&base) * 2;
            let (_temp, mut file) = fixture("public", true, budgets);
            let shown = execute(&mut file, &base);
            let handle = shown.view.unwrap().raw_handle;
            let got = file
                .dispatch("read_output", &json!({"handle":handle}), &mut no_exec)
                .unwrap();
            assert_eq!(got["stdout"], base.stdout);
            assert!(file.halted());
            assert_eq!(
                file.journal().iter().map(|r| r.output_bytes).sum::<usize>(),
                content(&base) * 2
            );
            assert!(
                file.dispatch("read_output", &json!({"handle":handle}), &mut no_exec)
                    .is_err()
            );
        }
        _ => panic!("unknown authored guard case"),
    }
    json!({"fixture_id":id,"fidelity":"pass","outcome":"expected_refusal_verified"})
}
/// Integer basis points, signed, weighted by totals; no averaging or clamping.
pub fn summarize(rows: &[Value]) -> Value {
    let sum = |key: &str| rows.iter().map(|r| r[key].as_i64().unwrap()).sum::<i64>();
    let raw = sum("raw_content_bytes");
    let shown = sum("shown_content_bytes");
    let recalled = sum("recalled_content_bytes");
    let raw_json = sum("raw_json_bytes");
    let serialized = sum("serialized_bytes");
    json!({"raw_content_bytes":raw,"shown_content_bytes":shown,"recalled_content_bytes":recalled,
        "delivered_content_bytes":shown+recalled,"net_content_bytes":raw-shown-recalled,
        "content_reduction_basis_points":if raw==0 {None} else {Some((raw-shown-recalled)*10000/raw)},
        "raw_json_bytes":raw_json,"serialized_bytes":serialized,"net_json_bytes":raw_json-serialized,
        "json_reduction_basis_points":if raw_json==0 {None} else {Some((raw_json-serialized)*10000/raw_json)} })
}
pub fn report() -> Value {
    let corpus = router_adapter_api::strict_json_limited(FIXTURES, 262144).unwrap();
    assert_eq!(corpus["schema_version"], 1);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let mut ids = BTreeSet::new();
    let mut rows = vec![];
    let mut fallbacks = vec![];
    let mut refusals = vec![];
    let mut stops = vec![];
    for case in cases {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        match case["kind"].as_str().unwrap() {
            "source" => source_case(case, &mut rows),
            "output" => output_case(case, &mut rows, &mut fallbacks),
            _ => refusals.push(guard_case(case)),
        }
        if case["expected"]["truncated"] == true {
            stops.push(json!({"fixture_id":case["id"],"reason":if case["kind"]=="source" {"admitted_output_truncated"} else {"capture_incomplete"}}));
        }
    }
    let mut summaries = serde_json::Map::new();
    for mode in ["exact", "window", "view"] {
        let selected: Vec<_> = rows.iter().filter(|r| r["mode"] == mode).cloned().collect();
        summaries.insert(mode.into(), summarize(&selected));
    }
    let selected: Vec<_> = rows
        .iter()
        .filter(|r| r["mode"] != "exact")
        .cloned()
        .collect();
    json!({"schema_version":1,"corpus_sha256":content_digest(FIXTURES),
        "measurement_basis":"authored ToolResult JSON; fixed elapsed_ms=1000; deterministic test-only recall binding; excludes MCP envelope",
        "evidence":{"billed_tokens":"unknown","provider_cost":"unknown","qualified_pricing":"unknown","model_success":"unknown","native_linux_arm64":"not_run"},
        "rows":rows,"summary_by_mode":summaries,"combined_selection":summarize(&selected),"fallback_count":fallbacks.len(),"fallbacks":fallbacks,"stopped_fixtures":stops,"refusals":refusals})
}
pub fn bytes() -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(&report()).unwrap();
    bytes.push(b'\n');
    bytes
}
