---
adr: "0024"
title: Accepted revision accounting
status: implemented
created: 2026-10-07
last_modified: 2026-10-07
scope: "Offline supplied-evidence accounting, F07a."
amended_by: []
supersedes: []
superseded_by: []
related: ["0019"]
---

# ADR 0024: Accepted revision accounting

## Context

The existing task measurement report can say that a task has an accepted run.
It does not bind that outcome to the selected exact revision. Editing an
accepted result must not silently give the new revision its predecessor's
acceptance, or charge later work to an earlier acceptance frontier.

## Decision

Add a pure `router-feedback` calculation and
`agent-router eval accepted-changes --input PATH --json`. Both the standalone
input and report have `schema_version: 1`. They do not change the store, existing
feedback schemas, report hashes, immutable `record_fact`, or runtime behavior.
These feature-owned contracts are not entries in the public schema registry.

All evidence is supplied offline. `evidence_trust: supplied_unverified` means
that binding checks passed, not that a provider, reviewer or invoice was
authenticated. No provider call, hourly rate, quota conversion, allocation,
invoice reconciliation, spending authority or wall-clock result is introduced.

### Sealed cohort and exact revision history

`cohort` contains `complete: true` and every assignment, including assignments
without work or output. Each assignment binds `assignment_id`, `start_sequence`,
`task_start_sha256`, `source_sha256` and `criteria_sha256`. `cohort_sha256` is the
SHA-256 of `accepted-changes/cohort/v1`, a zero byte, and the compact Serde JSON
encoding of that cohort. Assignment array order is significant. The producer
can call `accepted_cohort_sha256` to produce this content seal. This seal checks
membership integrity within the supplied input; it cannot prove that the
producer disclosed every assignment.

`analysis_sequence` identifies the supplied global evidence snapshot. Each
assignment, revision, verdict and unique physical observation has a positive,
distinct sequence at or before that snapshot. Expected inventory entries name
the observation's same sequence. Revision and verdict arrays must be strictly
ordered. These are evidence append positions, not timestamps.

Every `revisions` entry contains `sequence`, `binding` and
`predecessor_sha256`. A binding names assignment, source, criteria, candidate
and exact revision digests. Candidate and revision are distinct supplied
identities; the calculator neither reads nor hashes a patch. The first revision
has no predecessor; the next names the immediately previous revision for that
assignment. Source and criteria must exactly match the sealed assignment.

`selections` contains exactly one entry per assignment. An existing revision
must be selected explicitly, even when an older revision is chosen. A null
revision is permitted only when the assignment has no recorded revision. The
latest verdict for that exact binding decides the selected status; absent
verdicts and assignments without revisions are `unknown`.

`verdicts` is an append-only supplied history with `accepted`, `rejected`,
`pending`, `unknown` and `revoked` states. Every entry names the exact binding.
Acceptance requires a `passed` verifier outcome, supplied verifier and reviewer
evidence digests, and an explicit `cost_frontier_sequence`. A process exit or
timeout cannot supply that verdict. The frontier must be at least the revision
sequence and at most the acceptance sequence, itself no later than the analysis
snapshot. Revocation names `acceptance_sequence` for an earlier acceptance of
the same binding and supplies a reviewer digest. A duplicated revocation or a
revocation of another revision is refused. The target must be the currently
authoritative accepted verdict for that revision: revoking an older acceptance
after a newer acceptance is refused. Reacceptance after revocation needs a new
explicit accepted event with its own supplied evidence. Revocation remains in history and
does not erase the acceptance or its historical cost.

This calculator validates a supplied history, not an authenticated append-only
log. An external producer owns durable storage and preservation of earlier
snapshots. Re-sealing rewritten history cannot establish historical truth.

### Physical observations and explicit attribution

An observation identity is exactly `(authority_sha256, physical_sha256)`.
The independently supplied authority namespace and physical identity are both
required. `external_request_id` is optional display metadata and never a join
key. Identical repeated observations count once. A different cost, source,
kind, binding, sequence or other field under the same key is a conflict. A real
retry has a new physical key and counts again even if an external ID repeats.

