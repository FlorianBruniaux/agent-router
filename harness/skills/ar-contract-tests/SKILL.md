---
name: ar-contract-tests
description: "Contract tests and fixtures for Agent Router: reproduce failures, define negative cases and verify externally observable JSON and hook behavior."
---

# ar-contract-tests

## Triggers

Adding a regression, protocol fixture or test for a specified behavior.

## Non-triggers

Running the entire release gate without a test change, architecture redesign.

## Inputs

Reproduction, expected output or exit code, owning crate and fixture provenance.

## Steps

1. Reproduce the failing public behavior with sanitized deterministic inputs. Done when the expected assertion fails for the right reason.
2. Cover success, no-op, invalid input, unavailable dependency and refusal when applicable. Done when each case maps to an assertion on output or effects.
3. Run the owning test target and inspect fixture differences. Done when the regression passes without changing unrelated snapshots.

## Checks

Run from the repository root, using `rtk` when available.

```sh
cargo test -p xtask --locked
cargo test -p router-policy --test fixture_matrix --locked
cargo test -p agent-router-cli --locked
```

## Stop

Never copy provider responses, credentials or personal data into fixtures. Missing expected behavior must be resolved before recording a golden file.

This is a textual skill, not an executable extension. Host permissions still apply.
