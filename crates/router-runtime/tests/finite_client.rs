//! Synthetic files, authenticated private proof store, real loopback listeners.
//! No native binary, provider turn or global configuration is used.
use router_adapter_api::{
    CapabilityAttemptOutcome, CapabilityNetworkScope, CapabilityProofKey, CapabilityProofRecord,
    CapabilityProofTier, EvidenceKind, EvidenceSource, content_digest,
};
use router_runtime::{
    ApprovedFiniteClient, CapabilityProofStore, FiniteClientReview, OwnedBroker, ResourceLimits,
    StoredCapabilityProofVerifier,
};
use router_types::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::TcpListener,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifest: FiniteClientManifest,
    run: PreparedRun,
    broker: Option<OwnedBroker>,
    host: HostSnapshot,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = temp.path().canonicalize().unwrap();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let state = root.join("state");
        fs::create_dir(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        let pin = |name: &str, executable: bool| {
            let path = root.join(name);
            fs::write(&path, name).unwrap();
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if executable { 0o500 } else { 0o400 }),
            )
            .unwrap();
            FiniteClientFile {
                path: path.to_str().unwrap().into(),
                sha256: content_digest(name.as_bytes()),
            }
        };
        let broker =
            OwnedBroker::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap()).unwrap();
        let manifest = FiniteClientManifest {
            schema_version: 1,
            task_id: "task-1".into(),
            host_snapshot_id: "fixture-host".into(),
            client: pin("client", true),
            native: pin("native", true),
            helper: pin("helper", true),
            wrapper: pin("wrapper", true),
            profile: pin("profile", false),
            configuration: pin("configuration", false),
            private_state: state.to_str().unwrap().into(),
            native_version: "fixture-1".into(),
            broker_endpoint: broker.endpoint().to_owned(),
            broker_instance_sha256: broker.instance_sha256().into(),
        };
        let model = ModelId::parse("fixture-model").unwrap();
        let host = HostSnapshot {
            id: "fixture-host".into(),
            harness: HarnessId::parse("codex").unwrap(),
            supported_models: BTreeSet::from([model.clone()]),
            supported_efforts: BTreeMap::from([(
                model.clone(),
                BTreeSet::from([EffortLevel::Low]),
            )]),
            capabilities: BTreeSet::from([Capability::Network]),
        };
        let run = PreparedRun {
            schema_version: 1,
            receipt_id: "a".repeat(64),
            source_state: "b".repeat(40),
            scope: vec!["patch.txt".into()],
            authorization_digest: "c".repeat(64),
            snapshot_hash: "d".repeat(64),
            host: host.harness.clone(),
            requested_model: model,
            requested_effort: EffortLevel::Low,
            program: "finite-client".into(),
            argv: vec!["--worker".into()],
            cwd: workspace.to_str().unwrap().into(),
            stdin: PreparedInput::new(b"PRIVATE_TASK_CANARY".to_vec()),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            sandbox: ExecutionSandbox::WorkspaceWrite,
            timeout_ms: 3000,
            max_output_bytes: 4096,
            degradations: vec![],
            missing_runtime_evidence: vec![],
            parent_model_changed: false,
        };
        Self {
            _temp: temp,
            root,
            manifest,
            run,
            broker: Some(broker),
            host,
        }
    }
    fn review(&mut self) -> FiniteClientReview {
        FiniteClientReview::new(
            self.manifest.clone(),
            &self.run,
            self.broker.take().unwrap(),
        )
        .unwrap()
    }
    fn approve(&mut self) -> ApprovedFiniteClient {
        let review = self.review();
        let digest = review.approval_digest().to_owned();
        review.approve(&digest).unwrap()
    }
    fn approve_codex(&mut self, limits: ResourceLimits) -> ApprovedFiniteClient {
        let broker = self.broker.take().unwrap();
        let store = self.root.join("preparation");
        fs::create_dir(&store).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
        let state = broker.prepare_codex_state(&store, &self.run).unwrap();
        self.manifest.configuration = state.configuration().clone();
        self.manifest.private_state = state.path().to_str().unwrap().into();
        let review = FiniteClientReview::with_codex_state(
            self.manifest.clone(),
            &self.run,
            broker,
            limits,
            state,
        )
        .unwrap();
        let digest = review.approval_digest().to_owned();
        review.approve(&digest).unwrap()
    }
    fn proof(
        &self,
        approved: &ApprovedFiniteClient,
    ) -> (
        EvidenceSource,
        StoredCapabilityProofVerifier,
        CapabilityProofRecord,
    ) {
        let dir = self.root.join("proofs");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let mut store = CapabilityProofStore::create(&dir).unwrap();
        let key = CapabilityProofKey::derive(&[19; 32], store.store_id()).unwrap();
        let binding = approved.proof_binding(&self.host.id).unwrap();
        let record = CapabilityProofRecord {
            schema_version: 1,
            proof_id: "first".into(),
            sequence: 1,
            store_id: store.store_id().into(),
            capability: Capability::Network,
            tier: CapabilityProofTier::Proven,
            network_scope: Some(CapabilityNetworkScope::LoopbackFetch),
            binding: binding.clone(),
            model_id: "fixture-model".into(),
            nonce_sha256: "e".repeat(64),
            expected_effect_sha256: "f".repeat(64),
            started_at_unix_seconds: 99,
            ended_at_unix_seconds: 100,
            max_age_seconds: 60,
            outcome: CapabilityAttemptOutcome::Succeeded,
        };
        let proof = key.authenticate(record.clone()).unwrap();
        let hash = proof.digest().unwrap();
        store.append(&proof, &[19; 32]).unwrap();
        drop(store);
        let source = EvidenceSource {
            id: "first".into(),
            kind: EvidenceKind::HostRuntime,
            observed_version: Some("fixture-1".into()),
            observed_at_unix_seconds: 100,
            content_sha256: hash,
        };
        let verifier =
            StoredCapabilityProofVerifier::open_existing(&dir, [19; 32], vec![binding]).unwrap();
        (source, verifier, record)
    }
}

