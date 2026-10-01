//! Explicit store-backed proof loading. No host execution and no proof issuer.
use crate::{CliError, execution::SandboxConfig};
use router_adapter_api::{CapabilityProofVerifier, HostObservation};
use router_runtime::{CapabilityProofStore, StoredCapabilityProofVerifier};
use std::{path::Path, sync::Arc};

pub(crate) fn has_claims(observation: &HostObservation) -> bool {
    observation
        .capabilities
        .values()
        .any(|e| e.support != router_types::CapabilitySupport::Unknown)
}

fn secret(store: &Path) -> Result<[u8; 32], CliError> {
    crate::approval_key::load_existing(store)?
        .bytes()
        .try_into()
        .map_err(|_| CliError::Internal)
}

/// Current pins come from executable bytes, compiled adapter/profile code and
/// configuration, never from a caller-supplied digest. inspect uses the proof's
/// bound relative scope; routing and execution replace it with the actual scope.
pub(crate) fn fresh(
    store: &Path,
    config: &SandboxConfig,
    observation: &HostObservation,
    scope: Option<(&[String], bool)>,
) -> Result<Arc<StoredCapabilityProofVerifier>, CliError> {
    let secret = secret(store)?;
    let proofs = CapabilityProofStore::open_existing(store)
        .map_err(|e| CliError::Product(e.code()))?
        .verifier(&secret, vec![])
        .map_err(|e| CliError::Product(e.code()))?;
    let mut expected = Vec::new();
    for binding in proofs.authenticated_bindings() {
        if binding.harness_id != observation.host.harness.as_str()
            || binding.host_snapshot_id != observation.host.id
        {
            continue;
        }
        // A conflicting explicit observation voids the pin. Missing model-cache
        // version is UNKNOWN; the exact executable is still hashed and native
        // dispatch must independently observe the proof's version.
        if observation.evidence.iter().any(|e| {
            e.kind != router_adapter_api::EvidenceKind::HostRuntime
                && e.observed_version
                    .as_ref()
                    .is_some_and(|v| v != &binding.observed_version)
        }) {
            continue;
        }
        let (roots, network) = match scope {
            Some((roots, network)) => (roots.iter().cloned().collect(), network),
            None => (binding.write_roots.clone(), binding.network),
        };
        let binding = crate::execution::capability_binding(
            config,
            &observation.host,
            &binding.observed_version,
            roots,
            network,
        )?;
        if !expected.contains(&binding) {
            expected.push(binding);
        }
    }
    Ok(Arc::new(
        StoredCapabilityProofVerifier::open_existing(store, secret, expected)
            .map_err(|e| CliError::Product(e.code()))?,
    ))
}

/// Historical reconstruction only. The caller must use a persisted run clock
/// and feed the result exclusively to Runtime::reopen, never fresh dispatch.
pub(crate) fn historical(store: &Path) -> Result<Arc<dyn CapabilityProofVerifier>, CliError> {
    historical_through(store, None)
}
pub(crate) fn historical_through(
    store: &Path,
    sequence: Option<u64>,
) -> Result<Arc<dyn CapabilityProofVerifier>, CliError> {
    let secret = secret(store)?;
    let proofs = CapabilityProofStore::open_existing(store)
        .map_err(|e| CliError::Product(e.code()))?
        .verifier(&secret, vec![])
        .map_err(|e| CliError::Product(e.code()))?;
    let bindings = proofs.authenticated_bindings();
    let verifier = StoredCapabilityProofVerifier::open_existing(store, secret, bindings)
        .map_err(|e| CliError::Product(e.code()))?;
    Ok(Arc::new(match sequence {
        Some(sequence) => verifier.through_sequence(sequence),
        None => verifier,
    }))
}

