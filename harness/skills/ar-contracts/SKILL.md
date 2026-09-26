---
name: ar-contracts
description: "Agent Router routing contracts and invariants: assess deterministic selection, constraints, trust and JSON compatibility."
---

# ar-contracts

## Triggers

Changes to selection, exclusions, ranking, limits or versioned JSON.

## Non-triggers

Host discovery implementation, harness installation, general Rust formatting.

## Inputs

Issue scope, affected crate and a request/catalog fixture.

## Steps

1. Read [JSON contracts](../../../docs/reference/json-contracts.md) and locate the enforcing symbol. Done when each changed invariant has a source path.
2. Check that hard constraints precede scoring, unknown required high-risk capability excludes, and untrusted skills cannot improve utility. Done when the relevant existing tests are identified.
3. Add the smallest positive and negative contract cases. Done when targeted tests pass and byte stability or explicit contract version changes are recorded.

## Checks

Run from the repository root, using `rtk` when available.

```sh
cargo test -p router-types -p router-catalog -p router-policy --locked
```

## Stop

An incompatible contract or absent fixture blocks implementation until its expected behavior is decided. Do not turn a catalog claim into a runtime capability.

This is a textual skill, not an executable extension. Host permissions still apply.
