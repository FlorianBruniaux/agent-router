use router_doctor::{AuditInput, DoctorError, build_audit_bundle, verify_audit_bundle};
use router_types::AuditArtifactKind;

#[test]
fn bundle_projects_only_fixed_metadata_and_hashes() {
    let bytes = br#"{"schema_version":1,"prompt":"PRIVATE_PROMPT","api_key":"PRIVATE_KEY","path":"/private/home/user","custom":{"anything":"SECRET"}}"#;
    let inputs = [
        AuditInput {
            kind: AuditArtifactKind::EffectiveConfig,
            bytes,
        },
        AuditInput {
            kind: AuditArtifactKind::Events,
            bytes: br#"[{"output":"PRIVATE_LOG"}]"#,
        },
    ];
    let bundle = build_audit_bundle(&inputs).unwrap();
    let output = serde_json::to_string(&bundle).unwrap();
    for forbidden in [
        "PRIVATE_PROMPT",
        "PRIVATE_KEY",
        "PRIVATE_LOG",
        "/private/home/user",
        "SECRET",
    ] {
        assert!(!output.contains(forbidden));
    }
    assert!(bundle.records[0].metadata.contained_secret_pattern);
    assert_eq!(bundle.records[0].metadata.schema_version, Some(1));
    assert!(!bundle.raw_content_included);
    assert!(!bundle.signature_verified);
    assert!(!format!("{:?}", inputs[0]).contains("PRIVATE_KEY"));
    verify_audit_bundle(&bundle, Some(&bundle.root_hash)).unwrap();
    assert_eq!(bundle, build_audit_bundle(&inputs).unwrap());
}

#[test]
fn anchored_chain_detects_edit_reorder_truncation_and_replacement() {
    let bundle = build_audit_bundle(&[
        AuditInput {
            kind: AuditArtifactKind::Catalog,
            bytes: br#"{"schema_version":1}"#,
        },
        AuditInput {
            kind: AuditArtifactKind::Decision,
            bytes: b"{}",
        },
    ])
    .unwrap();
    let mut changed = bundle.clone();
    changed.records[0].byte_count += 1;
    assert_eq!(
        verify_audit_bundle(&changed, None),
        Err(DoctorError::Integrity)
    );
    let mut changed = bundle.clone();
    changed.records.swap(0, 1);
    assert_eq!(
        verify_audit_bundle(&changed, None),
        Err(DoctorError::Integrity)
    );
    let mut changed = bundle.clone();
    changed.records.pop();
    changed.root_hash = changed.records[0].record_hash.clone();
    verify_audit_bundle(&changed, None).unwrap();
    assert_eq!(
        verify_audit_bundle(&changed, Some(&bundle.root_hash)),
        Err(DoctorError::Integrity)
    );
    let other = build_audit_bundle(&[AuditInput {
        kind: AuditArtifactKind::Evidence,
        bytes: b"{}",
    }])
    .unwrap();
    assert_eq!(
        verify_audit_bundle(&other, Some(&bundle.root_hash)),
        Err(DoctorError::Integrity)
    );
    let mut changed = bundle;
    changed.signature_verified = true;
    assert_eq!(
        verify_audit_bundle(&changed, None),
        Err(DoctorError::Integrity)
    );
}

#[test]
fn malformed_duplicate_secret_content_cannot_enter_audit_fields() {
    for bytes in [b"bad".as_slice(), b"{\"a\":1,\"a\":2}", b"42"] {
        assert_eq!(
            build_audit_bundle(&[AuditInput {
                kind: AuditArtifactKind::Evidence,
                bytes
            }])
            .unwrap_err(),
            DoctorError::Schema
        );
    }
}