#[cfg(all(test, feature = "codex"))]
mod tests {
    use super::*;
    use router_adapter_api::*;
    use router_types::*;
    use serde_json::json;
    use std::{collections::BTreeSet, fs, os::unix::fs::PermissionsExt, path::PathBuf};

    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        store: PathBuf,
        config: PathBuf,
        cache: PathBuf,
        binary: PathBuf,
        key: [u8; 32],
        proof: AuthenticatedCapabilityProof,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let root = fs::canonicalize(temp.path()).unwrap();
            let store = root.join("store");
            fs::create_dir(&store).unwrap();
            fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
            let key = [42; 32];
            fs::write(store.join("approval.key"), key).unwrap();
            fs::set_permissions(
                store.join("approval.key"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            let binary = root.join("fixture-host");
            fs::write(&binary, b"synthetic executable bytes, never executed").unwrap();
            let helper = root.join("fixture-helper");
            fs::write(&helper, b"synthetic helper bytes, never executed").unwrap();
            let config = root.join("sandbox.json");
            fs::write(&config, serde_json::to_vec(&json!({"schema_version":1,
                "programs":{"codex":binary},"helper":helper,"bubblewrap":helper,
                "read_roots":[],"environment_names":[],
                "limits":{"cpu_seconds":1,"address_space_bytes":null,"max_open_files":64,"max_processes":8,"max_file_bytes":8192},
                "feedback_store":null})).unwrap()).unwrap();
            let cache = root.join("models.json");
            fs::write(&cache, serde_json::to_vec(&json!({"fetched_at":"1970-01-01T00:01:40Z","client_version":"0.1.0",
                "models":[{"slug":"fixture-best","supported_reasoning_levels":[{"effort":"medium"}]}]})).unwrap()).unwrap();
            let observation = Self::inspect_paths(&cache, None, None, 120);
            let configuration = crate::execution::sandbox(&config)
                .unwrap_or_else(|_| panic!("fixture configuration"));
            let binding = crate::execution::capability_binding(
                &configuration,
                &observation.host,
                "0.1.0",
                BTreeSet::from(["src".into()]),
                false,
            )
            .unwrap_or_else(|_| panic!("fixture binding"));
            let mut db = CapabilityProofStore::create(&store).unwrap();
            let proof = CapabilityProofKey::derive(&key, db.store_id())
                .unwrap()
                .authenticate(CapabilityProofRecord {
                    schema_version: 1,
                    proof_id: "synthetic-write-proof".into(),
                    sequence: 1,
                    store_id: db.store_id().into(),
                    capability: Capability::WorkspaceWrite,
                    tier: CapabilityProofTier::Proven,
                    network_scope: None,
                    binding,
                    model_id: "fixture-best".into(),
                    nonce_sha256: "7".repeat(64),
                    expected_effect_sha256: "8".repeat(64),
                    started_at_unix_seconds: 105,
                    ended_at_unix_seconds: 110,
                    max_age_seconds: 1000,
                    outcome: CapabilityAttemptOutcome::Succeeded,
                })
                .unwrap();
            db.append(&proof, &key).unwrap();
            Self {
                _temp: temp,
                root,
                store,
                config,
                cache,
                binary,
                key,
                proof,
            }
        }
        fn inspect_paths(
            cache: &Path,
            store: Option<&Path>,
            config: Option<&Path>,
            now: u64,
        ) -> HostObservation {
            let outcome = crate::inspect::inspect(
                crate::inspect::InspectArgs {
                    store: store.map(Path::to_owned),
                    execution_config: config.map(Path::to_owned),
                    host: crate::command::HostChoice::Codex,
                    model_cache: Some(cache.to_owned()),
                    native_stream: None,
                    observed_at: None,
                    observed_version: Some("0.1.0".into()),
                    now: Some(now),
                },
                true,
            )
            .unwrap_or_else(|e| panic!("inspect {}", e.code()));
            serde_json::from_str(&outcome.output).unwrap()
        }
        fn inspected(&self, now: u64) -> HostObservation {
            Self::inspect_paths(&self.cache, Some(&self.store), Some(&self.config), now)
        }
        fn inputs(&self, observation: &HostObservation, now: u64) -> crate::command::PhaseArgs {
            let mut request: PhaseRouteRequest = serde_json::from_str(include_str!(
                "../../../fixtures/examples/phase-request.json"
            ))
            .unwrap();
            request.phase = StageKind::Execute;
            request.task.required_capabilities = BTreeSet::from([Capability::WorkspaceWrite]);
            request.candidates.truncate(1);
            request.candidates[0].sandbox = PhaseSandbox::WorkspaceWrite;
            request.candidates[0].required_capabilities =
                BTreeSet::from([Capability::WorkspaceWrite]);
            request.candidates[0].required_mutations = BTreeSet::from([Mutation::WorkspaceWrite]);
            let context = self.root.join("context.json");
            fs::write(&context, serde_json::to_vec(&request).unwrap()).unwrap();
            let host_snapshot = self.root.join("snapshot.json");
            fs::write(&host_snapshot, serde_json::to_vec(observation).unwrap()).unwrap();
            let catalog = self.root.join("catalog.json");
            fs::write(
                &catalog,
                include_bytes!("../../../fixtures/examples/phase-catalog.json"),
            )
            .unwrap();
            crate::command::PhaseArgs {
                store: Some(self.store.clone()),
                execution_config: Some(self.config.clone()),
                context,
                host_snapshot,
                catalog,
                policy: None,
                policy_store: None,
                asset_authority: None,
                now: Some(now),
                snapshot_max_age_seconds: 300,
            }
        }
    }

