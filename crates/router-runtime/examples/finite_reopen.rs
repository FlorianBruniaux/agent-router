// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Qualification-only observer. Opening performs the real Store recovery.
//! No application handle is reconstructed and no execution is requested.
use router_runtime::{Runtime, RuntimeLimits};
use router_types::{AdmissionConfig, OutputRetentionPolicy};
use std::{error::Error, os::unix::fs::MetadataExt, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "expected existing public fixture runtime directory and admission policy hash".into(),
        );
    }
    let root = PathBuf::from(&args[0]);
    let allowed = ["kill_before", "kill_after", "positive"]
        .map(|case| PathBuf::from(format!("/task/entries/{case}/runtime")));
    if !allowed.contains(&root) || root.canonicalize()? != root {
        return Err("only the isolated public qualification stores are allowed".into());
    }
    let metadata = root.symlink_metadata()?;
    if !metadata.is_dir()
        || metadata.mode() & 0o7777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || !root.join("runtime.sqlite3").is_file()
    {
        return Err("existing private fixture store required".into());
    }
    let config = AdmissionConfig {
        schema_version: 1,
        registry: root.parent().ok_or("fixture parent")?.join("admission"),
        expected_policy_sha256: args[1].to_str().ok_or("policy hash")?.into(),
        runtime_id: "entry".into(),
        session_budget: None,
    };
    let runtime = Runtime::open_with_admission(
        &root,
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 3600,
            },
            ..Default::default()
        },
        &config,
    )?;
    runtime.verify_integrity()?;
    let events = runtime.events()?;
    if events.len() > 256 {
        return Err("qualification event bound".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "opened_by": "Runtime::open_with_admission", "integrity": "passed",
            "summary": runtime.summary("entry")?,
            "verification": runtime.verification_record("entry")?,
            "cleanup": runtime.cleanup_evidence("entry")?, "events": events,
            "application_authority_reconstructed": false,
            "replay_attempted": false,
        })
    );
    Ok(())
}
