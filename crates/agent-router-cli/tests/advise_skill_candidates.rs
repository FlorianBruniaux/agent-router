// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `advise --skill-candidates` (ADR 0055, issue #237). Candidates are declared
//! by an external router, never verified, and only trusted catalog skills
//! attach. Nothing is executed and no provider is contacted. Helpers are copied
//! on purpose: the existing advisory test files are edited by parallel changes.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const NOW: &str = "1000";
const OBJECTIVE: &str = "Add a dry-run flag to the backup script";

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
fn write(path: &Path, value: &Value) -> PathBuf {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_owned()
}
fn parsed(output: &[u8]) -> Value {
    serde_json::from_slice(output).unwrap_or_else(|error| panic!("one JSON result: {error}"))
}
fn error_line(code: &str) -> String {
    format!("{{\"schema_version\":1,\"error\":\"{code}\"}}\n")
}

/// Declared Codex host; `tools` is a declared capability when `tools` is true.
fn host(dir: &Path, tools: bool) -> PathBuf {
    write(
        &dir.join("host.json"),
        &json!({"id": "owner-codex", "harness": "codex",
            "supported_models": ["gpt-5.6-sol", "gpt-5.6-terra"],
            "supported_efforts": {"gpt-5.6-sol": ["medium", "high", "xhigh"],
                "gpt-5.6-terra": ["medium", "high", "xhigh"]},
            "capabilities": if tools { json!(["tools"]) } else { json!([]) }}),
    )
}

/// The shipped catalog plus synthetic implement skills `skill-a` to `skill-d`
/// and one untrusted skill.
fn catalog(dir: &Path) -> PathBuf {
    let mut value: Value =
        serde_json::from_slice(&fs::read(root("assets/catalogs/codex-core-v2.json")).unwrap())
            .unwrap();
    let skills = value["skills"].as_array_mut().unwrap();
    for id in ["skill-a", "skill-b", "skill-c", "skill-d"] {
        skills.push(json!({"id": id, "description": "Synthetic skill.",
            "task_kinds": ["implement"], "required_capabilities": [], "trust": "builtin"}));
    }
    skills.push(
        json!({"id": "skill-untrusted", "description": "Synthetic untrusted skill.",
        "task_kinds": ["implement"], "required_capabilities": [], "trust": "untrusted"}),
    );
    write(&dir.join("catalog.json"), &value)
}

fn document(dir: &Path, name: &str, source: &str, candidates: &[(&str, i64)]) -> PathBuf {
    write(
        &dir.join(name),
        &json!({"schema_version": "1", "source": source,
            "candidates": candidates.iter()
                .map(|(skill, score)| json!({"skill": skill, "score_micros": score}))
                .collect::<Vec<_>>()}),
    )
}

fn advise(dir: &Path, tools: bool, extra: &[&str]) -> Command {
    let mut cmd = command();
    cmd.args([
        "advise",
        "--text",
        OBJECTIVE,
        "--kind",
        "implement",
        "--risk",
        "medium",
        "--scope",
        "scripts",
        "--budget",
        "0:600000:200000",
    ])
    .arg("--catalog")
    .arg(catalog(dir))
    .arg("--host")
    .arg(host(dir, tools))
    .args(extra)
    .args(["--now", NOW, "--json"]);
    cmd
}

fn run(dir: &Path, tools: bool, candidates: &Path, extra: &[&str]) -> Value {
    let output = advise(dir, tools, extra)
        .arg("--skill-candidates")
        .arg(candidates)
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    parsed(&output)
}