    #[test]
    fn inspect_folds_authenticated_proven_proof_and_write_phase_passes_shared_policy() {
        let f = Fixture::new();
        let observation = f.inspected(120);
        assert_eq!(
            observation.capabilities[&Capability::WorkspaceWrite].support,
            CapabilitySupport::Supported
        );
        assert_eq!(
            observation.capabilities[&Capability::Network].support,
            CapabilitySupport::Unknown
        );
        let (router, context) = crate::phases::load(&f.inputs(&observation, 120))
            .unwrap_or_else(|e| panic!("route {}", e.code()));
        let receipt = router.route(&context).unwrap();
        assert!(receipt.selected.is_some(), "{:?}", receipt.exclusions);
        assert_eq!(receipt.schema_version, 5);
        let evidence = &receipt.capability_evidence[&Capability::WorkspaceWrite];
        assert_eq!(evidence.proof_sha256, f.proof.digest().unwrap());
        assert_eq!(evidence.tier, "proven");
        assert_eq!(evidence.expires_at_unix_seconds, 1110);
        // The snapshot is fresh but the proof may be older than the 300s bound.
        let mut cache: serde_json::Value =
            serde_json::from_slice(&fs::read(&f.cache).unwrap()).unwrap();
        cache["fetched_at"] = "1970-01-01T00:08:20Z".into();
        fs::write(&f.cache, serde_json::to_vec(&cache).unwrap()).unwrap();
        let old_proof = f.inspected(500);
        assert!(crate::phases::load(&f.inputs(&old_proof, 500)).is_ok());
    }

    #[test]
    fn shared_cli_loader_refuses_handwritten_relabelled_sources_and_changed_binary() {
        let f = Fixture::new();
        let valid = f.inspected(120);
        for mutate in [
            (|o: &mut HostObservation| {
                o.evidence
                    .iter_mut()
                    .find(|e| e.kind == EvidenceKind::HostRuntime)
                    .unwrap()
                    .content_sha256 = "f".repeat(64);
            }) as fn(&mut HostObservation),
            |o| {
                o.evidence
                    .iter_mut()
                    .find(|e| e.kind == EvidenceKind::HostRuntime)
                    .unwrap()
                    .observed_version = Some("0.2.0".into());
            },
            |o| {
                o.evidence
                    .iter_mut()
                    .find(|e| e.kind == EvidenceKind::HostRuntime)
                    .unwrap()
                    .observed_at_unix_seconds += 1;
            },
            |o| {
                o.host.id = "another-host".into();
            },
            |o| {
                let evidence = o.capabilities.remove(&Capability::WorkspaceWrite).unwrap();
                o.host.capabilities.remove(&Capability::WorkspaceWrite);
                o.host.capabilities.insert(Capability::Network);
                o.capabilities.insert(Capability::Network, evidence);
            },
        ] {
            let mut changed = valid.clone();
            mutate(&mut changed);
            let changed = changed.seal().unwrap();
            assert!(crate::phases::load(&f.inputs(&changed, 120)).is_err());
        }
        let mut inputs = f.inputs(&valid, 120);
        inputs.store = None;
        inputs.execution_config = None;
        assert_eq!(
            crate::phases::load(&inputs).err().unwrap().code(),
            "capability.evidence.unauthenticated"
        );
        fs::write(&f.binary, b"changed executable bytes").unwrap();
        assert_eq!(
            f.inspected(120).capabilities[&Capability::WorkspaceWrite].support,
            CapabilitySupport::Unknown
        );
        assert!(crate::phases::load(&f.inputs(&valid, 120)).is_err());
    }