#[test]
fn validates_exact_preparation_without_launching_or_releasing_listener() {
    let mut f = Fixture::new();
    let endpoint = f.manifest.broker_endpoint.clone();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let guard = a.validate(&f.run, &f.host, &s, &v, 101).unwrap();
    assert_eq!(guard.broker_endpoint(), endpoint);
    assert!(TcpListener::bind(endpoint.strip_prefix("tcp://").unwrap()).is_err());
    let _ = guard;
    drop(a);
    assert!(TcpListener::bind(endpoint.strip_prefix("tcp://").unwrap()).is_ok());
}
#[test]
fn changed_stdin_is_refused_even_when_serialized_prepared_preview_is_equal() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let mut changed = f.run.clone();
    changed.stdin = PreparedInput::new(b"REPLACED_TASK".to_vec());
    assert_eq!(
        serde_json::to_value(&changed).unwrap(),
        serde_json::to_value(&f.run).unwrap()
    );
    assert!(a.validate(&changed, &f.host, &s, &v, 101).is_err());
}
#[test]
fn wrong_review_digest_cannot_approve() {
    let mut f = Fixture::new();
    assert!(f.review().approve(&"0".repeat(64)).is_err());
}
#[test]
fn loopback_address_alone_cannot_replace_listener_instance() {
    let mut f = Fixture::new();
    f.manifest.broker_instance_sha256 = "0".repeat(64);
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
}
#[test]
fn wrong_endpoint_is_refused_before_any_preparation() {
    let mut f = Fixture::new();
    f.manifest.broker_endpoint = "tcp://127.0.0.1:1".into();
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
}
#[test]
fn changed_assets_and_file_permissions_refuse_old_approval() {
    for name in [
        "client",
        "native",
        "helper",
        "wrapper",
        "profile",
        "configuration",
    ] {
        let mut f = Fixture::new();
        let a = f.approve();
        let (s, v, _) = f.proof(&a);
        let path = f.root.join(name);
        let original_mode = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, b"REPLACED").unwrap();
        fs::set_permissions(&path, original_mode).unwrap();
        assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err(), "{name}");
    }
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    fs::set_permissions(f.root.join("native"), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
}
#[test]
fn identical_replacement_and_symlink_are_not_old_file_identity() {
    for link in [false, true] {
        let mut f = Fixture::new();
        let a = f.approve();
        let (s, v, _) = f.proof(&a);
        fs::rename(
            f.root.join("configuration"),
            f.root.join("old-configuration"),
        )
        .unwrap();
        if link {
            symlink(
                f.root.join("old-configuration"),
                f.root.join("configuration"),
            )
            .unwrap();
        } else {
            fs::copy(
                f.root.join("old-configuration"),
                f.root.join("configuration"),
            )
            .unwrap();
        }
        assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
    }
}
#[test]
fn private_state_replacement_or_shared_permissions_refuse() {
    for replace in [false, true] {
        let mut f = Fixture::new();
        let a = f.approve();
        let (s, v, _) = f.proof(&a);
        let state = f.root.join("state");
        if replace {
            fs::rename(&state, f.root.join("old-state")).unwrap();
            fs::create_dir(&state).unwrap();
            fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        } else {
            fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
    }
}
#[test]
fn changed_task_scope_model_effort_or_host_refuse() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let mutations: [fn(&mut PreparedRun); 6] = [
        |r| r.scope.push("extra.txt".into()),
        |r| r.requested_model = ModelId::parse("other-model").unwrap(),
        |r| r.requested_effort = EffortLevel::High,
        |r| r.argv.push("--extra".into()),
        |r| r.receipt_id = "e".repeat(64),
        |r| r.authorization_digest = "e".repeat(64),
    ];
    for mutate in mutations {
        let mut changed = f.run.clone();
        mutate(&mut changed);
        assert!(a.validate(&changed, &f.host, &s, &v, 101).is_err());
    }
    let mut host = f.host.clone();
    host.id = "other-host".into();
    assert!(a.validate(&f.run, &host, &s, &v, 101).is_err());
}
#[test]
fn stale_future_or_historical_proof_cannot_validate_preparation() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    for now in [98, 160, 161] {
        assert!(a.validate(&f.run, &f.host, &s, &v, now).is_err(), "{now}");
    }
    assert!(
        a.validate(&f.run, &f.host, &s, &v.through_sequence(1), 101)
            .is_err()
    );
}
#[test]
fn later_authenticated_failure_is_seen_on_revalidation() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, mut record) = f.proof(&a);
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_ok());
    record.sequence = 2;
    record.proof_id = "later-failure".into();
    record.outcome = CapabilityAttemptOutcome::Failed;
    record.started_at_unix_seconds = 101;
    record.ended_at_unix_seconds = 101;
    let mut store = CapabilityProofStore::create(&f.root.join("proofs")).unwrap();
    let key = CapabilityProofKey::derive(&[19; 32], store.store_id()).unwrap();
    store
        .append(&key.authenticate(record).unwrap(), &[19; 32])
        .unwrap();
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
}
#[test]
fn forged_source_and_missing_store_fail_closed() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (mut s, v, _) = f.proof(&a);
    s.content_sha256 = "0".repeat(64);
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
    fs::rename(f.root.join("proofs"), f.root.join("old-proofs")).unwrap();
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
}
#[test]
fn bound_contract_does_not_enable_product_broker_sandbox() {
    use router_runtime::{DenySandbox, NetworkAuthority, SandboxPolicy, SandboxSupervisor};
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let _guard = a.validate(&f.run, &f.host, &s, &v, 101).unwrap();
    assert!(
        DenySandbox
            .command(
                &f.run,
                &SandboxPolicy {
                    network: NetworkAuthority::ControllerBrokerOnly,
                    host_state: None
                }
            )
            .is_err()
    );
}