fn row<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no row for {id}: {}", report["skills"]))
}
fn rows<'a>(report: &'a Value, id: &str) -> Vec<&'a Value> {
    report["skills"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == id)
        .collect()
}
fn stage_skills(report: &Value) -> Vec<Vec<String>> {
    report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| {
            stage["skills"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}
fn models(report: &Value) -> Vec<String> {
    report["decision"]["decision"]["selected"]["plan"]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["model"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn advise_suggests_a_library_skill_for_an_implement_task() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("lib-refactor", 900_000)],
    );
    let report = run(dir.path(), true, &file, &[]);
    let row = row(&report, "lib-refactor");
    assert_eq!(row["status"], "outside_catalog_suggested");
    assert_eq!(row["provenance"], "declared");
    assert_eq!(row["source"], "external-declared:owner-router");
    assert_eq!(row["score_micros"], 900_000);
    assert_eq!(row["stages"], json!([]));
    // Reported, never attached, and named for the human.
    assert!(stage_skills(&report).iter().all(Vec::is_empty));
    assert!(
        report["next_manual_action"]["summary"]
            .as_str()
            .unwrap()
            .contains("lib-refactor")
    );
    assert_eq!(report["executed"], false);
}

#[test]
fn the_document_digest_is_a_declared_fact() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("lib-refactor", 900_000)],
    );
    let report = run(dir.path(), true, &file, &[]);
    let fact = report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fact| fact["subject"] == "skill_candidates.document")
        .expect("document fact");
    assert_eq!(fact["provenance"], "declared");
    let digest = fact["value"]["sha256"].as_str().unwrap();
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn a_catalog_candidate_attaches_and_the_cap_keeps_two_per_stage() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[
            ("skill-a", 900_000),
            ("skill-b", 800_000),
            ("skill-c", 700_000),
        ],
    );
    let report = run(dir.path(), true, &file, &[]);
    for skills in stage_skills(&report) {
        assert_eq!(skills, ["skill-a", "skill-b"]);
    }
    assert_eq!(row(&report, "skill-a")["status"], "suggested");
    assert_eq!(
        row(&report, "skill-a")["stages"],
        json!(["plan", "execute", "verify"])
    );
    assert_eq!(row(&report, "skill-c")["status"], "not_attached");
    assert_eq!(row(&report, "skill-c")["stages"], json!([]));
}

#[test]
fn candidates_rank_operator_first_then_score_then_id() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[
            ("skill-a", 500_000),
            ("skill-b", 900_000),
            ("skill-c", 900_000),
        ],
    );
    let report = run(dir.path(), true, &file, &["--skill", "skill-d"]);
    for skills in stage_skills(&report) {
        assert_eq!(skills, ["skill-d", "skill-b"]);
    }
    let file = document(
        dir.path(),
        "tie.json",
        "owner-router",
        &[
            ("skill-d", 500_000),
            ("skill-b", 500_000),
            ("skill-c", 500_000),
        ],
    );
    for skills in stage_skills(&run(dir.path(), true, &file, &[])) {
        assert_eq!(skills, ["skill-b", "skill-c"]);
    }
}

#[test]
fn a_score_under_the_floor_is_reported_and_not_attached() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-a", 299_999), ("skill-b", 300_000)],
    );
    let report = run(dir.path(), true, &file, &[]);
    assert_eq!(row(&report, "skill-a")["status"], "below_floor");
    assert_eq!(row(&report, "skill-b")["status"], "suggested");
    for skills in stage_skills(&report) {
        assert_eq!(skills, ["skill-b"]);
    }
}

#[test]
fn advise_does_not_attach_untrusted_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-untrusted", 1_000_000)],
    );
    let report = run(dir.path(), true, &file, &[]);
    assert_eq!(row(&report, "skill-untrusted")["status"], "untrusted");
    assert!(stage_skills(&report).iter().all(Vec::is_empty));
}

#[test]
fn incompatible_kind_and_unknown_capability_are_named() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("critique-plan", 900_000), ("tdd", 900_000)],
    );
    let report = run(dir.path(), false, &file, &[]);
    assert_eq!(
        row(&report, "critique-plan")["status"],
        "incompatible_task_kind"
    );
    // A declared host that does not list `tools` leaves it UNKNOWN, not absent.
    assert_eq!(row(&report, "tdd")["status"], "capability_unknown");
    assert!(stage_skills(&report).iter().all(Vec::is_empty));
    let report = run(dir.path(), true, &file, &[]);
    assert_eq!(row(&report, "tdd")["status"], "suggested");
}

