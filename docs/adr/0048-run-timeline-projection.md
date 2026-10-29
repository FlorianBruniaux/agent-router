---
adr: "0048"
title: Bounded read-only run timeline projection
status: implemented
created: 2026-10-28
last_modified: 2026-10-28
scope: "Opt-in observation v4 timeline for #118; native facts, trusted timestamps and acceptance remain unknown."
amended_by: []
supersedes: []
superseded_by: []
related: ["0025", "0030"]
---

# ADR 0048: Bounded read-only run timeline projection

## Context

[#118](https://github.com/FlorianBruniaux/agent-router/issues/118) asks for one
readable chronological view of a run. Observation v1 to v3
([ADR 0025](0025-run-observation.md), [ADR 0030](0030-qualified-local-capture-timing.md))
give a snapshot, per-attempt facts and local capture intervals, but only the last
20 sequence/state references in the human view and no separation of plan,
approval, attempt, transport, verification, cancellation and cleanup. The journal
has no trusted timestamp, and its detail field is private and untrusted.

## Decision

`router_runtime::observe_run_v4(root, id)` and `observe --schema-version 4`
return the v3 report with `schema_version: 4` and an added `timeline`. The
digest domain is `run-observation-v4`. The v1 to v3 projection code is not modified in the diff, and the frozen v1 test
passes; v2 and v3 have no golden file. The v3 `known.events` list (up to 4,096
references) remains in the v4 JSON; the 64 limit bounds `timeline.entries` only. The default stays v1.

The timeline is built from the selected run's events inside the same deferred
read transaction as v3, after the same row/byte admission and full-chain check.
No ownership, recovery, migration, artifact read, retry, approval or network
access is added.

- **Closed vocabulary.** An event is published as sequence, hash, recorded state,
  a `phase` and a `kind` label from a fixed allowlist in
  `store/timeline.rs`. Any other journal kind is `unclassified` in phase
  `other`. Event detail is never published. The JSON Schema enum of kinds is
  pinned to the allowlist by a test.
- **Bounds.** `timeline.entries` holds at most 64 entries, the most recent ones, in ascending journal
  order. `omitted_earlier` and `run_events` state the cut. A fixed ten-row phase
  summary counts every event, including omitted ones. The selected run is already
  capped at 4,096 references.
- **Distinctions.** `approval` reports state transitions of the latest approval cycle only (a retry returns to pending). `cancellation`
  is the logical request and terminal state; `cleanup` is the recorded cleanup
  and the journal markers; `physical_cleanup` stays unknown. `entries` flag
  events recorded after the first cancel request, and each attempt reports
  whether its finished marker is after it.
- **Requested versus applied.** The requested model and effort come from
  validated attempt evidence (as in v2). The applied selection is unknown.
- **Usage.** Attempt evidence token and cost values are shown as
  `recorded_unattested`, `partial` or `unknown`; an absent value is never zero.
- **Unknown by construction.** Acceptance of the exact revision, notification
  delivery, lost notifications, event timestamps, physical cleanup and the
  objective have no source in the store and are fixed to `unknown`.
- **Concurrent append.** `later_global_events` counts journal events committed
  after the run's last event (other runs), so a reader sees that the horizon moved.

## Consequences

Human and JSON views are two renderings of one value. Rendering the same
snapshot twice is byte-identical. A refusal (corrupt chain, missing store, limits)
has no timeline; there is no partial success.

`attempt` on an entry is positional (the latest attempt start at or before the
event) and not an ordinal binding from the journal. Phase for `transition` events
is derived from the recorded state. Attempt-evidence usage is not native
accounting. Durations stay owned by #117.

Adding a journal kind requires adding its label to the allowlist and the schema
enum, or it appears as `unclassified`.

## Rejected options

- Printing event detail: it is private and can carry untrusted text.
- Paginating with a cursor: a second snapshot may differ, and the reader would
  have to trust a cursor. The bounded window plus the JSON horizon identify the
  snapshot instead.
- Inferring acceptance, applied model or notification loss from neighbouring
  events: no producer writes them.
