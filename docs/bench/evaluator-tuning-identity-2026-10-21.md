# Tuning case identity: local verification, 2026-10-21

Base: merged main `65c0392b6ec0f4162abd4522e5811ab4b55cb459` (PR #193).
This #174 slice strengthens the supplied-evidence prerequisite for #132.

A legacy input with nonempty tuning output hashes previously produced a
conclusive comparison despite lacking tuning case identities. The regression
failed before the fix. Schema 2 now requires explicit tuning ID sets and blocks
case reuse even with different output bytes or a different evaluator version.
Legacy tuning without IDs is readable but inconclusive.

## Verified locally

Host: macOS ARM64, Rust 1.85.1. Cargo dependency access was offline.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | PASS |
| `cargo test --workspace --all-features --locked --offline` | PASS |
| `cargo run -p xtask --locked --offline -- check` | PASS |
| `python3 scripts/check-adrs.py` | PASS (Homebrew Python) |
| `gitleaks dir . --config .gitleaks.toml --redact --no-banner` | PASS |

Workspace output contains 196 result summaries: 1934 reported passes,
0 failures and 43 ignored results. These sums include nested helper output;
they are not counts of unique test definitions. The owning qualification suite
passes 22 tests; the offline CLI suite passes 5 tests, including the new v2
inconclusive result and absence of state creation. Both are included in the
workspace results.

Before the implementation, the 22-test qualification suite had 17 passes and
5 expected failures. The same suite passed all 22 after the correction.

## Tested source identities

| File | SHA-256 |
| --- | --- |
| `crates/router-eval/src/evaluator_qualification.rs` | `d4086414c0b170c57b834a07cba33f1a81461b9b41b51e2deca7908d2127b38d` |
| `crates/router-eval/tests/evaluator_qualification.rs` | `afe31b01b9c8c81cb6491a7f93fbf2f5efe8becf1b706353354b3b12f3b17030` |
| `crates/agent-router-cli/tests/offline_evidence.rs` | `c159b28bc59273325ba43152d36c09b58c8cb3d4a889cb3e8869db73c78465a3` |

## Evidence limits and remaining work

The tests use synthetic supplied evidence. Oracle authenticity remains
`supplied_unverified`; adoption remains false. Neither inventory completeness
nor semantic independence follows from matching case IDs. Task/cohort/witness
seals retain their previous domains and contents.

The patch changes no runtime, sandbox, permissions or private-access code.
The 8 GiB limit and zero-swap policy are unchanged. No model, provider or real
campaign registry was used. Native ignored tests were not opted into, and the
Linux isolation replay was not repeated for this pure offline comparison change.
Hosted CI and an independent review are not claimed by this local report.

#174 remains open for independent oracle labels/provenance, execution under
recorded limits, semantic hold-out qualification, durable history and comparable
real outcomes. #132 still needs its independent useful-task acceptance and other
native-entry/lifecycle criteria. This report grants no execution authority.
