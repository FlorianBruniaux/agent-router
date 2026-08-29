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
