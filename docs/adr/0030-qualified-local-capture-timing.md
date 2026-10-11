---
adr: "0030"
title: Qualify local supervisor capture intervals
status: implemented
created: 2026-10-11
last_modified: 2026-10-11
scope: "F05b-a local capture timing marker and opt-in observation v3; broader #117 timing remains open."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0030: Qualify local supervisor capture intervals

## Context

`TransportOutcome.elapsed_ms` already measures a local interval in
`process::capture_guarded`. Legacy attempt evidence does not identify its
producer or clock boundaries. Reinterpreting that field alone would silently
qualify old data. [Issue #117](https://github.com/FlorianBruniaux/agent-router/issues/117)
also covers phase timing and interruption semantics beyond this bounded slice.

## Decision

Reuse the existing start `Instant`, immediately before dispatch/capability
validation. Sample one end `Instant` after process wait/reap and both sequential,
bounded stdout and stderr drains. Both `elapsed_ms` and the new measurement use
these same endpoints. The interval includes dispatch checks, spawn, supervision,
termination signalling/grace, wait/reap and both output drains. It excludes
initial argument/cancellation checks before that start, durable cleanup
confirmation, host-state cleanup, decoding, output persistence and journal writes.
It is neither native-model/provider time nor proof of physical cleanup.

`checked_duration_since` and checked millisecond conversion produce a known
integer, truncated to milliseconds, or explicit `clock_regression` /
`duration_overflow`. A genuine submillisecond interval can be known zero.
Unknown measurement does not fail execution or become a known zero. Existing
transport elapsed saturation behavior is retained. No `Instant`, wall-clock
endpoint or cross-process interval is serialized.

A successful capture propagated to `Store::observed_attempt` appends immutable
`attempt.timing.v1` immediately after `attempt.evidence.v1` in the same SQLite
transaction. Schema 1 contains ordinal, exact evidence event hash, producer
`capture_guarded_v1`, scope `supervisor_capture`, clock
`monotonic_same_invocation`, boundaries `before_dispatch_validation` and
`after_reap_and_both_output_drains`, precision `milliseconds_truncated`, and a
`measurement` tagged `known` with `duration_ms` or `unknown` with `reason`.
The ordinary 2,048-byte event-detail ceiling applies. It adds no database table,
migration, permission or execution authority. `RuntimeEvent`, its hash domain,
`AttemptEvidence` v1 and existing transport/feedback formats remain unchanged.

Opt-in `observe --schema-version 3` retains v2 attempt facts and adds `timings`,
one row per attempt (at most five), joined by ordinal and attempt key. Each row
contains scope/clock/boundary/precision tags, a nullable timing-event hash and the
measurement. Missing markers yield `unknown` / `missing_record`, including legacy
elapsed values and interrupted or unsuccessful capture paths. No start or end is
invented on restart. Completed persisted intervals remain readable after restart.
Native and aggregate duration remain `unknown`; intervals are not summed.

The passive reader uses one transaction and the existing journal preflights.
Every recognized timing record must immediately follow its referenced evidence,
match its outcome state, ordinal, validated v2 evidence hash and, when known,
exact elapsed value. Malformed, duplicate, conflicting, substituted, oversized
or unsupported timing records refuse v3 with redacted errors. Nothing downgrades
a rejected record to missing. The new snapshot hash domain is
`run-observation-v3`. V1 remains the default, and v1/v2 serialization and hash
domains are unchanged for an unchanged journal. New events naturally change the
journal horizon and thus snapshot identities.

## Evidence limits and follow-up

Local shell and synthetic-store tests cover measurement boundaries, successful,
failed and cancelled captures, atomic persistence, strict record joins, legacy
unknowns, restart, and schema behavior. They do not establish provider/native
measurement or production performance. Hash-chain integrity does not authenticate
a same-user writer. Existing v2-only offline correlation stays v2-only.

Preparation, approval waits, human/end-to-end timing, phase/frontier events,
provider/model durations, cross-restart segments and comparative savings remain
outside F05b-a and open under #117 and related measurement work.
