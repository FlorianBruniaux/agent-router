use std::{fs, path::PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn workspace_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CLI crate is nested beneath the workspace root")
        .join(path)
}

fn command() -> Command {
    Command::cargo_bin("agent-router").expect("agent-router binary exists")
}

fn route_args(request: PathBuf, catalog: PathBuf) -> Vec<String> {
    vec![
        "route".into(),
        "--request".into(),
        request.display().to_string(),
        "--catalog".into(),
        catalog.display().to_string(),
        "--json".into(),
    ]
}

fn catalog_path() -> PathBuf {
    workspace_path("assets/catalogs/codex-core-v1.json")
}

fn implementation_request() -> PathBuf {
    workspace_path("tests/fixtures/requests/codex-implementation.json")
}

fn no_route_request() -> PathBuf {
    workspace_path("tests/fixtures/requests/no-compatible-plan.json")
}

#[test]
fn help_is_rendered_by_clap_on_stdout_with_success() {
    let mut command = command();
    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: agent-router"))
        .stdout(predicate::str::contains("route"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn route_help_is_rendered_by_clap_on_stdout_with_success() {
    let mut command = command();
    command
        .args(["route", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: agent-router route"))
        .stdout(predicate::str::contains("--catalog <CATALOG>"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn version_is_rendered_by_clap_on_stdout_with_success() {
    let mut command = command();
    command
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("agent-router 0.1.0"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn invalid_command_remains_an_opaque_syntax_error() {
    let mut command = command();
    command
        .arg("not-a-command")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: cli.invalid\n");
}

#[test]
fn route_emits_versioned_json_without_mutating_home() {
    let temp_home = TempDir::new().unwrap();
    let mut command = command();
    command
        .env("HOME", temp_home.path())
        .args(route_args(implementation_request(), catalog_path()))
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": \"1\""))
        .stderr(predicate::str::is_empty());

    assert_eq!(fs::read_dir(temp_home.path()).unwrap().count(), 0);
}

#[test]
fn malformed_request_returns_code_two_and_keeps_its_body_off_stderr() {
    let temporary = TempDir::new().unwrap();
    let request = temporary.path().join("request.json");
    let secret = "DO_NOT_LEAK_REQUEST_BODY";
    fs::write(
        &request,
        format!("{{\"task\":{{\"objective\":\"{secret}\"}}}}"),
    )
    .unwrap();

    let mut command = command();
    command
        .args(route_args(request, catalog_path()))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("error: request.json.invalid\n"))
        .stderr(predicate::str::contains(secret).not());
}

#[test]
fn invalid_embedded_skill_candidates_return_request_invalid() {
    let cases = [
        ("negative", -1, "valid-source", 1),
        ("above-max", 1_000_001, "valid-source", 1),
        ("i64-max", i64::MAX, "valid-source", 1),
        ("empty-source", 500_000, "", 1),
        ("too-many", 500_000, "valid-source", 257),
    ];

    for (name, score, source, count) in cases {
        let temporary = TempDir::new().unwrap();
        let request = temporary.path().join(format!("{name}.json"));
        write_request_candidates(&request, score, source, count);

        let mut command = command();
        command
            .args(route_args(request, catalog_path()))
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr("error: request.invalid\n");
    }
}

#[test]
fn combined_routing_work_limit_returns_request_invalid() {
    let temporary = TempDir::new().unwrap();
    let request = temporary.path().join("work-limit-request.json");
    let catalog = temporary.path().join("work-limit-catalog.json");
    write_work_limit_fixture(&request, &catalog, 32, 256);

    command()
        .args(route_args(request, catalog))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: request.invalid\n");
}

#[test]
fn oversized_request_is_rejected_before_json_parsing() {
    let temporary = TempDir::new().unwrap();
    let request = temporary.path().join("oversized-request.json");
    fs::write(&request, vec![b' '; 1024 * 1024 + 1]).unwrap();

    command()
        .args(route_args(request, catalog_path()))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: request.too_large\n");
}

#[test]
fn oversized_catalog_is_rejected_before_json_parsing() {
    let temporary = TempDir::new().unwrap();
    let catalog = temporary.path().join("oversized-catalog.json");
    fs::write(&catalog, vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();

    command()
        .args(route_args(implementation_request(), catalog))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: catalog.too_large\n");
}

#[test]
fn oversized_decision_is_rejected_before_json_parsing() {
    let temporary = TempDir::new().unwrap();
    let decision = temporary.path().join("oversized-decision.json");
    fs::write(&decision, vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();

    command()
        .args(["explain", "--decision", decision.to_str().unwrap()])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: decision.too_large\n");
}

#[test]
fn invalid_catalog_returns_code_two_and_a_single_concise_error_line() {
    let mut command = command();
    command
        .args(route_args(
            implementation_request(),
            workspace_path("tests/fixtures/catalogs/invalid-alias.json"),
        ))
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr("error: catalog.invalid\n");
}

#[test]
fn no_route_has_a_distinct_exit_code_and_still_emits_json() {
    let mut command = command();
    command
        .args(route_args(no_route_request(), catalog_path()))
        .assert()
        .code(3)
        .stdout(predicate::str::contains("\"no_route\""))
        .stderr(predicate::str::is_empty());
}

#[test]
fn route_stdout_is_byte_exactly_the_pinned_implementation_decision() {
    let golden = fs::read(workspace_path(
        "tests/fixtures/decisions/codex-implementation.json",
    ))
    .expect("pinned implementation decision exists");

    let output = command()
        .args(route_args(implementation_request(), catalog_path()))
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stderr, b"");
    assert_eq!(output.stdout, golden);
}

#[test]
fn explain_selected_prints_the_plan_without_request_or_skill_contents() {
    let temporary = TempDir::new().unwrap();
    let decision = temporary.path().join("selected.json");
    let output = command()
        .args(route_args(implementation_request(), catalog_path()))
        .output()
        .unwrap();
    assert!(output.status.success());
    fs::write(&decision, output.stdout).unwrap();

    let mut command = command();
    command
        .args(["explain", "--decision", decision.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Preset: plan-execute-verify"))
        .stdout(predicate::str::contains(
            "Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high",
        ))
        .stdout(predicate::str::contains("Implement a bounded parser change").not())
        .stdout(predicate::str::contains("red-green-refactor").not())
        .stderr(predicate::str::is_empty());
}

#[test]
fn explain_no_route_prints_each_exclusion_without_the_request_body() {
    let temporary = TempDir::new().unwrap();
    let decision = temporary.path().join("no-route.json");
    let output = command()
        .args(route_args(no_route_request(), catalog_path()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    fs::write(&decision, output.stdout).unwrap();

    let mut command = command();
    command
        .args(["explain", "--decision", decision.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No route"))
        .stdout(predicate::str::contains("Stage: plan"))
        .stdout(predicate::str::contains("Model: gpt-5.6-terra"))
        .stdout(predicate::str::contains("Code: model.stage.unsupported"))
        .stdout(predicate::str::contains("Evidence:"))
        .stdout(predicate::str::contains("Implement without an available planning model").not())
        .stderr(predicate::str::is_empty());
}

fn write_request_candidates(path: &std::path::Path, score: i64, source: &str, count: usize) {
    let mut request: serde_json::Value =
        serde_json::from_slice(&fs::read(implementation_request()).unwrap()).unwrap();
    let candidate = serde_json::json!({
        "skill": "tdd",
        "source": source,
        "score_micros": score
    });
    request["skill_candidates"] =
        serde_json::Value::Array((0..count).map(|_| candidate.clone()).collect());
    fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
}

fn write_work_limit_fixture(
    request_path: &std::path::Path,
    catalog_path: &std::path::Path,
    model_count: usize,
    skill_count: usize,
) {
    let model_ids = (0..model_count)
        .map(|index| format!("provider-work-{index:02}"))
        .collect::<Vec<_>>();
    let skill_ids = (0..skill_count)
        .map(|index| format!("work-skill-{index:03}"))
        .collect::<Vec<_>>();
    let models = model_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "stages": ["plan", "execute", "verify"],
                "efforts": ["medium", "high"],
                "capabilities": ["tools"],
                "quality_micros": 800000,
                "cost_micros": 0,
                "latency_micros": 0,
                "source": "work-limit-fixture"
            })
        })
        .collect::<Vec<_>>();
    let skills = skill_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "description": "work limit fixture",
                "task_kinds": ["implement"],
                "required_capabilities": ["tools"],
                "trust": "builtin"
            })
        })
        .collect::<Vec<_>>();
    let supported_efforts = model_ids
        .iter()
        .map(|id| (id.clone(), serde_json::json!(["medium", "high"])))
        .collect::<serde_json::Map<_, _>>();
    let skill_candidates = skill_ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "skill": id,
                "source": "work-limit-fixture",
                "score_micros": 1000000
            })
        })
        .collect::<Vec<_>>();

    let catalog = serde_json::json!({
        "schema_version": 1,
        "version": "work-limit-v1",
        "models": models,
        "skills": skills
    });
    let request = serde_json::json!({
        "task": {
            "objective": "reject excessive combined routing work",
            "kind": "implement",
            "risk": "medium",
            "required_capabilities": ["tools"],
            "acceptance": ["work budget is authoritative"]
        },
        "host": {
            "id": "work-limit-host",
            "harness": "codex",
            "supported_models": model_ids,
            "supported_efforts": supported_efforts,
            "capabilities": ["tools"]
        },
        "skill_candidates": skill_candidates,
        "capability_knowledge": {}
    });
    fs::write(catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    fs::write(request_path, serde_json::to_vec(&request).unwrap()).unwrap();
}

#[test]
fn explain_escapes_terminal_controls_and_bidi_marks_in_untrusted_decision_fields() {
    let temporary = TempDir::new().unwrap();
    let decision = temporary.path().join("no-route.json");
    let output = command()
        .args(route_args(no_route_request(), catalog_path()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let mut value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    value["decision_id"] = "id\u{1b}[2Jx".into();
    let exclusion = &mut value["decision"]["no_route"]["exclusions"][0];
    exclusion["code"] = "c\nfake: line\u{202e}".into();
    exclusion["evidence"] = "e\u{200f}v\u{61c}i\u{200e}d".into();
    fs::write(&decision, serde_json::to_vec(&value).unwrap()).unwrap();

    let output = command()
        .args(["explain", "--decision", decision.to_str().unwrap()])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    for raw in ['\u{1b}', '\u{202e}', '\u{200f}', '\u{61c}', '\u{200e}'] {
        assert!(!text.contains(raw), "{raw:?} leaked in {text:?}");
    }
    assert!(text.contains("id\\u{1b}[2Jx"), "{text}");
    assert!(text.contains("c\\nfake: line\\u{202e}"), "{text}");
    assert!(text.contains("e\\u{200f}v\\u{61c}i\\u{200e}d"), "{text}");
    assert!(!text.lines().any(|line| line.starts_with("fake:")));
}

#[cfg(unix)]
#[test]
fn a_broken_stdout_pipe_fails_without_panicking_or_reporting_success() {
    use std::process::{Command as Std, Stdio};
    let bin = assert_cmd::cargo::cargo_bin("agent-router");
    let args = route_args(implementation_request(), catalog_path());

    // A closed stdout (`>&-`) is not testable here: the Rust runtime reopens fds
    // 0-2 on /dev/null at startup, so the descriptor is valid and writes succeed.
    // A pipe whose reader already exited: the write end stays open here, so the
    // first write fails with EPIPE deterministically.
    let mut reader = Std::new("true").stdin(Stdio::piped()).spawn().unwrap();
    let broken = reader.stdin.take().unwrap();
    reader.wait().unwrap();
    let output = Std::new(&bin)
        .args(&args)
        .stdout(Stdio::from(broken))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(4), "{stderr}");
    assert!(stderr.contains("route.internal"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
