---
name: ar-rust-quality
description: "Rust quality gate for Agent Router: run formatting, warnings-denied Clippy, workspace tests and generated harness parity."
---

# ar-rust-quality

## Triggers

Before a scoped commit or when the user requests the Rust verification gate.

## Non-triggers

Architecture redesign, audit of provider permissions, implementing a new feature.

## Inputs

Working directory, changed path list and available toolchain.

## Steps

1. Inspect Git status and preserve other work. Done when the changed paths and current branch are recorded.
2. Run the commands below from the repository root. Done when every exit status is recorded, including failures.
3. Run Gitleaks when installed; otherwise report unavailable locally and require the CI scan. Done when the result is distinct from cargo audit and cargo deny availability.

## Checks

Run from the repository root, using `rtk` when available.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo run -p xtask --locked -- check
gitleaks dir . --redact --no-banner
```

## Stop

A failed gate blocks a passing verdict. Do not install cargo audit or cargo deny to satisfy a local checklist; report missing tools.

This is a textual skill, not an executable extension. Host permissions still apply.