#[test]
fn held_broker_is_the_listener_that_accepts_the_controller_connection() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let guard = a.validate(&f.run, &f.host, &s, &v, 101).unwrap();
    let control =
        std::net::TcpStream::connect(guard.broker_endpoint().strip_prefix("tcp://").unwrap())
            .unwrap();
    let (accepted, peer) = guard.broker_listener().accept().unwrap();
    assert_eq!(peer, control.local_addr().unwrap());
    assert_eq!(accepted.local_addr().unwrap(), control.peer_addr().unwrap());
}
#[test]
fn wildcard_listener_is_never_a_loopback_broker() {
    assert!(OwnedBroker::from_listener(TcpListener::bind(("0.0.0.0", 0)).unwrap()).is_err());
}
#[test]
fn malformed_manifest_and_oversized_input_refuse_at_review() {
    let mutations: [fn(&mut FiniteClientManifest); 4] = [
        |m| m.schema_version = 2,
        |m| m.task_id = "../task".into(),
        |m| m.native.sha256 = "bad-hash".into(),
        |m| m.private_state = "relative-state".into(),
    ];
    for mutate in mutations {
        let mut f = Fixture::new();
        mutate(&mut f.manifest);
        assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
    }
    let mut f = Fixture::new();
    f.run.stdin = PreparedInput::new(vec![b'x'; 32769]);
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
    let f = Fixture::new();
    let mut value = serde_json::to_value(&f.manifest).unwrap();
    value["extra"] = "unexpected".into();
    assert!(serde_json::from_value::<FiniteClientManifest>(value).is_err());
}
#[test]
fn assets_inside_writable_workspace_and_oversized_configuration_refuse() {
    let mut f = Fixture::new();
    let replacement = PathBuf::from(&f.run.cwd).join("configuration");
    fs::copy(&f.manifest.configuration.path, &replacement).unwrap();
    f.manifest.configuration.path = replacement.to_str().unwrap().into();
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
    let mut f = Fixture::new();
    fs::set_permissions(
        &f.manifest.configuration.path,
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::write(&f.manifest.configuration.path, vec![0; 65537]).unwrap();
    f.manifest.configuration.sha256 = content_digest(&vec![0; 65537]);
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
}
#[test]
fn nonregular_asset_refuses_without_waiting_for_fifo_writer() {
    let mut f = Fixture::new();
    let fifo = f.root.join("fifo");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRUSR).unwrap();
    f.manifest.configuration.path = fifo.to_str().unwrap().into();
    assert!(FiniteClientReview::new(f.manifest, &f.run, f.broker.take().unwrap()).is_err());
}
#[test]
fn authenticated_proof_for_another_preparation_is_not_reusable() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (mut s, _v, mut record) = f.proof(&a);
    record.sequence = 2;
    record.proof_id = "different-binding".into();
    record.binding.argument_template_sha256 = "0".repeat(64);
    let mut store = CapabilityProofStore::create(&f.root.join("proofs")).unwrap();
    let key = CapabilityProofKey::derive(&[19; 32], store.store_id()).unwrap();
    let proof = key.authenticate(record.clone()).unwrap();
    s.id = record.proof_id;
    s.content_sha256 = proof.digest().unwrap();
    store.append(&proof, &[19; 32]).unwrap();
    drop(store);
    let v = StoredCapabilityProofVerifier::open_existing(
        &f.root.join("proofs"),
        [19; 32],
        vec![record.binding],
    )
    .unwrap();
    assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
}

