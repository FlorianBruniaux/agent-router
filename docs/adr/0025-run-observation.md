---
adr: "0025"
title: Bounded passive run observation
status: implemented
created: 2026-10-07
last_modified: 2026-10-07
scope: "F04a: consistent redacted runtime-store projection; F04b remains open."
amended_by: []
supersedes: []
superseded_by: []
related: ["0010", "0018", "0019", "0024"]
---

# ADR 0025: Bounded passive run observation

## Context

[#116](https://github.com/FlorianBruniaux/agent-router/issues/116) requires
one consistent observation of a run. Existing `observe`, `observe_cleanup` and
`observe_events` open separate readers. Combining them can mix committed states
from different journal horizons. `measurement_inventory` already demonstrates
one passive read transaction over the existing runtime store.

The journal is private and contains untrusted detail. Its chain verifies byte
integrity and sequence continuity, not authenticity against a same-user writer.
`RuntimeEvent` has no trusted timestamp. A cancellation request and a terminal
state do not prove that a process stopped.

## Decision

`router_runtime::observe_run(root, id)` returns a serialize-only
`RunObservation` schema version 1. It reads the existing `runtime.sqlite3`
through one deferred read transaction. State, cleanup dependencies, chain
verification, event references and the global journal horizon share that
transaction. Existing `status` and `events` contracts are unchanged. The
[CLI usage guide](../run-observation.md) describes the two views and refusals.

Passive means no supervision ownership, recovery, migration, transition,
forced checkpoint, lease probing or permit release. An absent root/database
is refused without creation. SQLite may create or update WAL/SHM coordination
files; passive reading does not mean zero filesystem activity.

### Fields and provenance

| Field | Source and meaning |
| --- | --- |
| `schema_version` | Projection contract, currently 1 |
| `snapshot_sha256` | Domain-separated digest of the complete redacted projection with its own digest field set to the empty string |
| `identity.run_key_sha256` | Existing feedback run key: content digest of `(run_id, definition_hash, approval_digest)` |
| `identity.definition_sha256` | Validated digest from the persisted run summary |
| Other identity digests | Domain-separated hashes of receipt, plan and node identifiers; no raw identifier is exposed |
| `horizon` | Last sequence and event hash of the entire verified journal, including other runs |
| `known.state`, `attempts`, `failure` | Persisted summary, cross-checked against selected journal state and durable attempt ordinals |
| `known.cleanup` | Existing persisted cleanup interpretation, requiring resolved attempt leases and safe preparation/admission finalization; unresolved, missing, blocked or ambiguous evidence stays `unknown` |
| `known.events` | All selected-run event sequence/state/hash references in sequence order; private kind/detail are omitted |
| `unknown` | Model/effort requested and observed separately, transport, validation, objective, duration, journal authenticity and current process liveness remain `unknown` in F04a |
| `completeness.journal` | `complete` means the full retained chain through the horizon was checked; it does not claim complete external delivery or authenticated event contents |
| `completeness.external_observations` | `unknown`; no adapter notification delivery guarantee is inferred |
| `completeness.detail` | `redacted`; no private detail is published or semantically interpreted as an external outcome |

The run key follows the current durable approval binding. A retry can change
that binding; it is not a universal task, session or physical-send identity.
Cross-run/task correlation belongs to F06a/#120.

No prompt, raw provider response, secret, command output, private artifact
content, filesystem path or user-controlled identifier is returned. State,
failure and cleanup use existing finite enums. Hashes support local correlation;
they are not an anonymization guarantee or an export authorization.

### Bounds and refusal

Before allocating strings or decoding JSON, SQLite returns numeric row counts
and byte lengths. Each scan stops at its row ceiling plus one sentinel.
`length(CAST(value AS BLOB))` counts bytes, including non-ASCII input.

| Input | Ceiling |
| --- | --- |
| Requested run identifier | 1,024 bytes |
| Global journal | 100,000 rows; 64 MiB total text fields; 65,536 bytes per row |
| Selected-run references | 4,096 rows |
| Selected summary | One row; 65,536 bytes |
| Durable attempts and attempt leases | Five rows each |
| Workflow instances and authority pins | 10,000 rows per table |
| Selected preparation records | 1,024 rows |
| Selected admission intent | One row |
| Each admitted table scan | 64 MiB total; 65,536 bytes per row |

The preflight includes every text field read by the full-chain authority check
and the selected cleanup dependencies. Selected lease records are counted but
never opened as filesystem artifacts. Private artifacts are never opened.

Unsupported schemas, broken hashes, sequence gaps, substituted run bindings,
summary/history mismatch and malformed private JSON produce redacted typed
errors. Limits produce `RuntimeError::Limit`. There is no truncated success
report. A refusal has no snapshot identity and must not be rendered as an empty
or complete observation. Older readable schemas preserve unknown cleanup and
are not migrated.

The byte ceilings bound the Rust allocation and decoding work. SQLite still
performs its own bounded-size query processing and WAL coordination; this is
not an OS sandbox or a guarantee against arbitrary database-layout attacks.

## Consequences

Repeating the same durable snapshot produces byte-identical JSON. A writer's
later committed append changes the global horizon and snapshot digest, even
when it belongs to another run. The selected event list can therefore end
before the global horizon.

Full-chain verification is deliberately conservative: corruption or size limits
in another run refuse the requested observation. This reuses existing chain
integrity rather than inventing independent per-run authority.

Synthetic integration tests exercise cancellation versus cleanup, private
canaries, malformed JSON, oversized records, event ceilings, corrupt chains,
missing stores, older/future schemas and simultaneous WAL writers. These tests
prove their local fixture scope, not a live provider, useful patch, spend cap,
process shutdown or observed savings.

F04b remains responsible for the broader authoritative field inventory,
receipt/model/effort provenance, qualified transport/verification/objective
projections, explicit malformed/lost/truncated external-notification coverage,
published JSON Schema and broader stale/interrupted inspection fixtures. F05b
/#117 owns trustworthy timing; duration remains `unknown` here. F05a can consume
these references without fabricating event semantics from private detail.

## Rejected options

- Concatenating existing independent readers can produce an incoherent report.
- Opening `Runtime` would acquire ownership, recover interrupted runs and
  migrate storage as a side effect of inspection.
- Returning raw summaries/events exposes private paths and untrusted detail.
- A second collector or database creates another source of truth and is outside
  the observation contract.
