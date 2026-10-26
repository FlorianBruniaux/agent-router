#[path = "../../../tests/support/capability_verifier.rs"]
#[allow(clippy::duplicate_mod)] // Shared fixtures can coexist in one integration test.
mod fixture_proofs;
use std::{collections::BTreeMap, fs, path::Path};

use adapter_codex::{CodexProbe, CodexProbeConfig, MAX_MODEL_CACHE_BYTES};
use router_adapter_api::{HostProbe, ProbeError, SnapshotError, SnapshotValidation};
use router_types::CapabilitySupport;

const FIXTURE: &[u8] = include_bytes!("fixtures/models-cache.json");
const NOW: u64 = 1_767_225_600;

fn probe(path: &Path) -> CodexProbe {
    CodexProbe::new(CodexProbeConfig {
        model_cache_path: path.to_owned(),
        observed_version: Some("0.1.0".into()),
    })
}

fn tree(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn probe_is_deterministic_passive_and_keeps_cache_capabilities_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    fs::write(directory.path().join("sentinel"), b"unchanged").unwrap();
    let before = tree(directory.path());
    let first = probe(&path).probe(NOW + 10).unwrap();
    let second = probe(&path).probe(NOW + 10).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.evidence[0].observed_at_unix_seconds, NOW);
    assert_eq!(first.evidence[0].observed_version.as_deref(), Some("0.1.0"));
    assert!(
        first
            .capabilities
            .values()
            .all(|evidence| evidence.support == CapabilitySupport::Unknown)
    );
    assert!(first.host.capabilities.is_empty());
    assert_eq!(first.host.supported_models.len(), 1);
    assert_eq!(before, tree(directory.path()));
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains(directory.path().to_str().unwrap()));
    assert!(!json.contains("example_tool"));
    assert!(!json.contains("synthetic-fixture"));
    fixture_proofs::from_json(
        json.as_bytes(),
        SnapshotValidation {
            now_unix_seconds: NOW + 10,
            max_age_seconds: 60,
        },
    )
    .unwrap();
}

#[test]
fn old_cache_stays_old_after_a_new_probe() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    let fresh_probe = probe(&path).probe(NOW + 100).unwrap();
    assert!(matches!(
        fixture_proofs::from_observation(
            fresh_probe,
            SnapshotValidation {
                now_unix_seconds: NOW + 100,
                max_age_seconds: 60
            }
        ),
        Err(SnapshotError::StaleSource)
    ));
}

#[test]
fn mismatched_host_version_is_not_hidden_by_cache_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    let mismatch = CodexProbe::new(CodexProbeConfig {
        model_cache_path: path,
        observed_version: Some("0.2.0".into()),
    });
    assert!(matches!(
        mismatch.probe(NOW),
        Err(ProbeError::VersionMismatch)
    ));
}

#[test]
fn whole_cache_version_accepts_prerelease_discovery_but_preserves_the_full_cli_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    let mut hashes = Vec::new();
    for version in ["0.1.0", "0.1.0-alpha.10.2", "0.1.0-alpha.11.1+desktop"] {
        let observed = CodexProbe::new(CodexProbeConfig {
            model_cache_path: path.clone(),
            observed_version: Some(version.into()),
        })
        .probe(NOW)
        .unwrap();
        assert_eq!(
            observed.evidence[0].observed_version.as_deref(),
            Some("0.1.0")
        );
        let source = observed
            .evidence
            .iter()
            .find(|source| source.id == "codex-observed-version")
            .unwrap();
        assert_eq!(source.observed_version.as_deref(), Some(version));
        assert_eq!(
            serde_json::to_value(source.kind).unwrap(),
            "codex_cli_version"
        );
        assert!(observed.host.capabilities.is_empty());
        assert!(
            observed
                .capabilities
                .values()
                .all(|value| value.support == CapabilitySupport::Unknown)
        );
        hashes.push(observed.snapshot_hash);
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[1], hashes[2]);
    assert_eq!(fs::read(path).unwrap(), FIXTURE);
}