    #[test]
    fn failed_attempt_revokes_inspection_but_history_at_run_clock_survives() {
        let f = Fixture::new();
        let observation = f.inspected(120);
        let mut store = CapabilityProofStore::create(&f.store).unwrap();
        let mut record = f.proof.record.clone();
        record.sequence = 2;
        record.proof_id = "synthetic-failure".into();
        record.started_at_unix_seconds = 150;
        record.ended_at_unix_seconds = 160;
        record.outcome = CapabilityAttemptOutcome::Failed;
        let proof = CapabilityProofKey::derive(&f.key, store.store_id())
            .unwrap()
            .authenticate(record)
            .unwrap();
        store.append(&proof, &f.key).unwrap();
        assert_eq!(
            f.inspected(170).capabilities[&Capability::WorkspaceWrite].support,
            CapabilitySupport::Unknown
        );
        assert!(crate::phases::load(&f.inputs(&observation, 170)).is_err());
        let verifier = historical(&f.store).unwrap_or_else(|e| panic!("history {}", e.code()));
        assert!(
            ValidatedHostObservation::from_observation_with_verifier(
                observation.clone(),
                SnapshotValidation {
                    now_unix_seconds: 120,
                    max_age_seconds: 86_400,
                },
                Some(verifier.as_ref())
            )
            .is_ok()
        );
        assert!(
            ValidatedHostObservation::from_observation_with_verifier(
                observation,
                SnapshotValidation {
                    now_unix_seconds: 1200,
                    max_age_seconds: 86_400,
                },
                Some(verifier.as_ref())
            )
            .is_err()
        );
    }
    #[test]
    fn many_previous_scopes_do_not_exhaust_current_or_historical_binding_bounds() {
        let f = Fixture::new();
        let observation = f.inspected(120);
        let config = crate::execution::sandbox(&f.config).unwrap_or_else(|_| panic!("config"));
        let mut store = CapabilityProofStore::create(&f.store).unwrap();
        let key = CapabilityProofKey::derive(&f.key, store.store_id()).unwrap();
        for sequence in 2..=66 {
            let mut record = f.proof.record.clone();
            record.sequence = sequence;
            record.proof_id = format!("other-scope-{sequence}");
            record.binding = crate::execution::capability_binding(
                &config,
                &observation.host,
                "0.1.0",
                BTreeSet::from([format!("scope-{sequence}")]),
                false,
            )
            .unwrap_or_else(|_| panic!("binding"));
            store
                .append(&key.authenticate(record).unwrap(), &f.key)
                .unwrap();
        }
        assert!(crate::phases::load(&f.inputs(&observation, 120)).is_ok());
        let verifier = historical_through(&f.store, Some(1))
            .unwrap_or_else(|e| panic!("historical {}", e.code()));
        assert!(
            ValidatedHostObservation::from_observation_with_verifier(
                observation,
                SnapshotValidation {
                    now_unix_seconds: 120,
                    max_age_seconds: 300,
                },
                Some(verifier.as_ref())
            )
            .is_ok()
        );
    }
}
