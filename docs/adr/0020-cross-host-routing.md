---
adr: "0020"
title: Cross-host phase routing
status: proposed
created: 2026-09-30
last_modified: 2026-09-30
scope: "Design only. No code implements it; single-host routing stays the default."
amended_by: []
supersedes: []
superseded_by: []
related: ["0002", "0003", "0009", "0012", "0019"]
---

# ADR 0020: Cross-host phase routing

## Context

One routing decision cannot compare a Claude model with a Codex model. Facts
read on `main` at `c43d289`:

- `ValidatedPhaseContext` holds one `HostObservation`
  (`crates/router-policy/src/phases.rs:69`).
- A candidate whose model the snapshot host does not list is excluded with
  `model.host.unsupported` (`crates/router-policy/src/phases.rs:468-469`).
- `PhaseCandidate` has no host field (`crates/router-types/src/phases.rs:94-111`)
  and `PhaseReceipt` binds one `snapshot_hash`
  (`crates/router-types/src/phases.rs:208`).
- Preparation takes the execution host from the snapshot
  (`crates/router-prepare/src/lib.rs:444`), and authorization refuses a request
  whose host differs from the snapshot harness
  (`crates/router-adapter-api/src/execution.rs:465`).
- A replay input carries one snapshot (`crates/router-eval/src/replay.rs:104`).
- The CLI picks the adapter from the request host
  (`crates/agent-router-cli/src/execution.rs:495-503`).
- The utility of a candidate depends on that candidate, its catalog model and
  the policy, not on the rest of the pool (`score`,
  `crates/router-policy/src/phases.rs:777-795`). Order, abstention below the
  threshold and ambiguity against the runner-up are computed over the eligible
  list (`crates/router-policy/src/phases.rs:230-235`, `:322-332`).
- Admission and session budgets key a run by the provider of its bound asset
  contract, not by the host name (`crates/router-runtime/src/lib.rs:1921-1927`,
  `crates/router-runtime/src/admission/runtime.rs:68-72`).

The [router value protocol](../bench/router-value-protocol.md) therefore runs
its router arm once per provider, and lists cross-host analyses as v2 work.

## Decision

Adopt option B: route each host with its own validated context, then select
across hosts with a separate, versioned cross-host step.

- Each host keeps its current `ValidatedPhaseContext`, snapshot, receipt and
  validation. No existing wire type, hash domain or receipt version changes.
- A new pure function in `router-policy` takes two or more validated contexts
  for the same phase. It refuses the set unless every context has the same
  task, phase, restrictions, policy hash and catalog hash, and a distinct host
  harness. Candidate identity across hosts is the pair `(harness, candidate id)`.
- The step merges the eligible lists read from the validated contexts, not
  from receipts, and applies the existing order and abstention rules to the
  union: preferred model, then utility, then `(harness, candidate id)`. The
  threshold and ambiguity checks use the global first and second candidates.
  A host whose context cannot be built (stale or missing evidence) is listed in
  the result with an explicit code and contributes no candidate.
- The result is a new `CrossHostReceipt` in `router-types`, versioned and
  `deny_unknown_fields`. It binds the ordered host list, each per-host context
  hash and receipt hash, the selected `(harness, candidate id)` and the
  alternatives. The selected host's own `PhaseReceipt` stays the dispatch input.
- Runtime review recomputes the cross-host step from the same contexts, as it
  recomputes the per-host route today (`crates/router-runtime/src/lib.rs:427-429`).
  The execution definition, and so the approval digest, binds the
  `CrossHostReceipt` hash.
- Dispatch runs on the selected host only. Its capability evidence must be
  current and its sandbox verified, exactly as for a single-host run. The
  snapshots of the other hosts influenced the choice and grant nothing.
- Cross-host selection is opt-in per call. Without it, routing, preparation,
  the local API and replay behave as today.

## Consequences

- Single-host callers, receipts and replay inputs are unchanged, so the v1
  per-provider protocol stays valid.
- A cross-host choice is only as meaningful as the catalog scale it compares:
  `quality_micros` and cost estimates across providers are catalog
  declarations, not measurements. The v2 analyses of the value protocol measure
  them; this decision does not.
- Budgets (ADR 0009) need no new dimension: the selected candidate's provider
  bucket applies at reservation, as today. A cross-host call reserves nothing
  on the hosts it did not select.
- Measurement (ADR 0019) records route facts from the cross-host result when
  one exists: global rank, margin to the global runner-up, alternatives across
  hosts and the selected harness. Usage semantics differ by provider (open
  questions in the value protocol), so cost comparisons across hosts inherit
  those UNKNOWN fields.
- Every host involved must be observed before routing, which costs one fresh
  snapshot per host and per call.
- Escalation, fallback after an unavailable candidate and generated plans stay
  within one host. Extending them across hosts needs its own ADR.
- The local API (ADR 0012) stays single-host. Accepting several snapshots in
  `POST /v1/route` changes its closed envelope and needs its own decision.
- Not verified: whether one coordinated wave already accepts steps whose
  contexts carry different hosts. Each coordinator step receives its own
  context (`crates/router-coordinator/src/lib.rs:262`), but a mixed-host wave
  was not run.

## Rejected options

A, a multi-host context with a host bound to each candidate. It gives one
ranking in one receipt, but it changes `PhaseCandidate`, `PhaseReceipt`,
`ReplayInput`, the pool and snapshot hash domains, and the host check of
authorization. Asset availability documents are bound to one host snapshot
(ADR 0003), so each would need a per-host variant. A stale snapshot for any host
would invalidate the whole context. Option B reaches the same selection for a
pointwise score without these changes.

C, single-host routing only, with comparison per provider. It is the current
behavior and stays the default. As the end state it forces the caller to choose
the host before routing and leaves the v2 analyses of the value protocol
impossible.

## Implementation batches

1. This ADR, status `proposed`. No code.
2. `CrossHostReceipt` in `router-types` and the pure selection function in
   `router-policy`, with tests: deterministic output under host reordering,
   ambiguity between candidates of two hosts, a host excluded for stale
   evidence, and refusal of unequal policy or catalog hashes.
3. Replay: a cross-host replay input made of per-host replay inputs, so the v2
   arms of the value protocol can be evaluated offline.
4. Runtime and CLI: the execution definition binds the cross-host receipt hash,
   runtime review recomputes it, and dispatch uses the selected per-host
   context unchanged.
5. Feedback route facts from the cross-host result, extending the fields of
   ADR 0019 batch 4.

Batch 3 depends on batch 2. Batch 4 depends on batch 2. Batch 5 depends on
batch 4.