#[test]
fn authenticated_proof_for_another_host_does_not_change_reviewed_host_identity() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (mut s, _v, mut record) = f.proof(&a);
    let mut other = f.host.clone();
    other.id = "replacement-host".into();
    record.sequence = 2;
    record.proof_id = "replacement-host-proof".into();
    record.binding.host_snapshot_id = other.id.clone();
    let mut store = CapabilityProofStore::create(&f.root.join("proofs")).unwrap();
    let key = CapabilityProofKey::derive(&[19; 32], store.store_id()).unwrap();
    let proof = key.authenticate(record.clone()).unwrap();
    s.id = record.proof_id;
    s.content_sha256 = proof.digest().unwrap();
    store.append(&proof, &[19; 32]).unwrap();
    drop(store);
    let v = StoredCapabilityProofVerifier::open_existing(
        &f.root.join("proofs"),
        [19; 32],
        vec![record.binding],
    )
    .unwrap();
    assert!(a.validate(&f.run, &other, &s, &v, 101).is_err());
}

#[test]
fn concrete_resource_limits_are_checked_before_review() {
    let mut f = Fixture::new();
    let limits = ResourceLimits {
        cpu_seconds: 0,
        ..ResourceLimits::default()
    };
    assert!(
        FiniteClientReview::with_resources(
            f.manifest.clone(),
            &f.run,
            f.broker.take().unwrap(),
            limits
        )
        .is_err()
    );
}