#[test]
fn whole_cache_version_rejects_other_releases_and_malformed_prereleases() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    for version in [
        "0.1.1-alpha.1",
        "0.2.0",
        "1.1.0",
        "0.1.0-alpha..1",
        "0.1.0-01",
        "0.1.0 trailing",
    ] {
        let result = CodexProbe::new(CodexProbeConfig {
            model_cache_path: path.clone(),
            observed_version: Some(version.into()),
        })
        .probe(NOW);
        assert!(
            matches!(result, Err(ProbeError::VersionMismatch)),
            "{version}"
        );
    }
}

#[test]
fn an_explicit_prerelease_cache_is_not_equivalent_to_another_prerelease() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    let mut cache: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    cache["client_version"] = "0.1.0-alpha.1".into();
    fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    for version in ["0.1.0", "0.1.0-alpha.2"] {
        assert!(matches!(
            CodexProbe::new(CodexProbeConfig {
                model_cache_path: path.clone(),
                observed_version: Some(version.into()),
            })
            .probe(NOW),
            Err(ProbeError::VersionMismatch)
        ));
    }
}

#[test]
fn identical_malformed_versions_are_not_a_compatible_discovery_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    for version in ["0.1.0-alpha..1", "0.1.0-01", "01.1.0"] {
        let mut cache: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        cache["client_version"] = version.into();
        fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
        assert!(
            matches!(
                CodexProbe::new(CodexProbeConfig {
                    model_cache_path: path.clone(),
                    observed_version: Some(version.into()),
                })
                .probe(NOW),
                Err(ProbeError::VersionMismatch)
            ),
            "{version}"
        );
    }
}

#[test]
fn native_version_pins_still_require_the_exact_prerelease() {
    use router_adapter_api::HarnessAdapter;
    let adapter = adapter_codex::CodexAdapter;
    assert!(
        adapter
            .validate_native_version_pin(b"codex-cli 0.163.0-alpha.10.2\n", "0.163.0-alpha.10.2")
            .is_ok()
    );
    for version in ["0.163.0", "0.163.0-alpha.11.1", "0.159.2"] {
        assert!(
            adapter
                .validate_native_version_pin(b"codex-cli 0.163.0-alpha.10.2\n", version)
                .is_err()
        );
    }
}

#[test]
fn missing_malformed_duplicate_and_oversized_cache_are_typed_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    assert!(matches!(
        probe(&path).probe(NOW),
        Err(ProbeError::Unavailable)
    ));
    fs::write(&path, b"invalid sensitive response").unwrap();
    let error = probe(&path).probe(NOW).unwrap_err();
    assert_eq!(error.code(), "probe.cache.invalid");
    assert!(!error.to_string().contains("sensitive"));
    let mut document: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    let model = document["models"][0].clone();
    document["models"].as_array_mut().unwrap().push(model);
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(matches!(
        probe(&path).probe(NOW),
        Err(ProbeError::InvalidCache("duplicate model"))
    ));
    fs::write(&path, vec![b' '; MAX_MODEL_CACHE_BYTES + 1]).unwrap();
    assert!(matches!(
        probe(&path).probe(NOW),
        Err(ProbeError::CacheTooLarge)
    ));
}

#[test]
fn unknown_effort_invalid_date_and_future_cache_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    let mut document: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    document["models"][0]["supported_reasoning_levels"][0]["effort"] = "invented".into();
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(matches!(
        probe(&path).probe(NOW),
        Err(ProbeError::InvalidCache("schema"))
    ));
    let mut document: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    document["fetched_at"] = "2026-02-30T00:00:00Z".into();
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(matches!(
        probe(&path).probe(NOW),
        Err(ProbeError::InvalidCache("timestamp"))
    ));
    fs::write(&path, FIXTURE).unwrap();
    assert!(matches!(
        probe(&path).probe(NOW - 1),
        Err(ProbeError::Snapshot(SnapshotError::FutureTimestamp))
    ));
}

#[cfg(unix)]
#[test]
fn non_regular_cache_and_symlink_are_rejected_without_blocking() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cache.json");
    fs::write(&path, FIXTURE).unwrap();
    let link = directory.path().join("linked-cache.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(matches!(probe(&link).probe(NOW), Err(ProbeError::Read)));
    assert!(matches!(
        probe(directory.path()).probe(NOW),
        Err(ProbeError::InvalidCache("not a regular file"))
    ));
    let fifo = directory.path().join("cache-fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(matches!(
        probe(&fifo).probe(NOW),
        Err(ProbeError::InvalidCache("not a regular file"))
    ));
}
