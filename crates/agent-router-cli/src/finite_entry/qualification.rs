//! Explicit workspace-only public fixture qualification. Never built by default.
use super::{EntryResult, Submission};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Mode {
    FreshT02V1,
}
const PACKETS: &[u8] = include_bytes!("../../../router-runtime/examples/campaign/packets.json");
pub(super) fn check_submission(s: &Submission) -> EntryResult<()> {
    let packets: Value = serde_json::from_slice(PACKETS)?;
    let files = packets["assignments"]
        .as_array()
        .ok_or("fixture packets")?
        .iter()
        .find(|p| p["assignment_id"] == "t02-Lm-r1-H1")
        .and_then(|p| p["files"].as_object())
        .ok_or("fixture files")?;
    if s.output != "lib.rs"
        || s.public_files.len() != files.len()
        || s.public_files
            .iter()
            .any(|(name, f)| files.get(name).and_then(Value::as_str) != Some(f.content.as_str()))
    {
        return Err("fixed public fixture required".into());
    }
    Ok(())
}

use router_adapter_api::{content_digest, strict_json_limited};
use router_runtime::{FiniteClientVerifier, Runtime};
use router_types::{VerificationOutcome, VerificationRecord};
use serde_json::json;
use std::{path::Path, process::Command, time::Duration};
mod process;

const ENGINE: &[u8] = include_bytes!("../../../router-runtime/examples/campaign/t02-v2/verify.py");
const CASES: &[u8] = include_bytes!("../../../router-runtime/examples/campaign/t02-v2/cases.rs");
const BASELINE: &[u8] =
    include_bytes!("../../../router-runtime/examples/campaign/t02-v2/acceptance.json");
const ADAPTER: &[u8] = include_bytes!("qualification/fresh_t02.py");

pub(super) fn identity() -> EntryResult<Value> {
    let baseline: Value = serde_json::from_slice(BASELINE)?;
    Ok(
        json!({"scope":"fresh_t02_v1","adapter_sha256":content_digest(ADAPTER),
        "verifier_sha256":content_digest(ENGINE),"cases_sha256":content_digest(CASES),
        "compiler_sha256":baseline["compiler_sha256"],"qualification_only":true}),
    )
}

fn validate_receipt(raw: &[u8], report: &[u8]) -> EntryResult<VerificationRecord> {
    let receipt = strict_json_limited(raw, 32768)?;
    let original: Value = serde_json::from_slice(report)?;
    let baseline: Value = serde_json::from_slice(BASELINE)?;
    let mut outcomes = baseline["outcomes"].clone();
    for label in ["source", "candidate"] {
        let hash = receipt["outcomes"][label]["stdout_sha256"]
            .as_str()
            .ok_or("case output hash")?;
        if !router_adapter_api::is_digest(hash) {
            return Err("case output hash".into());
        }
        outcomes[label]["stdout_sha256"] = hash.into();
    }
    let expected = json!({"schema_version":1,"scope":"fresh_t02_v1",
        "session_id":original["session_id"],"submission_sha256":original["submission_sha256"],
        "run_id":"entry","receipt_id":original["summary"]["receipt_id"],
        "runtime_report_sha256":content_digest(report),"source_sha256":baseline["source_sha256"],
        "candidate_sha256":baseline["candidate_sha256"],"artifact_sha256":baseline["artifact_sha256"],
        "compiler_sha256":baseline["compiler_sha256"],"adapter_sha256":content_digest(ADAPTER),
        "verifier_sha256":content_digest(ENGINE),"cases_sha256":content_digest(CASES),
        "outcomes":outcomes,"memory_bytes":8589934592u64,"swap_bytes":0,
        "objective":"unknown","human_acceptance":null,"source_publication":false,"runtime_verdict_modified":false});
    if receipt != expected
        || original["run_id"] != "entry"
        || original["artifact"]["sha256"] != baseline["artifact_sha256"]
        || original["output"]["content"]
            .as_str()
            .map(|s| content_digest(s.as_bytes()))
            .as_deref()
            != baseline["candidate_sha256"].as_str()
    {
        return Err("fresh qualification receipt binding".into());
    }
    Ok(VerificationRecord {
        verifier: "fresh-t02-v1".into(),
        evidence_sha256: content_digest(raw),
        artifact_sha256: baseline["artifact_sha256"].as_str().map(str::to_owned),
        validation: VerificationOutcome::Passed,
        objective: VerificationOutcome::Unknown,
    })
}

