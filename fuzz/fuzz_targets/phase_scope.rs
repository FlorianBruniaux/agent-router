#![no_main]

use libfuzzer_sys::fuzz_target;
use router_policy::{MAX_PHASE_REQUEST_BYTES, PhaseExecutionScope, asset_authority_scope_digest};
use router_types::PhaseRouteRequest;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > MAX_PHASE_REQUEST_BYTES + 8192 {
        return;
    }
    let Ok((request, scope)) =
        serde_json::from_slice::<(PhaseRouteRequest, PhaseExecutionScope)>(bytes)
    else {
        return;
    };

    let first = asset_authority_scope_digest(&request, &scope);
    let repeated = asset_authority_scope_digest(&request, &scope);
    match (first, repeated) {
        (Ok(first), Ok(repeated)) => {
            assert_eq!(first, repeated);
            assert_eq!(first.len(), 64);
            assert!(
                first
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            );

            let mut reordered_request = request.clone();
            reordered_request.candidates.reverse();
            let mut reordered_scope = scope.clone();
            reordered_scope.scope.reverse();
            assert_eq!(
                first,
                asset_authority_scope_digest(&reordered_request, &reordered_scope).unwrap()
            );
        }
        (Err(_), Err(_)) => {}
        _ => panic!("phase scope validation must be deterministic"),
    }
});