Each observation declares `kind: model | ci | human`,
`attribution: cohort_once`, its cohort digest, sequence, evidence digest,
`agent_outcome`, coverage and nullable supplied `cost_microusd`. Model and human
observations have exactly one work link. CI can link several assignments or
revisions, but its physical cost counts once for the cohort. The report
preserves every link without inventing a per-assignment split.

A work link binds assignment, source and criteria. Its candidate and revision
digests are either both present or both null. Present digests must match a
recorded revision exactly. Null digests allow failures that produced no
artifact. `failed`, `timed_out`, `cancelled` and `failed_before_send` remain
separate supplied agent outcomes. A missing observation does not establish a
failed-before-send outcome, zero cost or zero usage. Supplied off-cohort rows
are preserved and counted separately but excluded from cohort costs. They
cannot satisfy this cohort's inventory.

### Coverage and historical cost frontiers

There is at most one inventory per cost kind. It declares `coverage` and the
expected physical keys with their immutable observation sequence. Every
provided in-cohort observation must appear in its category inventory when one
is present. Inventory identities cannot be reused between categories, and
conflicting sequence or cohort joins fail. An absent inventory keeps supplied
subtotals but makes completeness unknown.

A cost category reports `known_subtotal_microusd`, nullable `total_microusd`,
`observation_count`, `missing_expected_count` and `inventory_coverage`.
A complete total requires complete inventory coverage, all expected physical
observations, supplied cost for each, and complete usage and pricing coverage.
`last_turn_only`, `partial` or `unknown` coverage never establishes a complete
cost. A partial supplied amount contributes only to the known subtotal.
Explicit complete, empty inventory establishes zero for that category; absence
establishes unknown. No number is inferred from legacy aggregate `RunReport`.

Each historical acceptance reports `cohort_costs_at_frontier`, summing all
cohort observations through its own frontier, including failed or rejected assignments, correction attempts and
shared CI. Expected but missing observations at or before that frontier make
the historical total unknown. Later observations and expectations do not alter
that subtotal. The coverage declaration applies to the whole supplied
inventory, so partial inventory remains conservatively incomplete at earlier
frontiers too. Sequences bound supplied observed/completed work, not accrued
but undisclosed work, future work, invoice adjustments or elapsed time.

`assignment_model_cost_at_frontier` separately sums model work linked to the
accepted assignment, including its previous failures and corrections. It
excludes unrelated assignments and provides no per-assignment CI split. Because
v1 expected identities do not carry assignment attribution, any missing model
identity through the frontier conservatively makes this assignment total null;
its attributable known subtotal is still reported.

The analysis-level `cohort_costs` includes all cohort observations through the
snapshot. `selected_accepted_changes` counts selected revisions whose current
supplied status is accepted. `cost_per_selected_accepted_change` has separate
model, CI and human rational values `{numerator_microusd, denominator}` using
the full cohort numerator. A missing total or zero denominator yields null.
The historical frontier figures are not summed into that numerator.

### Determinism, bounds and verification

Inputs are capped at 4 MiB before strict JSON decoding. Unknown fields and
duplicate keys are rejected. There are at most 256 assignments, 4096 revisions,
4096 verdicts, 4096 observation snapshots, three inventories and 4096 expected
observations per inventory. Work links are bounded by the assignment cap.
Costs use checked integer addition. No floating-point division is performed.

Reports sort selected assignments and physical observation keys, preserve the
ordered revision/verdict histories and emit inventories in model/CI/human
order. `input_sha256` hashes the typed input with domain
`accepted-changes/input/v1`; input array order remains significant.
`report_sha256` hashes the report with its own field set to the empty string
and domain `accepted-changes/report/v1`. Both use the cohort hashing convention
above. Repeated identical input produces identical output bytes.

The committed fixture at `tests/fixtures/evaluation/accepted-changes-v1.json`
is synthetic. Its failed model attempt costs 100 microusd, the attempt yielding
A costs 200, shared CI costs 50, supplied human cost is 30, and later B costs
400. A's frontier therefore has cohort model subtotal 300 and assignment model
subtotal 200; the analysis cohort subtotal is 700. B has no acceptance. Those numbers exercise arithmetic and evidence
boundaries and make no claim about live usage or router gains.
