#[path = "../../router-policy/tests/support/asset_fixture.rs"]
mod asset_fixture;

use assert_cmd::Command;
use router_adapter_api::{
    HostObservation, SnapshotValidation, ValidatedHostObservation, content_digest,
};
use router_catalog::Catalog;
use router_policy::PhaseExecutionScope;
use router_retrieval::{MetadataDocument, MetadataEnvelope};
use router_types::{PhaseRouteRequest, SkillId};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Output};

const BODY: &str = "PRIVATE_ASSET_BODY_CANARY. Inspect the bounded source and return evidence.";

fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
struct Fixture {
    directory: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut request: PhaseRouteRequest = serde_json::from_slice(include_bytes!(
            "../../../fixtures/examples/phase-request.json"
        ))
        .unwrap();
        for candidate in &mut request.candidates {
            candidate.estimated_tokens = 900;
            candidate
                .skills
                .insert(SkillId::parse("fixture-skill").unwrap());
        }
        let observation: HostObservation = serde_json::from_slice(include_bytes!(
            "../../../fixtures/examples/phase-snapshot.json"
        ))
        .unwrap();
        let snapshot = ValidatedHostObservation::from_observation(
            observation,
            SnapshotValidation {
                now_unix_seconds: 100,
                max_age_seconds: 100,
            },
        )
        .unwrap();
        let mut catalog: Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/examples/phase-catalog.json"
        ))
        .unwrap();
        catalog["skills"] = json!([{"id":"fixture-skill","description":"synthetic fixture","task_kinds":["explore"],"required_capabilities":[],"trust":"trusted_local"}]);
        let catalog = Catalog::from_json(&serde_json::to_vec(&catalog).unwrap()).unwrap();
        let inputs = asset_fixture::inputs(
            &request,
            &snapshot,
            &catalog,
            PhaseExecutionScope {
                workspace: "/fixture/workspace".into(),
                source_state: "a".repeat(40),
                scope: vec!["src".into()],
            },
            &BTreeMap::from([(SkillId::parse("fixture-skill").unwrap(), BODY.into())]),
            512,
        );
        write(&root.join("context.json"), &request);
        write(&root.join("snapshot.json"), snapshot.observation());
        fs::write(root.join("catalog.json"), catalog.canonical_json().unwrap()).unwrap();
        write(&root.join("contracts.json"), inputs.contracts.document());
        write(
            &root.join("availability.json"),
            inputs.availability.document(),
        );
        write(&root.join("policy.json"), &inputs.policy);
        write(&root.join("requests.json"), &inputs.requests);
        let metadata:MetadataDocument=serde_json::from_value(json!({"schema_version":"1","catalog_version":catalog.version(),"assets":[
            {"id":"fixture-skill","kind":"skill","trust":"trusted_local","description":"review target","body":{"path":"skill.txt","sha256":content_digest(BODY.as_bytes())}},
            {"id":"never-selected","kind":"skill","trust":"trusted_local","description":"absent body must never be read","body":{"path":"absent.txt","sha256":"d".repeat(64)}}
        ]})).unwrap();
        let metadata = MetadataEnvelope::new(metadata, &catalog.content_hash().unwrap()).unwrap();
        write(&root.join("metadata.json"), &metadata);
        fs::write(root.join("skill.txt"), BODY).unwrap();
        write(
            &root.join("authority.json"),
            &json!({"schema_version":1,"contracts":{"path":"contracts.json","expected_hash":inputs.contracts.hash()},
            "availability":{"path":"availability.json","expected_hash":inputs.availability.hash()},"policy":"policy.json","requests":"requests.json",
            "execution_scope":inputs.execution_scope,"approved_review_digests":inputs.approved_review_digests,"metadata":"metadata.json","body_root":"."}),
        );
        Self { directory }
    }
    fn phase(&self, command: &mut Command) {
        command
            .arg("--context")
            .arg(self.directory.path().join("context.json"))
            .arg("--host-snapshot")
            .arg(self.directory.path().join("snapshot.json"))
            .arg("--catalog")
            .arg(self.directory.path().join("catalog.json"))
            .args(["--now", "100", "--json"]);
    }
    fn review(&self, skills: &[&str]) -> Output {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
        command
            .args(["assets", "review", "--manifest"])
            .arg(self.directory.path().join("authority.json"));
        self.phase(&mut command);
        for skill in skills {
            command.args(["--skill", skill]);
        }
        command.output().unwrap()
    }
    fn change(&self, file: &str, change: impl FnOnce(&mut Value)) {
        let path = self.directory.path().join(file);
        let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut value);
        write(&path, &value);
    }
}