#[test]
fn an_operator_skill_wins_over_the_same_id_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-c", 900_000)],
    );
    let out = dir.path().join("advice");
    let report = run(
        dir.path(),
        true,
        &file,
        &[
            "--skill",
            "skill-c",
            "--output-directory",
            out.to_str().unwrap(),
        ],
    );
    let all = rows(&report, "skill-c");
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all.iter().any(|row| row["status"] == "operator_declared"));
    assert!(all.iter().any(|row| row["status"] == "suggested"));
    let request: Value =
        serde_json::from_slice(&fs::read(out.join("route-request.json")).unwrap()).unwrap();
    assert_eq!(
        request["skill_candidates"],
        json!([{"skill": "skill-c", "source": "operator-declared", "score_micros": 0}])
    );
}

#[test]
fn candidates_never_change_the_selected_models() {
    let dir = tempfile::tempdir().unwrap();
    let bare = parsed(
        &advise(dir.path(), true, &[])
            .assert()
            .code(0)
            .get_output()
            .stdout,
    );
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-a", 900_000), ("lib-x", 800_000)],
    );
    let loaded = run(dir.path(), true, &file, &["--skill", "skill-b"]);
    assert_eq!(models(&bare), models(&loaded));
    assert_eq!(
        bare["decision"]["decision"]["selected"]["plan"]["utility_micros"],
        loaded["decision"]["decision"]["selected"]["plan"]["utility_micros"]
    );
}

#[test]
fn stable_v1_keeps_its_rules_for_file_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-a", 100), ("skill-b", 800_000), ("skill-c", 700_000)],
    );
    let report = run(dir.path(), true, &file, &["--policy", "stable-v1"]);
    for skills in stage_skills(&report) {
        assert_eq!(skills, ["skill-a", "skill-b", "skill-c"]);
    }
    assert_eq!(row(&report, "skill-a")["status"], "suggested");
}

#[test]
fn the_two_best_outside_catalog_candidates_are_suggested() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[
            ("lib-a", 400_000),
            ("lib-b", 900_000),
            ("lib-c", 400_000),
            ("lib-d", 299_999),
        ],
    );
    let report = run(dir.path(), true, &file, &[]);
    assert_eq!(row(&report, "lib-b")["status"], "outside_catalog_suggested");
    assert_eq!(row(&report, "lib-a")["status"], "outside_catalog_suggested");
    assert_eq!(row(&report, "lib-c")["status"], "outside_catalog");
    assert_eq!(row(&report, "lib-d")["status"], "outside_catalog");
}

#[test]
fn candidate_order_in_file_does_not_change_decision_id() {
    let dir = tempfile::tempdir().unwrap();
    let rows = [
        ("skill-a", 500_000),
        ("lib-x", 700_000),
        ("skill-b", 900_000),
    ];
    let mut reversed = rows;
    reversed.reverse();
    let one = dir.path().join("one");
    let two = dir.path().join("two");
    let first = document(dir.path(), "a.json", "owner-router", &rows);
    let second = document(dir.path(), "b.json", "owner-router", &reversed);
    let a = run(
        dir.path(),
        true,
        &first,
        &["--output-directory", one.to_str().unwrap()],
    );
    let b = run(
        dir.path(),
        true,
        &second,
        &["--output-directory", two.to_str().unwrap()],
    );
    assert_eq!(a["decision"]["decision_id"], b["decision"]["decision_id"]);
    assert_eq!(
        fs::read(one.join("route-request.json")).unwrap(),
        fs::read(two.join("route-request.json")).unwrap()
    );
}

