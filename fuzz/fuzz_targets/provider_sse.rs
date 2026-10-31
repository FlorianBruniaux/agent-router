// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
#![no_main]

use libfuzzer_sys::fuzz_target;
use router_adapter_api::MAX_TRANSPORT_BYTES;
use router_bridge_contract::{AnthropicMessagesAdapter, OpenAiResponsesAdapter, ProviderAdapter};
use router_types::{TransportOutcome, TransportTermination};

const OUTCOME: TransportOutcome = TransportOutcome {
    termination: TransportTermination::Exited,
    exit_code: Some(0),
    elapsed_ms: 1,
};

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > MAX_TRANSPORT_BYTES {
        for adapter in [
            &OpenAiResponsesAdapter as &dyn ProviderAdapter,
            &AnthropicMessagesAdapter as &dyn ProviderAdapter,
        ] {
            assert!(
                adapter
                    .decode_stream(bytes, OUTCOME.clone(), MAX_TRANSPORT_BYTES)
                    .is_err()
            );
        }
        return;
    }
    for adapter in [
        &OpenAiResponsesAdapter as &dyn ProviderAdapter,
        &AnthropicMessagesAdapter as &dyn ProviderAdapter,
    ] {
        let first = adapter.decode_stream(bytes, OUTCOME.clone(), MAX_TRANSPORT_BYTES);
        let repeated = adapter.decode_stream(bytes, OUTCOME.clone(), MAX_TRANSPORT_BYTES);
        match (first, repeated) {
            (Ok(first), Ok(repeated)) => assert_eq!(first, repeated),
            (Err(first), Err(repeated)) => assert_eq!(first.to_string(), repeated.to_string()),
            _ => panic!("provider stream decoding must be deterministic"),
        }
    }
});