#[test]
fn explicit_review_projects_selected_asset_without_body_or_activation() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.directory.path().join("authority.json")).unwrap();
    let output = fixture.review(&["fixture-skill"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["approved"], false);
    assert_eq!(json["executed"], false);
    assert_eq!(json["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(json["reviews"][0]["skill"], "fixture-skill");
    assert_eq!(json["reviews"][0]["source_authentication_unknown"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_ASSET_BODY_CANARY"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_ASSET_BODY_CANARY"));
    assert_eq!(
        before,
        fs::read(fixture.directory.path().join("authority.json")).unwrap()
    );
}

#[test]
fn routing_reads_no_body_before_selection_and_review_requires_exact_selected_bytes() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.directory.path().join("skill.txt")).unwrap();
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.arg("route-phase");
    fixture.phase(&mut command);
    command
        .arg("--asset-authority")
        .arg(fixture.directory.path().join("authority.json"));
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["schema_version"], 3);
    assert!(!fixture.review(&["fixture-skill"]).status.success());
    fs::write(
        fixture.directory.path().join("skill.txt"),
        "TAMPERED_SECRET_CANARY",
    )
    .unwrap();
    let output = fixture.review(&["fixture-skill"]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("TAMPERED_SECRET_CANARY"));
}

#[test]
fn review_rejects_rehashed_roots_and_cross_workspace_authority() {
    for mutation in 0..4 {
        let fixture = Fixture::new();
        match mutation {
            0 => fixture.change("contracts.json", |v| {
                v["models"][0]["provider"] = json!("OTHER_SECRET_CANARY")
            }),
            1 => fixture.change("authority.json", |v| {
                v["execution_scope"]["workspace"] = json!("/different/repo")
            }),
            2 => fixture.change("authority.json", |v| {
                v["availability"]["expected_hash"] = json!("a".repeat(64))
            }),
            _ => fixture.change("authority.json", |v| v["approved"] = json!(true)),
        }
        if mutation == 0 {
            // Recompute valid integrity, while preserving the parent's pinned
            // expected root. An attacker-controlled self-hash is insufficient.
            let root = fixture.directory.path();
            let catalog =
                Catalog::from_json(&fs::read(root.join("catalog.json")).unwrap()).unwrap();
            let document =
                serde_json::from_slice(&fs::read(root.join("contracts.json")).unwrap()).unwrap();
            let document = router_asset_contracts::seal_contracts(document, &catalog).unwrap();
            write(&root.join("contracts.json"), &document);
        }
        let output = fixture.review(&["fixture-skill"]);
        assert!(!output.status.success(), "mutation {mutation}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("OTHER_SECRET_CANARY"));
    }
}

#[test]
fn review_rejects_unrequested_or_duplicate_skill_selection_and_duplicate_json_keys() {
    let fixture = Fixture::new();
    assert!(!fixture.review(&["never-selected"]).status.success());
    assert!(
        !fixture
            .review(&["fixture-skill", "fixture-skill"])
            .status
            .success()
    );
    let path = fixture.directory.path().join("authority.json");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(path, original.replacen('{', "{\"schema_version\":1,", 1)).unwrap();
    assert!(!fixture.review(&["fixture-skill"]).status.success());
}
