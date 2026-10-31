// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Opt-in v5 reader: the v4 projection and its advisory findings, derived from
//! that one report inside the same passive read transaction.
use super::timeline::project_v4;
use super::*;
use crate::findings::{RunObservationV5, derive_findings};

pub fn observe_run_v5(root: &Path, id: &str) -> Result<RunObservationV5> {
    if id.is_empty() || id.len() > 1024 {
        return Err(RuntimeError::Invalid("observation run id"));
    }
    let mut db = control_connection(root, false)?;
    let tx = db.transaction()?;
    let report = project_v5(&tx, id).map_err(|error| match error {
        RuntimeError::Json(_) => RuntimeError::Integrity("observation malformed findings"),
        other => other,
    })?;
    tx.commit()?;
    Ok(report)
}

fn project_v5(db: &Connection, id: &str) -> Result<RunObservationV5> {
    let observation = project_v4(db, id)?;
    let findings = derive_findings(&observation)?;
    let mut report = RunObservationV5 {
        observation,
        findings,
    };
    let summary = &mut report.observation.observation.observation.summary;
    summary.schema_version = 5;
    summary.snapshot_sha256.clear();
    report
        .observation
        .observation
        .observation
        .summary
        .snapshot_sha256 = digest("run-observation-v5", &report)?;
    Ok(report)
}

/// Domain-separated identity of a finding.
pub(crate) fn finding_digest(value: &impl serde::Serialize) -> Result<String> {
    digest("run-finding-v1", value)
}
