use super::*;

#[tokio::test]
async fn displayed_patch_roundtrip_binds_exact_artifact_and_applies_once() {
    let (_t, r, s, rev) = fixture().await;
    let h = handle(&r, &s, &rev);
    let before = r.events().unwrap();
    let packet = h.presentation().await.unwrap();
    assert_eq!(packet.diff.as_bytes(), PATCH);
    assert_eq!(
        content_digest(packet.diff.as_bytes()),
        packet.review.artifact_sha256
    );
    let wire = serde_json::to_vec(&packet).unwrap();
    let decoded = serde_json::from_slice(&wire).unwrap();
    h.validate_presentation(&decoded).await.unwrap();
    assert_eq!(r.events().unwrap(), before);
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"before\n");
    h.accept_presented(&decoded, &packet.review.approval_digest)
        .await
        .unwrap();
    assert_eq!(fs::read(s.join("lib.rs")).unwrap(), b"after\n");
    assert!(
        h.accept_presented(&decoded, &packet.review.approval_digest)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn replaced_display_receipt_or_captured_patch_cannot_authorize_application() {
    for variant in 0..11 {
        let (_t, r, s, rev) = fixture().await;
        let h = handle(&r, &s, &rev);
        let mut packet = h.presentation().await.unwrap();
        let before = r.events().unwrap();
        match variant {
            0 => packet.diff = packet.diff.replace("+after", "+other"),
            1 => packet.review.artifact_sha256 = content_digest(b"different"),
            2 => packet.review.technical_record_sha256 = "0".repeat(64),
            3 => packet.diff = "a".repeat(131073),
            4 => {
                fs::write(r.store.root.join("artifacts/finite.patch"), b"replaced").unwrap();
            }
            5 => {
                fs::remove_file(r.store.root.join("artifacts/finite.patch")).unwrap();
            }
            6 => {
                let artifact = r.store.root.join("artifacts/finite.patch");
                fs::remove_file(&artifact).unwrap();
                std::os::unix::fs::symlink(s.join("lib.rs"), artifact).unwrap();
            }
            7 => {
                packet.diff = packet.diff.replace("+after", "+other");
                packet.review.artifact_sha256 = content_digest(packet.diff.as_bytes());
            }
            8 => {
                packet.review.candidate_sha256 = "0".repeat(64);
            }
            9 => {
                packet.review.run_id = "other".into();
            }
            10 => {
                fs::write(s.join("lib.rs"), b"user edit\n").unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            h.validate_presentation(&packet).await.is_err(),
            "variant {variant}"
        );
        assert!(
            h.accept_presented(&packet, &packet.review.approval_digest)
                .await
                .is_err()
        );
        assert_eq!(r.events().unwrap(), before);
        assert_eq!(
            fs::read(s.join("lib.rs")).unwrap(),
            if variant == 10 {
                b"user edit\n".as_slice()
            } else {
                b"before\n"
            }
        );
    }
}
