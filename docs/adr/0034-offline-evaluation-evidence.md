---
adr: "0034"
title: Keep evaluator qualification and supplied usage accounting separate
status: implemented
created: 2026-10-15
last_modified: 2026-10-21
scope: "Offline supplied-evidence evaluation and pricing; no native execution authority."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0034: Keep evaluator qualification and supplied usage accounting separate

## Context

An equal aggregate pass count can hide a lost required case. A lower token count
cannot establish an improvement when the evaluator, case membership or cost
coverage changed. Existing replay and accepted-revision accounting have different
responsibilities: neither authenticates a supplied oracle or collects provider
usage.

## Decision

Keep two feature-owned, bounded offline contracts instead of introducing a new
runtime journal or a second cost calculator:

- `router-eval` checks supplied evaluator qualification and comparisons by
  required-case identity. Missing or noncomparable evidence remains inconclusive.
  An output re-evaluation retains earlier verdicts and does not create a new
  execution or cost. Qualification witnesses remain supplied claims; structural
  consistency does not authenticate an independent oracle or authorize adoption.
- `router-feedback` joins explicitly supplied per-physical-call usage to existing
  cost observations, applies qualified pricing and reuses accepted-revision
  accounting. Only complete priced totals populate a physical observation's
  cost. Partial subtotals are retained separately. Supplied inventory and usage
  coverage are not upgraded by the bridge.
- The CLI reads one explicitly selected file for each command. A separately
  selected portable pricing projection contains allowlisted hashes, enums and
  numbers. The full accounting report remains local and may contain supplied
  identifiers; it is not a portable export.

Both contracts reject duplicate JSON keys, unknown fields, oversized input and
conflicting identities. Errors omit supplied values. They retain
`supplied_unverified` evidence. Estimated prices are not invoices, credits or
observed savings. Existing B1, observation v2/v3, feedback and replay contracts
remain unchanged.

## Amendment: tuning case identity (2026-10-21)

Evaluator qualification schema 2 adds an explicit complete supplied tuning case
ID set to each evaluator qualification. Both IDs and output hashes are excluded
from decision evidence across all supplied versions. Their sets are separate
because a tuning case may produce several outputs. Empty inventories must be
explicit and consistent; every case ID consumes the existing nested-row budget.

Schema 1 remains readable. Nonempty legacy tuning hashes without case identity
produce an inconclusive comparison rather than a conclusive held-out claim.
Task, cohort and witness seals retain their v1 domains and serialized contents.
Tuning inventories remain unauthenticated supplied claims. This amendment adds
no evaluator executor, persistent store or runtime authority.

## Consequences

The retained M4 evidence can be projected conservatively into the existing
accounting contract: one accepted Router revision, all three attempts visible,
unknown monetary totals. A host response hash cannot be promoted into a provider
physical-call identity. The previous stopped M4 attempt has a distinct scope.

Independent oracle provenance, authenticated producer collection, complete cost
coverage and a comparable repeated campaign remain separate work. The new
offline reports neither admit native execution nor relax memory, network or
approval checks. No real trial is part of this implementation.

## Validation

See [evaluator qualification](../evaluator-qualification.md),
[supplied usage pricing](../supplied-usage-pricing.md) and the
[retained accepted evidence](../bench/accepted-evidence-2026-10-15/README.md)
for executable examples, negative cases and the remaining proof limits.
