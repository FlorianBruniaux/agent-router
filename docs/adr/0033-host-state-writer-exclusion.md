---
adr: "0033"
title: "Require writer exclusion for host-state disposal"
status: implemented
created: 2026-10-15
last_modified: 2026-10-15
scope: "Runtime attempts and native-version probes; no new finite-client execution authority"
amended_by: []
supersedes: []
superseded_by: []
related: ["0007", "0032"]
---

# ADR 0033: Require writer exclusion for host-state disposal

## Decision

Before polling a capture that can expose HostState, pin the exact private helper lease and mark the state as possibly running. An armed owner never deletes its home or private executable on Drop, including async abandonment and failed proof validation. State that has never been exposed keeps its existing prepared-state cleanup behavior.

Delete exposed state only under an opaque CleanupProof whose locked file matches the retained lease inode. NotStarted is admissible because recovery invalidates pending under that same exclusive lock, preventing a delayed helper from starting. NamespaceTerminated additionally requires the trusted supervisor's PID-namespace mode. GroupTerminated cannot exclude detached descendants and is refused. The lock spans home/program disposal and the durable attempt or probe settlement.

Before dispatch, the existing private LeaseRecord persists namespace_required and host_state_pending for an attempt or probe with HostState. Clear host_state_pending only after the live owner finishes filesystem cleanup. Recovery cannot replace that owner: any pending filesystem obligation keeps authority quarantined, even with a valid process-stop marker. A crash after deletion but before commit therefore remains unknown. This lot deliberately does not add automatic moved-directory recovery or reconstruct an owner from an arbitrary path.

The two internal record fields default to false when absent, preserving the previous scope of historical leases. They do not backfill historical filesystem-cleanup evidence. Helper framing/markers, approval formats, public observation schemas and capability evidence are unchanged.

## Trust and compatibility

The trusted OS sandbox must exclude workload writes to the runtime parent, helper lease and executable-copy directories. Once its PID namespace is destroyed, those workers cannot race recursive deletion. The lease lock serializes the trusted helper and controller; it does not constrain an unrelated same-UID process outside that sandbox. Retained directory identities still refuse substitutions already present before removal.

A macOS HostState invocation with only group-clean now fails closed and preserves its state. This is intentional: that evidence cannot exclude detached writers. Commands without HostState retain their existing process-group behavior. The existing Linux bubblewrap supervisor is the product namespace mode; a JSON flag, test fixture or external diagnostic receipt does not qualify it.

FiniteCodexState preparation and the external Linux copied-client diagnostic are not routed through the product dispatch changed here. Their owners must still be retained through external teardown; this ADR gives them no new execution path or blanket cleanup guarantee. Binding the finite client to an approved namespace owner remains an F12a prerequisite.

The address-space default remains exactly 8589934592 bytes (8 GiB). ControllerBrokerOnly still refuses production execution. No model/provider trial, memory-contract change or F12a GO follows from this decision.

## Verification

[Writer-exclusion evidence](../bench/host-state-writer-exclusion-2026-10-15.md) records the reproduced early-Drop and restart failures, inert lease protocol tests, a real setsid writer, durable refusal/settlement cases and selected local checks. Namespace marker fixtures test control flow, not OS isolation. Native composition remains separately gated.
