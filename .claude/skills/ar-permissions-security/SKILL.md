---
name: ar-permissions-security
description: "Permissions and snapshot security in Agent Router: inspect authority, stale evidence, subprocess and filesystem boundaries, and advisory sidecars."
---

# ar-permissions-security

## Triggers

Review of permission propagation, snapshots, subprocess inputs, filesystem paths or a proposed sidecar contract.

## Non-triggers

Granting permissions, running models, implementing sidecars in issue #12, generic style review.

## Inputs

Threat surface, source paths, permission owner and redacted evidence.

## Steps

1. Trace each input to its side effect and identify the host authority. Done when every effect has an enforcement point.
2. Check stale snapshots, symlinks, traversal, bounded reads, secret redaction and argument separation. Done when each applicable risk has a test or explicit UNKNOWN.
3. Verify that a sidecar can only advise within Rust-validated constraints. Done when it cannot discover capabilities, grant access or cause execution.

## Checks

Run from the repository root, using `rtk` when available.

```sh
cargo test -p xtask --locked
cargo test -p router-policy --test constraints --locked
cargo test -p router-policy --test retrieval --locked
```

## Stop

Stop on secret access, permission escalation or absent authority. Static validation cannot prove a sandbox or a deployed security control.

This is a textual skill, not an executable extension. Host permissions still apply.