pub(super) fn evaluate(
    root: &Path,
    submission: &[u8],
    report: &Value,
    verifier: &FiniteClientVerifier<'_>,
    runtime: &Runtime,
) -> EntryResult<Value> {
    let patch = verifier.artifact()?;
    let baseline: Value = serde_json::from_slice(BASELINE)?;
    let task: Value = serde_json::from_slice(submission)?;
    let candidate = report["output"]["content"]
        .as_str()
        .ok_or("candidate")?
        .as_bytes();
    let source = task["public_files"]["lib.rs"]["content"]
        .as_str()
        .ok_or("source")?
        .as_bytes();
    if content_digest(&patch) != baseline["artifact_sha256"]
        || content_digest(candidate) != baseline["candidate_sha256"]
        || content_digest(source) != baseline["source_sha256"]
    {
        return Err("fixed fixture bytes required before compilation".into());
    }
    let raw_report = serde_json::to_vec(report)?;
    let capsule = root.join("fresh-t02-verification");
    super::session::mkdir(&capsule)?;
    for directory in [
        "crates",
        "crates/router-runtime",
        "crates/router-runtime/examples",
        "crates/router-runtime/examples/campaign",
        "crates/router-runtime/examples/campaign/t02-v2",
    ] {
        super::session::mkdir(&capsule.join(directory))?;
    }
    for (name, bytes) in [
        ("fresh_t02.py", ADAPTER),
        (
            "crates/router-runtime/examples/campaign/t02-v2/verify.py",
            ENGINE,
        ),
        (
            "crates/router-runtime/examples/campaign/t02-v2/cases.rs",
            CASES,
        ),
        ("submission.json", submission),
        ("report.json", raw_report.as_slice()),
        ("source.rs", source),
        ("candidate.rs", candidate),
        ("change.patch", patch.as_slice()),
    ] {
        super::session::save(&capsule.join(name), bytes)?;
    }
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-B"])
        .arg(capsule.join("fresh_t02.py"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &capsule)
        .env("TMPDIR", &capsule)
        .current_dir(&capsule);
    process::run(command, Duration::from_secs(60))?;
    let raw = super::session::read_output(&capsule.join("receipt.json"))?;
    let record = validate_receipt(&raw, &raw_report)?;
    // Preserve the exact independently checked receipt before changing the durable projection.
    super::session::save(&root.join("fresh-t02-receipt.json"), &raw)?;
    std::fs::File::open(root)?.sync_all()?;
    let summary = verifier.record(record.clone())?;
    runtime.verify_integrity()?;
    let history: Vec<VerificationRecord> = runtime
        .events()?
        .into_iter()
        .filter(|e| e.run_id == "entry" && e.kind == "verification.attested.v1")
        .map(|e| serde_json::from_str(&e.detail))
        .collect::<Result<_, _>>()?;
    if history != [record.clone()]
        || runtime.verification_record("entry")? != Some(record.clone())
        || summary.state != router_types::RunState::Verifying
    {
        return Err("verification history mismatch".into());
    }
    Ok(
        json!({"scope":"fresh_t02_v1","verification":record,"history_retained":true,
        "human_acceptance":null,"source_publication":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use router_adapter_api::content_digest;
    use serde_json::json;
    #[test]
    fn receipt_cannot_promote_substituted_or_incomplete_evidence() {
        let baseline: Value = serde_json::from_slice(include_bytes!(
            "../../../router-runtime/examples/campaign/t02-v2/acceptance.json"
        ))
        .unwrap();
        let candidate =
            include_str!("../../../../docs/bench/native-useful-score-2026-10-21/submission/lib.rs");
        let report = serde_json::to_vec(&json!({"session_id":"1".repeat(64),"submission_sha256":"2".repeat(64),"run_id":"entry","summary":{"receipt_id":"3".repeat(64)},"artifact":{"sha256":baseline["artifact_sha256"]},"output":{"content":candidate}})).unwrap();
        let mut receipt = json!({"schema_version":1,"scope":"fresh_t02_v1","session_id":"1".repeat(64),"submission_sha256":"2".repeat(64),"run_id":"entry","receipt_id":"3".repeat(64),
            "runtime_report_sha256":content_digest(&report),"source_sha256":baseline["source_sha256"],"candidate_sha256":baseline["candidate_sha256"],"artifact_sha256":baseline["artifact_sha256"],
            "compiler_sha256":baseline["compiler_sha256"],"adapter_sha256":content_digest(include_bytes!("qualification/fresh_t02.py")),"verifier_sha256":baseline["verifier_sha256"],"cases_sha256":baseline["required_cases_sha256"],
            "outcomes":baseline["outcomes"],"memory_bytes":8589934592u64,"swap_bytes":0,"objective":"unknown","human_acceptance":null,"source_publication":false,"runtime_verdict_modified":false});
        let raw = serde_json::to_vec(&receipt).unwrap();
        let record = validate_receipt(&raw, &report);
        assert!(record.is_ok(), "the exact trusted child receipt must bind");
        let record = record.unwrap();
        assert_eq!(record.verifier, "fresh-t02-v1");
        assert_eq!(record.objective, router_types::VerificationOutcome::Unknown);
        for path in [
            "/session_id",
            "/submission_sha256",
            "/run_id",
            "/receipt_id",
            "/runtime_report_sha256",
            "/artifact_sha256",
            "/candidate_sha256",
            "/compiler_sha256",
            "/verifier_sha256",
            "/adapter_sha256",
            "/cases_sha256",
            "/outcomes/candidate/outcome",
            "/outcomes/candidate/cases/t02_v2_empty",
            "/objective",
            "/human_acceptance",
        ] {
            let mut bad = receipt.clone();
            *bad.pointer_mut(path).unwrap() = json!("substituted");
            assert!(
                validate_receipt(&serde_json::to_vec(&bad).unwrap(), &report).is_err(),
                "{path}"
            );
        }
        receipt["outcomes"]["candidate"]["cases"]
            .as_object_mut()
            .unwrap()
            .remove("t02_v2_wide_sums");
        assert!(validate_receipt(&serde_json::to_vec(&receipt).unwrap(), &report).is_err());
        assert!(validate_receipt(&raw, b"{}").is_err());
    }
}