#[test]
fn each_changed_resource_limit_is_refused_by_fresh_preparation() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    let limits = ResourceLimits::default();
    assert!(
        a.validate_with_resources(&f.run, &f.host, &s, &v, 101, &limits)
            .is_ok()
    );
    for changed in [
        ResourceLimits {
            cpu_seconds: 301,
            ..limits.clone()
        },
        ResourceLimits {
            address_space_bytes: None,
            ..limits.clone()
        },
        ResourceLimits {
            max_open_files: 257,
            ..limits.clone()
        },
        ResourceLimits {
            max_processes: 257,
            ..limits.clone()
        },
        ResourceLimits {
            max_file_bytes: 1024,
            ..limits.clone()
        },
    ] {
        assert!(
            a.validate_with_resources(&f.run, &f.host, &s, &v, 101, &changed)
                .is_err()
        );
    }
}

#[test]
fn owned_codex_state_produces_private_config_and_survives_until_approval_drop() {
    let mut f = Fixture::new();
    let broker = f.broker.take().unwrap();
    let store = f.root.join("preparation");
    fs::DirBuilder::new()
        .recursive(false)
        .create(&store)
        .unwrap();
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
    let state = broker.prepare_codex_state(&store, &f.run).unwrap();
    let home = state.path().to_owned();
    let config = state.configuration().clone();
    assert_eq!(
        fs::metadata(&home).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&config.path).unwrap().permissions().mode() & 0o777,
        0o400
    );
    let text = fs::read_to_string(&config.path).unwrap();
    assert!(text.contains("model = \"fixture-model\""));
    assert!(text.contains("model_reasoning_effort = \"low\""));
    assert!(text.contains(&format!(
        "base_url = \"http://{}/v1\"",
        broker.endpoint().strip_prefix("tcp://").unwrap()
    )));
    assert!(text.contains("request_max_retries = 0"));
    assert!(text.contains("shell_tool = false"));
    assert!(!home.join("auth.json").exists());
    f.manifest.configuration = config;
    f.manifest.private_state = home.to_str().unwrap().into();
    let review = FiniteClientReview::with_codex_state(
        f.manifest.clone(),
        &f.run,
        broker,
        ResourceLimits::default(),
        state,
    )
    .unwrap();
    let digest = review.approval_digest().to_owned();
    let a = review.approve(&digest).unwrap();
    let (s, v, _) = f.proof(&a);
    let guard = a.validate(&f.run, &f.host, &s, &v, 101).unwrap();
    if cfg!(target_os = "macos") {
        let plan = guard.launch_plan().unwrap();
        let value = serde_json::to_value(&plan).unwrap();
        assert_eq!(value["supervised"]["program"], f.manifest.helper.path);
        assert_eq!(
            value["supervised"]["argv"],
            serde_json::json!([
                "--cpu-seconds",
                "300",
                "--address-space-bytes",
                "8589934592",
                "--max-open-files",
                "256",
                "--max-processes",
                "256",
                "--max-file-bytes",
                "16777216",
                "--supervise-v1",
                f.manifest.wrapper.path,
                "-f",
                f.manifest.profile.path,
                f.manifest.client.path,
                "--worker"
            ])
        );
        assert_eq!(value["native"]["program"], f.manifest.native.path);
        assert_eq!(
            value["native"]["argv"],
            serde_json::json!(["app-server", "--listen", "stdio://"])
        );
        assert_eq!(
            value["native"]["environment"]["CODEX_HOME"],
            f.manifest.private_state
        );
        assert_eq!(value["native"]["environment"].as_object().unwrap().len(), 4);
        assert_eq!(
            value["stdin_sha256"],
            content_digest(f.run.stdin.as_bytes())
        );
        assert!(
            !serde_json::to_string(&plan)
                .unwrap()
                .contains("PRIVATE_TASK_CANARY")
        );
    } else {
        assert!(guard.launch_plan().is_err());
    }
    assert!(home.exists());
    drop(a);
    assert!(!home.exists());
}

