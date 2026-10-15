---
adr: "0035"
title: "Transfer finite-client resources to the namespace lifecycle"
status: implemented
created: 2026-10-15
last_modified: 2026-10-15
scope: "Preparation ownership and opt-in Linux diagnostic; no product dispatch authority"
amended_by: []
supersedes: []
superseded_by: []
related: ["0028", "0032", "0033"]
---

# ADR 0035: Transfer finite-client resources to the namespace lifecycle

## Decision

`ApprovedFiniteClient::into_namespace_state` consumes the preparation owner and transfers its producer-created home, retained startup identities, five private executable/profile copies and exact bound broker into HostState. Revalidate the frozen review and startup pins before transfer and before exposure. Refuse manifest-only construction. The existing approval format and serialized startup binding remain unchanged.

The bundle refuses the generic native sandbox before adding any home-write grant. A qualified dedicated backend must retain this state, arm its exact private helper lease in trusted PID-namespace mode and retain the opaque matching cleanup lock through disposal. Group-only evidence cannot exclude detached writers. Pending invalidation under that lock can dispose an unstarted bundle, including after an unsupported-mode refusal.

Prepared Drop releases the home, copies and listener. Exposed Drop deliberately retains the whole bundle until controller exit, including the listener and startup descriptors. This conservative process-local quarantine leaks resources; it neither proves process termination nor implements crash recovery. Durable filesystem obligations remain UNKNOWN. A cleanup failure does not authorize reconstructing an owner from a JSON path. Outside processes with the same UID remain outside the sandbox guarantee.

## Diagnostic composition

The opt-in Linux diagnostic uses the unchanged resource helper, a retained bubblewrap copy, the Landlock/seccomp fixture and the copied finite client/native executable. The reviewed profile binds the namespace binary and arguments. The helper supervises bubblewrap outside the inner PID namespace, so its completion marker follows namespace-owner exit. Read-only root mounts and explicit writable files constrain the workload. The outer container has no network and retains an independent teardown fallback.

The driver observes a detached writer before stopping. Normal return, deadline and stdin EOF may dispose the bundle only after the helper's matching namespace-clean proof. Helper death leaves an active lease: even an independently stable heartbeat cannot upgrade that lease or release the broker. Keep the bundle reserved until external container destruction.

The fixtures and diagnostic rows are not authenticated capability evidence. They do not construct a PendingRun, admit an ApprovedRun or enable ControllerBrokerOnly. Actual execution still requires a dedicated backend, fresh OS evidence and durable admission. The selected native RPC initializes and starts an empty thread without a model turn. F12a remains NO_GO.

The address-space and container bounds remain exactly 8589934592 bytes, with no swap allowance. No provider trial is part of this decision.

## Verification

[Namespace-owner evidence](../bench/linux-namespace-owner-2026-10-15/README.md) separates inert ownership regressions, selected real Linux diagnostic cases and local quality checks. See [PID namespace semantics](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html) for the kernel boundary; recorded runtime observations qualify only the tested composition.
