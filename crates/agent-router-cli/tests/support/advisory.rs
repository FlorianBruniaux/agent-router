// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Helpers shared by the advisory CLI tests. Nothing is executed and no
//! provider is contacted.
#![allow(dead_code)]
use assert_cmd::Command;
use serde_json::Value;
#[cfg(feature = "codex")]
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn command() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
    command.timeout(Duration::from_secs(20));
    command
}

pub fn root(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

pub fn write(path: &Path, value: &Value) -> PathBuf {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path.to_owned()
}

pub fn os(value: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(value)
}

pub fn error_line(code: &str) -> String {
    format!("{{\"schema_version\":1,\"error\":\"{code}\"}}\n")
}

/// Synthetic Codex host snapshot built like the Codex probe: models and efforts
/// are discovered, capabilities stay UNKNOWN.
#[cfg(feature = "codex")]
pub fn discovered_snapshot_at(dir: &Path, name: &str, now: &str) -> PathBuf {
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
        ("gpt-6.1-terra", extended.as_slice()),
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
        .args(["--observed-version", "0.163.0", "--now", now, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let path = dir.join(name);
    fs::write(&path, observed).unwrap();
    path
}
