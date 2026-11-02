// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! `stable-v1` is frozen (ADR 0054): every request fixture routed over every
//! built-in catalog must keep producing the same decision bytes. The golden holds
//! the SHA-256 of each serialized decision and was generated from the code
//! before `stable-v2` existed.
use std::{fs, path::PathBuf};

use router_catalog::Catalog;
use router_policy::Router;
use router_types::RouteRequest;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const CATALOGS: [&str; 4] = [
    "claude-core-v1",
    "codex-core-v1",
    "codex-core-v2",
    "codex-core-v3",
];
const GOLDEN: &str = "tests/golden/stable-v1-fixtures.json";

fn root(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("router-policy crate is nested beneath the workspace root")
        .join(path)
}

fn actual() -> String {
    let mut requests = fs::read_dir(root("tests/fixtures/requests"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    requests.sort();
    let mut out = Map::new();
    for catalog_name in CATALOGS {
        let bytes = fs::read(root(&format!("assets/catalogs/{catalog_name}.json"))).unwrap();
        let catalog = Catalog::from_json(&bytes).unwrap();
        for path in &requests {
            let Ok(request) = serde_json::from_slice::<RouteRequest>(&fs::read(path).unwrap())
            else {
                // Not a route request (for example a retrieval document).
                continue;
            };
            let key = format!(
                "{}@{catalog_name}",
                path.file_name().unwrap().to_string_lossy()
            );
            let value = match Router::stable_v1().route(&request, &catalog) {
                Ok(decision) => {
                    let status = if decision.selected_plan().is_some() {
                        "selected"
                    } else {
                        "no_route"
                    };
                    let bytes = serde_json::to_vec(&decision).unwrap();
                    serde_json::json!({"status": status, "sha256": hex::encode(Sha256::digest(bytes))})
                }
                Err(error) => serde_json::json!({"status": "error", "code": error.code()}),
            };
            out.insert(key, value);
        }
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(out)).unwrap();
    text.push('\n');
    text
}

#[test]
fn v1_policy_is_frozen_for_all_fixtures() {
    let actual = actual();
    let golden = fs::read_to_string(root("crates/router-policy").join(GOLDEN)).unwrap_or_default();
    assert!(
        actual == golden,
        "stable-v1 output moved from {GOLDEN}; v1 is frozen, do not regenerate"
    );
}
