//! Existing-runtime application only. No provider, dispatch, or registry creation.
use super::{Input, config_bytes};
use crate::{CliError, Outcome};
use router_runtime::{Runtime, RuntimeLimits};
use router_types::{AdmissionConfig, OutputRetentionPolicy};
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u8,
    root: PathBuf,
    admission: AdmissionConfig,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    schema_version: u8,
    kind: String,
    approval_digest: String,
}
pub(crate) fn run(
    path: &Path,
    run_id: &str,
    approve: bool,
    json: bool,
) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let fail = || CliError::Product("finite_resume.refused");
    let c: Config = serde_json::from_value(
        router_adapter_api::strict_json_limited(&config_bytes(path)?, 65536).map_err(|_| fail())?,
    )
    .map_err(|_| fail())?;
    if c.schema_version != 1
        || !c.root.is_absolute()
        || c.root.canonicalize().map_err(|_| fail())? != c.root
        || !c.root.join("runtime.sqlite3").is_file()
        || !c.admission.registry.is_dir()
        || c.admission.registry.canonicalize().map_err(|_| fail())? != c.admission.registry
    {
        return Err(fail());
    }
    let runtime = Runtime::open_with_admission(
        &c.root,
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 3600,
            },
            ..Default::default()
        },
        &c.admission,
    )
    .map_err(|_| fail())?;
    let h = runtime.finite_resume(run_id).map_err(|_| fail())?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| CliError::Internal)?;
    let presentation = rt.block_on(h.presentation()).map_err(|_| fail())?;
    let review = &presentation.review;
    let mut nonce = [0; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(|_| fail())?;
    let output = super::output::Output::new(
        router_adapter_api::content_digest(&nonce),
        router_adapter_api::content_digest(&config_bytes(path)?),
    )
    .map_err(|_| fail())?;
    output
        .resume_review(&presentation, approve)
        .map_err(|_| fail())?;
    if approve {
        if review.approval_digest.is_none() {
            return Err(fail());
        }
        let bytes = Input::stdin()?.line(1024, Instant::now() + Duration::from_secs(30))?;
        let approval: Approval = serde_json::from_value(
            router_adapter_api::strict_json_limited(&bytes, 1024).map_err(|_| fail())?,
        )
        .map_err(|_| fail())?;
        if approval.schema_version != 1 || approval.kind != "resume.approve" {
            return Err(fail());
        }
        rt.block_on(h.apply_presented(&presentation, &approval.approval_digest))
            .map_err(|_| fail())?;
        output.emit("resume.applied", serde_json::json!({"run_id":run_id,"artifact_sha256":runtime.summary(run_id).map_err(|_| fail())?.artifact_sha256})).map_err(|_| fail())?;
    }
    Ok(Outcome {
        output: String::new(),
        exit_code: std::process::ExitCode::SUCCESS,
    })
}
