---
name: ar-host-adapter
description: "Agent Router host adapter development: define capability evidence, provenance and host protocol contracts for an explicitly scoped adapter issue."
---

# ar-host-adapter

## Triggers

An issue explicitly authorizes a host adapter or capability probe contract.

## Non-triggers

Issue #12 harness work, model execution, global host configuration, retrieval engines.

## Inputs

Approved adapter issue, documented host protocol, captured redacted fixtures and capability fields.

## Steps

1. Read the issue boundary and [technical specification](../../../docs/specs/technical-specification.md). Done when permitted host and operations are listed.
2. Define typed inputs and outputs, provenance, observation date, unsupported and UNKNOWN outcomes. Done when unknown evidence cannot become support.
3. Write protocol fixtures for supported, unsupported, malformed and unavailable host responses. Done when the adapter-specific tests pass with no live credentials.

## Checks

Run from the repository root, using `rtk` when available.

```sh
cargo test --workspace --locked
```

## Stop

Without an explicitly authorized adapter issue, stop. This skill does not implement HostProbe or authorize provider calls.

This is a textual skill, not an executable extension. Host permissions still apply.