#[test]
fn state_production_refuses_task_workspace_before_creating_files() {
    let f = Fixture::new();
    let broker = f.broker.as_ref().unwrap();
    fs::set_permissions(&f.run.cwd, fs::Permissions::from_mode(0o700)).unwrap();
    let before = fs::read_dir(&f.run.cwd).unwrap().count();
    assert!(
        broker
            .prepare_codex_state(PathBuf::from(&f.run.cwd).as_path(), &f.run)
            .is_err()
    );
    assert_eq!(fs::read_dir(&f.run.cwd).unwrap().count(), before);
}

#[test]
fn manifest_only_review_cannot_present_a_native_launch_plan() {
    let mut f = Fixture::new();
    let a = f.approve();
    let (s, v, _) = f.proof(&a);
    assert!(
        a.validate(&f.run, &f.host, &s, &v, 101)
            .unwrap()
            .launch_plan()
            .is_err()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn launch_preview_refuses_workspace_change_after_guard_validation() {
    let mut f = Fixture::new();
    let a = f.approve_codex(ResourceLimits::default());
    let (s, v, _) = f.proof(&a);
    let guard = a.validate(&f.run, &f.host, &s, &v, 101).unwrap();
    assert!(guard.launch_plan().is_ok());
    fs::set_permissions(&f.run.cwd, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(guard.launch_plan().is_err());
}

#[test]
fn generated_tmp_directory_substitution_and_shared_modes_refuse() {
    for mode_change in [false, true] {
        let mut f = Fixture::new();
        let a = f.approve_codex(ResourceLimits::default());
        let (s, v, _) = f.proof(&a);
        assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_ok());
        let tmp = PathBuf::from(&f.manifest.private_state).join("tmp");
        if mode_change {
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        } else {
            fs::rename(&tmp, tmp.with_file_name("previous-tmp")).unwrap();
            fs::create_dir(&tmp).unwrap();
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(a.validate(&f.run, &f.host, &s, &v, 101).is_err());
    }
}

#[test]
fn configuration_generated_for_another_model_or_listener_cannot_be_reviewed() {
    for changed_listener in [false, true] {
        let mut f = Fixture::new();
        let broker = f.broker.take().unwrap();
        let store = f.root.join("preparation");
        fs::create_dir(&store).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
        let state = broker.prepare_codex_state(&store, &f.run).unwrap();
        f.manifest.configuration = state.configuration().clone();
        f.manifest.private_state = state.path().to_str().unwrap().into();
        let broker = if changed_listener {
            let b =
                OwnedBroker::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap()).unwrap();
            f.manifest.broker_endpoint = b.endpoint().into();
            f.manifest.broker_instance_sha256 = b.instance_sha256().into();
            b
        } else {
            f.run.requested_model = ModelId::parse("other-model").unwrap();
            broker
        };
        assert!(
            FiniteClientReview::with_codex_state(
                f.manifest,
                &f.run,
                broker,
                ResourceLimits::default(),
                state
            )
            .is_err()
        );
    }
}
