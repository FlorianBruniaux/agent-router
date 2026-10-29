// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Authored offline corpus. No model, provider or process is executed.
#[path = "support/finite_context_corpus.rs"]
mod corpus;

#[test]
fn corpus_preserves_authored_outputs_and_refusals() {
    let report = corpus::report();
    assert_eq!(report["rows"].as_array().unwrap().len(), 26);
    assert_eq!(report["refusals"].as_array().unwrap().len(), 3);
    assert_eq!(
        report["combined_selection"]["raw_json_bytes"],
        report["summary_by_mode"]["exact"]["raw_json_bytes"]
    );
    assert_eq!(
        report["combined_selection"]["raw_content_bytes"],
        report["summary_by_mode"]["exact"]["raw_content_bytes"]
    );
}

#[test]
fn offline_context_report_is_byte_stable() {
    let first = corpus::bytes();
    assert_eq!(first, corpus::bytes());
    let expected = include_bytes!("../../../docs/bench/rtk-context-offline/report.json");
    if let Some(path) = std::env::var_os("AR_CONTEXT_OFFLINE_REPORT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&first).unwrap();
    }
    assert!(
        first.as_slice() == expected,
        "export and review the authored report before recording it"
    );
}

#[test]
fn rereads_can_make_net_volume_negative() {
    let report = corpus::report();
    let row = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["fixture_id"] == "stderr_only" && r["mode"] == "view")
        .expect("stderr reread scenario missing");
    let raw = row["raw_content_bytes"].as_i64().unwrap();
    let shown = row["shown_content_bytes"].as_i64().unwrap();
    let recalled = row["recalled_content_bytes"].as_i64().unwrap();
    assert_eq!(recalled, raw);
    assert!(shown > 0);
    assert!(raw - shown - recalled < 0);
    assert!(row["serialized_bytes"].as_u64().unwrap() > row["raw_json_bytes"].as_u64().unwrap());
}

#[test]
fn byte_saving_does_not_fill_unknown_usage_or_success() {
    let report = corpus::report();
    assert!(
        report["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["shown_content_bytes"].as_u64().unwrap()
                < r["raw_content_bytes"].as_u64().unwrap())
    );
    for key in [
        "billed_tokens",
        "provider_cost",
        "qualified_pricing",
        "model_success",
    ] {
        assert_eq!(report["evidence"][key], "unknown");
    }
    assert_eq!(report["evidence"]["native_linux_arm64"], "not_run");
    let serialized = String::from_utf8(corpus::bytes()).unwrap();
    assert!(!serialized.contains("mismatched types"));
    assert!(!serialized.contains("/Users/"));
    assert!(!serialized.contains("raw_handle"));
}

#[test]
fn aggregate_uses_compatible_totals_and_keeps_negative_values() {
    use serde_json::json;
    let rows = [
        json!({"raw_content_bytes":10,"shown_content_bytes":0,"recalled_content_bytes":0,"raw_json_bytes":20,"serialized_bytes":10}),
        json!({"raw_content_bytes":90,"shown_content_bytes":90,"recalled_content_bytes":90,"raw_json_bytes":180,"serialized_bytes":400}),
    ];
    let total = corpus::summarize(&rows);
    assert_eq!(total["net_content_bytes"], -80);
    assert_eq!(total["content_reduction_basis_points"], -8000);
    assert_eq!(total["net_json_bytes"], -210);
    assert_eq!(total["json_reduction_basis_points"], -10500);
    let empty = corpus::summarize(&[]);
    assert!(empty["content_reduction_basis_points"].is_null());
    assert!(empty["json_reduction_basis_points"].is_null());
}

#[test]
fn incomplete_results_are_stops_not_successful_raw_fallbacks() {
    let report = corpus::report();
    let fallbacks = report["fallbacks"].as_array().unwrap();
    assert_eq!(fallbacks.len(), 4);
    assert!(
        !fallbacks
            .iter()
            .any(|f| f["fixture_id"] == "already_truncated")
    );
    assert_eq!(
        report["stopped_fixtures"],
        serde_json::json!([
            {"fixture_id":"long_line","reason":"admitted_output_truncated"},
            {"fixture_id":"already_truncated","reason":"capture_incomplete"},
        ])
    );
}
