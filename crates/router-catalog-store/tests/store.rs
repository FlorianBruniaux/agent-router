//! Synthetic publisher keys only. No production roots or native execution.
#![cfg(unix)]
use ed25519_dalek::{Signer, SigningKey};
use router_catalog::*;
use router_catalog_store::*;
use router_types::SkillTrust;
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
};
fn roots(ceiling: SkillTrust) -> TrustedCatalogPublishers {
    roots_with_seed(37, ceiling)
}
fn roots_with_seed(seed: u8, ceiling: SkillTrust) -> TrustedCatalogPublishers {
    TrustedCatalogPublishers::new([CatalogPublisherGrant {
        key_id: "synthetic".into(),
        public_key: SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes(),
        catalog_ids: BTreeSet::from(["example".into()]),
        allowed_kinds: BTreeSet::from([CatalogAssetKind::Model]),
        allowed_licenses: BTreeSet::from(["MIT".into()]),
        trust_ceiling: ceiling,
    }])
    .unwrap()
}
fn document(version: &str) -> Vec<u8> {
    let entry:SignedCatalogEntry=serde_json::from_value(json!({
        "schema_version":1,"id":"model","version":version,"provenance":{"source":"https://example.test/catalog","revision":"fixture","source_sha256":"a".repeat(64)},"license_expression":"MIT","authors":["Synthetic publisher"],"valid_from_unix_seconds":100,"valid_until_unix_seconds":1000,"declared_trust":"builtin","capabilities":[],
        "requirements":{"capabilities":[],"mutations":[],"tools":[],"compatible_hosts":[]},
        "restrictions":{"allowed_mutations":[],"allowed_sandboxes":["read_only"],"maximum_sensitivity":"public"},"context_cost_estimate":0,"deprecated_by":null,"payload_sha256":"",
        "payload":{"kind":"model","definition":{"id":"model","stages":["plan"],"efforts":["medium"],"capabilities":[],"quality_micros":500000,"cost_micros":1,"latency_micros":1,"source":"https://example.test/catalog"}},
        "search":{"kind":"model","id":"model","trust":"builtin","description":"synthetic","body":null},
        "signals":{"asset":{"kind":"model","id":"model"},"languages":{"status":"unknown"},"required_tools":{"status":"unknown"},"history":{"status":"unknown"}}
    })).unwrap();
    let manifest = seal_signed_manifest(SignedCatalogManifest {
        schema_version: 1,
        id: "example".into(),
        version: version.into(),
        valid_from_unix_seconds: 100,
        valid_until_unix_seconds: 1000,
        assets: vec![entry],
        content_sha256: String::new(),
    })
    .unwrap();
    let signature = SigningKey::from_bytes(&[37; 32])
        .sign(&catalog_signing_message("synthetic", &manifest).unwrap());
    SignedCatalogDocument {
        schema_version: 3,
        manifest,
        signature: CatalogPublisherSignature {
            key_id: "synthetic".into(),
            signature_hex: hex::encode(signature.to_bytes()),
        },
    }
    .canonical_json()
    .unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    store: CatalogStore,
    roots: TrustedCatalogPublishers,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap().join("store");
        let store = CatalogStore::create(&path, "local-fixture", "example").unwrap();
        Self {
            _temp: temp,
            path,
            store,
            roots: roots(SkillTrust::VerifiedThirdParty),
        }
    }
    fn activate(&self, version: &str, now: u64) -> CatalogHead {
        let review = self
            .store
            .review_activate(&document(version), &self.roots, now, 20)
            .unwrap();
        self.store
            .apply(&review, review.approval_digest(), &self.roots, now)
            .unwrap()
    }
}
#[test]
fn approval_cas_restart_and_catalog_projection_are_bound() {
    let f = Fixture::new();
    let initial = f.store.status().unwrap();
    assert_eq!(initial.revision, 0);
    let review = f
        .store
        .review_activate(&document("A"), &f.roots, 110, 20)
        .unwrap();
    assert!(matches!(
        f.store.apply(&review, &"0".repeat(64), &f.roots, 110),
        Err(StoreError::Approval)
    ));
    assert_eq!(f.store.status().unwrap(), initial);
    let head = f
        .store
        .apply(&review, review.approval_digest(), &f.roots, 111)
        .unwrap();
    assert_eq!(head.revision, 1);
    assert_eq!(head.previous_head, Some(initial.head_hash));
    assert!(matches!(
        f.store
            .apply(&review, review.approval_digest(), &f.roots, 112),
        Err(StoreError::Conflict)
    ));
    let store = CatalogStore::open(&f.path, f.store.config_hash(), Some(&head.head_hash)).unwrap();
    let guard = store
        .acquire_current(&head.head_hash, &f.roots, 112)
        .unwrap();
    let access = guard.access(&f.roots, 112).unwrap();
    assert_eq!(
        guard.catalog().catalog(&access).unwrap().models().count(),
        1
    );
    assert_eq!(guard.head(), &head);
    assert!(
        !serde_json::to_string(review.projection())
            .unwrap()
            .contains(f.path.to_str().unwrap())
    );
    assert!(CatalogStore::open(&f.path, &"0".repeat(64), None).is_err());
}
#[test]
fn rollback_aba_and_history_never_restore_current_authority() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let b = f.activate("B", 111);
    let again = f.activate("A", 112);
    assert_eq!(a.binding_hash, again.binding_hash);
    assert_ne!(a.head_hash, again.head_hash);
    assert_eq!(again.revision, 3);
    assert_eq!(again.previous_head, Some(b.head_hash));
    assert!(matches!(
        f.store.acquire_current(&a.head_hash, &f.roots, 113),
        Err(StoreError::Conflict)
    ));
    assert!(
        f.store
            .acquire_current(&again.head_hash, &f.roots, 1000)
            .is_err()
    );
    let history = f.store.historical(&a.head_hash, &f.roots).unwrap();
    assert_eq!(history.head(), &a);
    assert_eq!(history.document().manifest.version, "A");
    let guard = f
        .store
        .acquire_current(&again.head_hash, &f.roots, 999)
        .unwrap();
    assert!(guard.access(&f.roots, 1000).is_err());
}
#[test]
fn roots_grants_expiry_and_revocation_fail_closed_without_implicit_install() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let changed = roots(SkillTrust::TrustedLocal);
    let none = TrustedCatalogPublishers::new([]).unwrap();
    let other_key = roots_with_seed(38, SkillTrust::VerifiedThirdParty);
    assert!(
        f.store
            .acquire_current(&a.head_hash, &other_key, 111)
            .is_err()
    );
    assert!(
        f.store
            .review_activate(&document("A"), &other_key, 111, 10)
            .is_err()
    );
    assert!(matches!(
        f.store.acquire_current(&a.head_hash, &changed, 111),
        Err(StoreError::Authority)
    ));
    assert!(f.store.acquire_current(&a.head_hash, &none, 111).is_err());
    assert!(
        f.store
            .review_activate(&document("A"), &none, 111, 10)
            .is_err()
    );
    let review = f
        .store
        .review_activate(&document("B"), &f.roots, 111, 10)
        .unwrap();
    assert!(matches!(
        f.store
            .apply(&review, review.approval_digest(), &changed, 112),
        Err(StoreError::Authority)
    ));
    assert!(matches!(
        f.store
            .apply(&review, review.approval_digest(), &f.roots, 121),
        Err(StoreError::Time)
    ));
    assert_eq!(f.store.status().unwrap(), a);
    let guard = f
        .store
        .acquire_current(&a.head_hash, &f.roots, 112)
        .unwrap();
    assert!(guard.access(&none, 112).is_err());
    drop(guard);
    let revoke = f.store.review_revoke(&none, 122, 10).unwrap();
    let revoked = f
        .store
        .apply(&revoke, revoke.approval_digest(), &none, 122)
        .unwrap();
    assert!(matches!(
        f.store.acquire_current(&revoked.head_hash, &none, 122),
        Err(StoreError::Revoked)
    ));
    assert!(f.store.historical(&a.head_hash, &f.roots).is_ok());
    // Explicitly approving the existing signed bytes under new external grants
    // creates a new binding and revision. No document can supply these roots.
    let rotate = f
        .store
        .review_activate(&document("A"), &changed, 123, 10)
        .unwrap();
    let rotated = f
        .store
        .apply(&rotate, rotate.approval_digest(), &changed, 123)
        .unwrap();
    assert_ne!(rotated.binding_hash, a.binding_hash);
    assert_ne!(rotated.grants_hash, a.grants_hash);
    assert!(
        f.store
            .acquire_current(&rotated.head_hash, &changed, 124)
            .is_ok()
    );
}
#[test]
fn time_scope_signature_and_bounds_are_checked_before_commit() {
    let f = Fixture::new();
    for ttl in [0, 301, u64::MAX] {
        assert!(
            f.store
                .review_activate(&document("A"), &f.roots, 110, ttl)
                .is_err()
        );
    }
    for now in [99, 1000] {
        assert!(
            f.store
                .review_activate(&document("A"), &f.roots, now, 10)
                .is_err()
        );
    }
    let mut forged = SignedCatalogDocument::from_json(&document("A")).unwrap();
    forged.manifest.version = "forged".into();
    forged.manifest = seal_signed_manifest(forged.manifest).unwrap();
    assert!(
        f.store
            .review_activate(&forged.canonical_json().unwrap(), &f.roots, 110, 10)
            .is_err()
    );
    let a = f.activate("A", 120);
    assert!(f.store.review_revoke(&f.roots, 119, 10).is_err());
    assert!(
        f.store
            .acquire_current(&a.head_hash, &f.roots, 119)
            .is_err()
    );
    assert_eq!(f.store.status().unwrap(), a);
    let other =
        CatalogStore::create(&f.path.with_file_name("other"), "other", "other-catalog").unwrap();
    assert!(
        other
            .review_activate(&document("A"), &f.roots, 120, 10)
            .is_err()
    );
}
#[test]
fn copied_store_or_replaced_directory_is_not_the_approved_locator() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let copy = f.path.with_file_name("copy");
    fs::create_dir(&copy).unwrap();
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["catalog.lock", "state.json"] {
        fs::copy(f.path.join(name), copy.join(name)).unwrap();
    }
    assert!(matches!(
        CatalogStore::open(&copy, f.store.config_hash(), None),
        Err(StoreError::Authority)
    ));
    let review = f.store.review_revoke(&f.roots, 111, 10).unwrap();
    let other =
        CatalogStore::create(&f.path.with_file_name("other"), "local-fixture", "example").unwrap();
    assert!(
        other
            .apply(&review, review.approval_digest(), &f.roots, 111)
            .is_err()
    );
    fs::rename(&f.path, f.path.with_file_name("moved")).unwrap();
    fs::rename(copy, &f.path).unwrap();
    assert!(
        f.store
            .acquire_current(&a.head_hash, &f.roots, 111)
            .is_err()
    );
}
#[test]
fn missing_tampered_duplicated_and_restored_state_are_refused() {
    for mutation in ["pin", "document", "history", "duplicate"] {
        let f = Fixture::new();
        f.activate("A", 110);
        let path = f.path.join("state.json");
        let bytes = fs::read(&path).unwrap();
        let mut state: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let changed = match mutation {
            "pin" => {
                state["head"]["binding_hash"] = json!("0".repeat(64));
                serde_json::to_vec(&state).unwrap()
            }
            "document" => {
                state["documents"] = json!({});
                serde_json::to_vec(&state).unwrap()
            }
            "history" => {
                state["events"] = json!([]);
                serde_json::to_vec(&state).unwrap()
            }
            _ => {
                let text = String::from_utf8(bytes).unwrap();
                text.replacen(
                    "\"schema_version\":1",
                    "\"schema_version\":1,\"schema_version\":1",
                    1,
                )
                .into_bytes()
            }
        };
        fs::write(path, changed).unwrap();
        assert!(f.store.status().is_err(), "{mutation}");
    }
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let old = fs::read(f.path.join("state.json")).unwrap();
    let b = f.activate("B", 111);
    let anchored = CatalogStore::open(&f.path, f.store.config_hash(), Some(&b.head_hash)).unwrap();
    fs::write(f.path.join("state.json"), old).unwrap();
    assert!(anchored.status().is_err());
    assert!(CatalogStore::open(&f.path, f.store.config_hash(), Some(&b.head_hash)).is_err());
    // Without a later external checkpoint, a whole valid historical snapshot is
    // indistinguishable from that old state. Do not claim anti-owner rollback.
    assert_eq!(
        CatalogStore::open(&f.path, f.store.config_hash(), Some(&a.head_hash))
            .unwrap()
            .status()
            .unwrap(),
        a
    );
}
#[test]
fn private_modes_symlinks_hardlinks_and_abandoned_staging_are_checked() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    assert_eq!(
        fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(f.path.join("state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fs::set_permissions(f.path.join("state.json"), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(f.store.status().is_err());
    fs::set_permissions(f.path.join("state.json"), fs::Permissions::from_mode(0o600)).unwrap();
    let link = f.path.join("hard");
    fs::hard_link(f.path.join("state.json"), &link).unwrap();
    assert!(f.store.status().is_err());
    fs::remove_file(link).unwrap();
    fs::rename(f.path.join("state.json"), f.path.join("original")).unwrap();
    symlink("original", f.path.join("state.json")).unwrap();
    assert!(f.store.status().is_err());
    fs::remove_file(f.path.join("state.json")).unwrap();
    fs::rename(f.path.join("original"), f.path.join("state.json")).unwrap();
    assert_eq!(f.store.status().unwrap(), a);
    fs::write(
        f.path.join("state.pending"),
        b"interrupted unpublished state",
    )
    .unwrap();
    fs::set_permissions(
        f.path.join("state.pending"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let b = f.activate("B", 111);
    assert_eq!(f.store.status().unwrap(), b);
    assert!(!f.path.join("state.pending").exists());
}
#[test]
fn interprocess_reader_blocks_update_until_guard_is_released() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let guard = f
        .store
        .acquire_current(&a.head_hash, &f.roots, 111)
        .unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_lock_helper", "--nocapture"])
        .env("CATALOG_STORE_TEST_PATH", &f.path)
        .env("CATALOG_STORE_TEST_CONFIG", f.store.config_hash())
        .env("CATALOG_STORE_TEST_EXPECT", "busy")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(f.store.status().unwrap(), a);
    drop(guard);
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_lock_helper", "--nocapture"])
        .env("CATALOG_STORE_TEST_PATH", &f.path)
        .env("CATALOG_STORE_TEST_CONFIG", f.store.config_hash())
        .env("CATALOG_STORE_TEST_EXPECT", "apply")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(f.store.status().unwrap().revision, 2);
}
#[test]
fn process_lock_helper() {
    let Some(path) = std::env::var_os("CATALOG_STORE_TEST_PATH") else {
        return;
    };
    let store = CatalogStore::open(
        &PathBuf::from(path),
        &std::env::var("CATALOG_STORE_TEST_CONFIG").unwrap(),
        None,
    )
    .unwrap();
    let roots = roots(SkillTrust::VerifiedThirdParty);
    let review = store.review_revoke(&roots, 120, 20).unwrap();
    let result = store.apply(&review, review.approval_digest(), &roots, 120);
    if std::env::var("CATALOG_STORE_TEST_EXPECT").unwrap() == "busy" {
        assert!(matches!(result, Err(StoreError::Busy)));
    } else {
        assert!(result.is_ok());
    }
}
#[test]
fn document_and_history_limits_refuse_without_mutation() {
    let f = Fixture::new();
    for index in 0..MAX_DOCUMENTS {
        f.activate(&format!("version-{index}"), 110);
    }
    let before = f.store.status().unwrap();
    assert!(matches!(
        f.store
            .review_activate(&document("overflow"), &f.roots, 110, 20),
        Err(StoreError::Limit)
    ));
    assert_eq!(f.store.status().unwrap(), before);
    for _ in MAX_DOCUMENTS..MAX_REVISIONS {
        f.activate("version-0", 110);
    }
    let before = f.store.status().unwrap();
    assert_eq!(before.revision, MAX_REVISIONS as u64);
    assert!(matches!(
        f.store.review_revoke(&f.roots, 110, 20),
        Err(StoreError::Limit)
    ));
    assert_eq!(f.store.status().unwrap(), before);
}

#[test]
fn oversized_state_and_missing_lock_fail_before_authority() {
    let f = Fixture::new();
    fs::OpenOptions::new()
        .write(true)
        .open(f.path.join("state.json"))
        .unwrap()
        .set_len(MAX_SNAPSHOT_BYTES as u64 + 1)
        .unwrap();
    assert!(matches!(f.store.status(), Err(StoreError::Limit)));
    assert!(matches!(
        CatalogStore::open(&f.path, f.store.config_hash(), None),
        Err(StoreError::Limit)
    ));
    let f = Fixture::new();
    fs::remove_file(f.path.join("catalog.lock")).unwrap();
    assert!(matches!(f.store.status(), Err(StoreError::Missing)));
    assert!(CatalogStore::open(&f.path, f.store.config_hash(), None).is_err());
}

#[test]
fn active_guard_detects_directory_replacement_before_projection() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let guard = f
        .store
        .acquire_current(&a.head_hash, &f.roots, 111)
        .unwrap();
    guard.validate_publishers(&f.roots, 111).unwrap();
    let moved = f.path.with_file_name("old-locked-store");
    fs::rename(&f.path, &moved).unwrap();
    fs::create_dir(&f.path).unwrap();
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["catalog.lock", "state.json"] {
        fs::copy(moved.join(name), f.path.join(name)).unwrap();
    }
    let reopened = CatalogStore::open(&f.path, f.store.config_hash(), Some(&a.head_hash)).unwrap();
    let revoke = reopened.review_revoke(&f.roots, 112, 10).unwrap();
    let revoked = reopened
        .apply(&revoke, revoke.approval_digest(), &f.roots, 112)
        .unwrap();
    assert!(matches!(
        reopened.acquire_current(&revoked.head_hash, &f.roots, 113),
        Err(StoreError::Revoked)
    ));
    assert!(guard.validate_publishers(&f.roots, 113).is_err());
    assert!(
        guard.access(&f.roots, 113).is_err(),
        "old held guard still projects A after replacement store accepted revocation"
    );
}
#[test]
fn active_guard_detects_lock_replacement_before_projection() {
    let f = Fixture::new();
    let a = f.activate("A", 110);
    let guard = f
        .store
        .acquire_current(&a.head_hash, &f.roots, 111)
        .unwrap();
    guard.validate_publishers(&f.roots, 111).unwrap();
    fs::rename(f.path.join("catalog.lock"), f.path.join("old.lock")).unwrap();
    fs::write(f.path.join("catalog.lock"), b"").unwrap();
    fs::set_permissions(
        f.path.join("catalog.lock"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let reopened = CatalogStore::open(&f.path, f.store.config_hash(), Some(&a.head_hash)).unwrap();
    let revoke = reopened.review_revoke(&f.roots, 112, 10).unwrap();
    let revoked = reopened
        .apply(&revoke, revoke.approval_digest(), &f.roots, 112)
        .unwrap();
    assert!(matches!(
        reopened.acquire_current(&revoked.head_hash, &f.roots, 113),
        Err(StoreError::Revoked)
    ));
    assert!(guard.validate_publishers(&f.roots, 113).is_err());
    assert!(
        guard.access(&f.roots, 113).is_err(),
        "old held guard still projects A after lock replacement accepted revocation"
    );
}