#[test]
fn the_saved_request_replays_through_route_without_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-a", 900_000), ("lib-x", 800_000)],
    );
    let out = dir.path().join("advice");
    let report = run(
        dir.path(),
        true,
        &file,
        &["--output-directory", out.to_str().unwrap()],
    );
    let request: Value =
        serde_json::from_slice(&fs::read(out.join("route-request.json")).unwrap()).unwrap();
    let sources: Vec<_> = request["skill_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["external-declared:owner-router"; 2]);
    fs::remove_file(&file).unwrap();
    let replay = command()
        .arg("route")
        .arg("--request")
        .arg(out.join("route-request.json"))
        .args(["--policy", "stable-v2"])
        .arg("--catalog")
        .arg(dir.path().join("catalog.json"))
        .args(["--now", NOW, "--json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    assert_eq!(parsed(&replay), report["decision"]);
}

#[test]
fn identical_inputs_give_identical_reports() {
    let dir = tempfile::tempdir().unwrap();
    let file = document(
        dir.path(),
        "c.json",
        "owner-router",
        &[("skill-a", 900_000)],
    );
    let once = advise(dir.path(), true, &[])
        .arg("--skill-candidates")
        .arg(&file)
        .output()
        .unwrap();
    let twice = advise(dir.path(), true, &[])
        .arg("--skill-candidates")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(once.stdout, twice.stdout);
}

fn refused(dir: &Path, file: &Path, code: &str) {
    advise(dir, true, &[])
        .arg("--skill-candidates")
        .arg(file)
        .assert()
        .code(2)
        .stderr(error_line(code));
}

#[test]
fn advise_refuses_more_than_256_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let rows: Vec<_> = (0..257).map(|i| (format!("lib-{i:03}"), 400_000)).collect();
    let borrowed: Vec<_> = rows
        .iter()
        .map(|(id, score)| (id.as_str(), *score))
        .collect();
    refused(
        dir.path(),
        &document(dir.path(), "c.json", "owner-router", &borrowed),
        "advisory.skill_candidates.too_many",
    );
    let ok: Vec<_> = borrowed.into_iter().take(256).collect();
    run(
        dir.path(),
        true,
        &document(dir.path(), "ok.json", "owner-router", &ok),
        &[],
    );
}

#[test]
fn advise_refuses_invalid_candidate_id() {
    let dir = tempfile::tempdir().unwrap();
    for id in [
        "../escape",
        "has space",
        "_lead",
        "a\u{e9}",
        &"x".repeat(97),
    ] {
        refused(
            dir.path(),
            &document(dir.path(), "c.json", "owner-router", &[(id, 400_000)]),
            "advisory.skill_candidate.invalid_id",
        );
    }
}

#[test]
fn advise_refuses_reserved_source_label() {
    let dir = tempfile::tempdir().unwrap();
    for source in [
        "operator-declared",
        "external-declared:x",
        "Owner",
        "has space",
        "",
    ] {
        let file = document(dir.path(), "c.json", source, &[("lib-x", 400_000)]);
        let expected = if source.is_empty() {
            "retrieval.source.empty"
        } else {
            "advisory.skill_candidates.source_invalid"
        };
        refused(dir.path(), &file, expected);
    }
    refused(
        dir.path(),
        &document(
            dir.path(),
            "long.json",
            &"a".repeat(65),
            &[("lib-x", 400_000)],
        ),
        "advisory.skill_candidates.source_invalid",
    );
    refused(
        dir.path(),
        &document(dir.path(), "empty.json", "Owner", &[]),
        "advisory.skill_candidates.source_invalid",
    );
}

#[test]
fn malformed_oversized_and_missing_documents_get_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.json");
    fs::write(&bad, b"{not json").unwrap();
    refused(dir.path(), &bad, "retrieval.json.invalid");
    let big = dir.path().join("big.json");
    fs::write(&big, vec![b' '; 1024 * 1024 + 1]).unwrap();
    refused(dir.path(), &big, "retrieval.document.too_large");
    refused(
        dir.path(),
        &write(
            &dir.path().join("dup.json"),
            &json!({"schema_version": "1", "source": "owner-router", "candidates": [
                {"skill": "lib-x", "score_micros": 1}, {"skill": "lib-x", "score_micros": 2}]}),
        ),
        "retrieval.candidate.duplicate_skill",
    );
    refused(
        dir.path(),
        &write(
            &dir.path().join("extra.json"),
            &json!({"schema_version": "1", "source": "owner-router", "candidates": [], "x": 1}),
        ),
        "retrieval.json.invalid",
    );
    advise(dir.path(), true, &[])
        .arg("--skill-candidates")
        .arg(dir.path().join("missing.json"))
        .assert()
        .code(2);
}
