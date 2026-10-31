// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use libfuzzer_sys::fuzz_target;
use router_adapter_api::{
    HostObservation, MAX_SNAPSHOT_BYTES, SnapshotValidation, ValidatedHostObservation,
};

const VALIDATION: SnapshotValidation = SnapshotValidation {
    now_unix_seconds: 100,
    max_age_seconds: 10,
};

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        assert!(ValidatedHostObservation::from_json(bytes, VALIDATION).is_err());
        return;
    }
    let first = ValidatedHostObservation::from_json(bytes, VALIDATION);
    let repeated = ValidatedHostObservation::from_json(bytes, VALIDATION);
    match (first, repeated) {
        (Ok(first), Ok(repeated)) => {
            assert_eq!(first.hash(), repeated.hash());
            assert_eq!(first.observation(), repeated.observation());
        }
        (Err(first), Err(repeated)) => assert_eq!(first.code(), repeated.code()),
        _ => panic!("snapshot validation must be deterministic"),
    }

    if let Ok(observation) = serde_json::from_slice::<HostObservation>(bytes) {
        let sealed = observation.clone().seal();
        let repeated = observation.seal();
        match (sealed, repeated) {
            (Ok(sealed), Ok(repeated)) => {
                assert_eq!(sealed, repeated);
                let first = ValidatedHostObservation::from_observation(sealed, VALIDATION);
                let second = ValidatedHostObservation::from_observation(repeated, VALIDATION);
                match (first, second) {
                    (Ok(first), Ok(second)) => {
                        assert_eq!(first.observation(), second.observation())
                    }
                    (Err(first), Err(second)) => assert_eq!(first.code(), second.code()),
                    _ => panic!("sealed snapshot validation must be deterministic"),
                }
            }
            (Err(first), Err(second)) => assert_eq!(first.code(), second.code()),
            _ => panic!("snapshot sealing must be deterministic"),
        }
    }
});
