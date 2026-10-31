//! Advisory routing of one normalized task. Nothing is executed, no provider is
//! contacted and only an explicitly chosen private directory is written.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

const NOW: &str = "10";

fn command() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.timeout(Duration::from_secs(20));
    command
}
fn root(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}
fn catalog() -> PathBuf {
    root("assets/catalogs/codex-core-v2.json")
}
fn write(path: &Path, value: &Value) -> PathBuf {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_owned()
}
fn parsed(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected one JSON result: {error}; status={:?}; stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
/// A complete task as `task-guide` exports it, different from every route fixture.
fn task(required_capabilities: Value) -> Value {
    json!({
        "objective": "Add a dry-run flag to the backup script and document it",
        "kind": "implement",
        "deliverables": ["Patch with the flag and its usage text"],
        "constraints": ["Keep the current default behavior"],
        "scope": ["scripts", "docs"],
        "risk": "medium",
        "sensitivity": "internal",
        "budget": {"max_cost_microusd": 0, "max_elapsed_ms": 600000, "max_tokens": 200000},
        "allowed_mutations": ["workspace_write"],
        "dependencies": [],
        "acceptance": ["The dry-run flag prints planned copies without writing"],
        "required_capabilities": required_capabilities
    })
}
fn declared_host(models: &[&str], capabilities: Value) -> Value {
    let efforts: BTreeMap<_, _> = models
        .iter()
        .map(|model| (*model, json!(["medium", "high", "xhigh"])))
        .collect();
    json!({
        "id": "owner-codex",
        "harness": "codex",
        "supported_models": models,
        "supported_efforts": efforts,
        "capabilities": capabilities
    })
}
/// Explicit parent facts, as `task-guide` collects them for a write task.
fn risk_context(dir: &Path) -> PathBuf {
    write(
        &dir.join("risk-context.json"),
        &json!({"schema_version": 1, "environment": "local",
            "reversibility": "reversible", "operations": ["workspace_change"]}),
    )
}
fn stage_models(report: &Value) -> Vec<(String, String)> {
    report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| {
            (
                stage["model"].as_str().unwrap().to_owned(),
                stage["effort"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}
fn fact<'a>(report: &'a Value, subject: &str) -> &'a Value {
    report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fact| fact["subject"] == subject)
        .unwrap_or_else(|| panic!("missing fact {subject}: {report:#}"))
}

#[cfg(feature = "codex")]
fn discovered_snapshot(dir: &Path) -> PathBuf {
    let standard = ["low", "medium", "high", "xhigh", "max"];
    let extended = ["low", "medium", "high", "xhigh", "max", "ultra"];
    let models: Vec<_> = [
        ("codex-auto-review", standard.as_slice()),
        ("gpt-5.6-luna", standard.as_slice()),
        ("gpt-5.6-sol", extended.as_slice()),
        ("gpt-5.6-terra", extended.as_slice()),
        ("gpt-6-astra", extended.as_slice()),
        ("gpt-6-luna", standard.as_slice()),
        ("gpt-6-sol", extended.as_slice()),
        ("gpt-6.1-sol", extended.as_slice()),
        ("gpt-reserve", standard.as_slice()),
    ]
    .into_iter()
    .map(|(slug, efforts)| {
        json!({"slug": slug, "supported_reasoning_levels":
            efforts.iter().map(|effort| json!({"effort": effort})).collect::<Vec<_>>()})
    })
    .collect();
    let cache = write(
        &dir.join("cache.json"),
        &json!({"fetched_at": "1970-01-01T00:00:00Z", "client_version": "0.163.0", "models": models}),
    );
    let observed = command()
        .args(["inspect", "--host", "codex", "--model-cache"])
        .arg(&cache)
        .args(["--observed-version", "0.163.0", "--now", NOW, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let path = dir.join("observation.json");
    fs::write(&path, observed).unwrap();
    path
}

#[test]
fn nominal_task_gets_a_recommendation_and_a_manual_action_per_phase() {
    let dir = tempfile::tempdir().unwrap();
    let task = write(&dir.path().join("task.json"), &task(json!(["tools"])));
    let host = write(
        &dir.path().join("host.json"),
        &declared_host(&["gpt-5.6-sol", "gpt-5.6-terra"], json!(["tools"])),
    );
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .args(["--skill", "tdd", "--now", NOW, "--json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["mode"], "advisory");
    assert_eq!(report["executed"], false);
    assert_eq!(report["status"], "recommended");
    assert_eq!(
        stage_models(&report),
        [
            ("gpt-5.6-sol".into(), "high".into()),
            ("gpt-5.6-terra".into(), "medium".into()),
            ("gpt-5.6-terra".into(), "high".into()),
        ]
    );
    let action = &report["next_manual_action"];
    assert_eq!(action["kind"], "select_per_phase");
    let phases = action["phases"].as_array().unwrap();
    assert_eq!(phases.len(), 3);
    assert_eq!(phases[0]["stage"], "plan");
    assert_eq!(phases[0]["model"], "gpt-5.6-sol");
    assert_eq!(phases[0]["effort"], "high");
    assert_eq!(phases[1]["skills"], json!(["tdd"]));
    assert!(
        action["summary"]
            .as_str()
            .unwrap()
            .contains("Nothing was executed")
    );
    // The catalog rating is a declared policy prior, never a measurement.
    assert_eq!(fact(&report, "catalog.models")["provenance"], "declared");
    // Facts the advisory route cannot enforce are reported, never dropped.
    let mapping = report["mapping"].as_array().unwrap();
    for field in ["scope", "budget", "allowed_mutations", "sensitivity"] {
        let row = mapping
            .iter()
            .find(|row| row["field"] == field)
            .unwrap_or_else(|| panic!("{field} missing from mapping"));
        assert_eq!(row["handling"], "not_enforced_by_advisory_routing");
        assert!(!row["value"].is_null(), "{field} value dropped");
    }
    for field in [
        "objective",
        "kind",
        "risk",
        "acceptance",
        "required_capabilities",
    ] {
        let row = mapping.iter().find(|row| row["field"] == field).unwrap();
        assert_eq!(row["handling"], "routed");
    }
}

#[cfg(feature = "codex")]
#[test]
fn unknown_tool_support_abstains_and_names_the_fact_that_would_change_it() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = discovered_snapshot(dir.path());
    let task = write(&dir.path().join("task.json"), &task(json!(["tools"])));
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host-snapshot")
        .arg(&snapshot)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(report["status"], "no_recommendation");
    assert!(report["decision"]["decision"]["no_route"].is_object());
    let tools = fact(&report, "host.capability.tools");
    assert_eq!(tools["provenance"], "unknown");
    assert_eq!(fact(&report, "host.models")["provenance"], "discovered");
    let action = &report["next_manual_action"];
    assert_eq!(action["kind"], "no_recommendation");
    assert!(action["phases"].as_array().unwrap().is_empty());
    let changes = action["would_change"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|change| change["code"] == "host.capability.unknown"
                && change["capability"] == "tools"),
        "{action:#}"
    );
}

#[cfg(feature = "codex")]
#[test]
fn task_without_required_capability_says_that_no_capability_was_checked() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = discovered_snapshot(dir.path());
    let task = write(&dir.path().join("task.json"), &task(json!([])));
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host-snapshot")
        .arg(&snapshot)
        .args(["--skill", "tdd", "--now", NOW, "--json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(report["status"], "recommended");
    assert_eq!(
        fact(&report, "host.capability.tools")["provenance"],
        "unknown"
    );
    // Without verified or declared tools, the tools-only skill is not suggested.
    assert_eq!(report["skills"][0]["status"], "capability_not_supported");
    assert!(
        report["next_manual_action"]["summary"]
            .as_str()
            .unwrap()
            .contains("no host capability was checked")
    );
    // An unmodified observation is bound into the decision, as with `route`.
    assert!(report["decision"]["host_snapshot_hash"].is_string());
}

#[test]
fn unknown_model_identifier_is_refused_instead_of_guessed() {
    let dir = tempfile::tempdir().unwrap();
    let task = write(&dir.path().join("task.json"), &task(json!([])));
    let host = write(
        &dir.path().join("host.json"),
        &declared_host(&["gpt-5.6-sol", "gpt-7-nova"], json!([])),
    );
    command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(2)
        .stdout("")
        .stderr("{\"schema_version\":1,\"error\":\"advisory.model.unknown\"}\n");
    // A short alias is not a canonical identifier either.
    let alias = write(
        &dir.path().join("alias.json"),
        &declared_host(&["sol"], json!([])),
    );
    command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&alias)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(2)
        .stdout("");
}

#[test]
fn absent_skill_is_reported_and_never_suggested() {
    let dir = tempfile::tempdir().unwrap();
    let task = write(&dir.path().join("task.json"), &task(json!(["tools"])));
    let host = write(
        &dir.path().join("host.json"),
        &declared_host(&["gpt-5.6-sol", "gpt-5.6-terra"], json!(["tools"])),
    );
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .args([
            "--skill",
            "imaginary-refactor",
            "--skill",
            "security-guardian",
            "--now",
            NOW,
            "--json",
        ])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let report = parsed(&output);
    let text = serde_json::to_string(&report["next_manual_action"]).unwrap();
    assert!(!text.contains("imaginary-refactor"), "{text}");
    assert!(!text.contains("security-guardian"), "{text}");
    for stage in report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
    {
        assert_eq!(stage["skills"], json!([]));
    }
    let skills = report["skills"].as_array().unwrap();
    let status = |id: &str| {
        skills
            .iter()
            .find(|skill| skill["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing"))["status"]
            .clone()
    };
    assert_eq!(status("imaginary-refactor"), "absent_from_catalog");
    // Present in the catalog, but declared for review and security tasks only.
    assert_eq!(status("security-guardian"), "incompatible_task_kind");
}

#[cfg(feature = "codex")]
#[test]
fn declared_capability_is_labelled_declared_and_unbinds_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = discovered_snapshot(dir.path());
    let task = write(&dir.path().join("task.json"), &task(json!(["tools"])));
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host-snapshot")
        .arg(&snapshot)
        .args([
            "--declare-capability",
            "tools",
            "--skill",
            "tdd",
            "--now",
            NOW,
            "--json",
        ])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(report["status"], "recommended");
    let tools = fact(&report, "host.capability.tools");
    assert_eq!(tools["provenance"], "declared");
    assert_eq!(tools["value"], "supported");
    assert_eq!(fact(&report, "host.models")["provenance"], "discovered");
    assert_eq!(fact(&report, "host.snapshot")["provenance"], "verified");
    // A declaration is not snapshot evidence, so the decision is not bound to it.
    assert!(report["decision"]["host_snapshot_hash"].is_null());
    assert!(
        report["next_manual_action"]["summary"]
            .as_str()
            .unwrap()
            .contains("declared")
    );
    // Network cannot be declared into an advisory route.
    command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host-snapshot")
        .arg(&snapshot)
        .args(["--declare-capability", "network", "--now", NOW, "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"advisory.declaration.network\"}\n");
}

#[test]
fn incomplete_task_asks_questions_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut incomplete = task(json!([]));
    incomplete["budget"] = Value::Null;
    let task = write(&dir.path().join("task.json"), &incomplete);
    let host = write(
        &dir.path().join("host.json"),
        &declared_host(&["gpt-5.6-sol"], json!([])),
    );
    let output_directory = dir.path().join("advice");
    let output = command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .arg("--output-directory")
        .arg(&output_directory)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(3)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(report["status"], "needs_clarification");
    assert!(report["decision"].is_null());
    assert!(
        report["questions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|question| question["code"] == "task.budget.unknown")
    );
    assert_eq!(report["next_manual_action"]["kind"], "answer_questions");
    assert!(!output_directory.exists());
}

fn listing(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            entries.extend(listing(&path));
        } else {
            entries.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    entries.sort();
    entries
}

#[cfg(unix)]
#[test]
fn writes_only_the_explicit_private_directory_and_never_the_target_repository() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let repository = dir.path().join("repository");
    fs::create_dir_all(repository.join("scripts")).unwrap();
    fs::write(repository.join("scripts/backup.sh"), "#!/bin/sh\n").unwrap();
    let inputs = dir.path().join("inputs");
    fs::create_dir(&inputs).unwrap();
    let task = write(&inputs.join("task.json"), &task(json!(["tools"])));
    let host = write(
        &inputs.join("host.json"),
        &declared_host(&["gpt-5.6-sol", "gpt-5.6-terra"], json!(["tools"])),
    );
    let before = listing(&repository);
    let inputs_before = listing(&inputs);
    let advice = dir.path().join("advice");
    let output = command()
        .current_dir(&repository)
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .arg("--output-directory")
        .arg(&advice)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let report = parsed(&output);
    assert_eq!(listing(&repository), before);
    assert_eq!(listing(&inputs), inputs_before);
    assert_eq!(
        fs::metadata(&advice).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let mut names: Vec<_> = fs::read_dir(&advice)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "advice.json",
            "decision.json",
            "risk-context.json",
            "route-request.json",
            "task.json"
        ]
    );
    for name in &names {
        let mode = fs::metadata(advice.join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(advice.join("advice.json")).unwrap()).unwrap();
    assert_eq!(saved["status"], report["status"]);
    assert_eq!(saved["decision"], report["decision"]);
    let request: Value =
        serde_json::from_slice(&fs::read(advice.join("route-request.json")).unwrap()).unwrap();
    // The saved request replays through the existing `route` command.
    let replay = command()
        .arg("route")
        .arg("--request")
        .arg(advice.join("route-request.json"))
        .arg("--catalog")
        .arg(catalog())
        .args(["--now", NOW, "--json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let replay: Value = serde_json::from_slice(&replay).unwrap();
    assert_eq!(replay, report["decision"]);
    assert_eq!(request["task"]["kind"], "implement");
    // An existing destination is never reused.
    command()
        .args(["advise", "--task"])
        .arg(&task)
        .arg("--risk-context")
        .arg(risk_context(dir.path()))
        .arg("--catalog")
        .arg(catalog())
        .arg("--host")
        .arg(&host)
        .arg("--output-directory")
        .arg(&advice)
        .args(["--now", NOW, "--json"])
        .assert()
        .code(2)
        .stderr("{\"schema_version\":1,\"error\":\"advisory.output\"}\n");
}

#[test]
fn identical_inputs_give_identical_reports() {
    let dir = tempfile::tempdir().unwrap();
    let task = write(&dir.path().join("task.json"), &task(json!(["tools"])));
    let host = write(
        &dir.path().join("host.json"),
        &declared_host(&["gpt-5.6-sol", "gpt-5.6-terra"], json!(["tools"])),
    );
    let context = risk_context(dir.path());
    let run = || {
        command()
            .args(["advise", "--task"])
            .arg(&task)
            .arg("--risk-context")
            .arg(&context)
            .arg("--catalog")
            .arg(catalog())
            .arg("--host")
            .arg(&host)
            .args(["--skill", "tdd", "--skill", "tdd", "--now", NOW, "--json"])
            .assert()
            .code(0)
            .get_output()
            .stdout
            .clone()
    };
    assert_eq!(run(), run());
}
