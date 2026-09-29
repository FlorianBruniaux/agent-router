# 0018: Incremental event chain verification by the store owner

Status: Accepted

Date: 2026-09-29

## Context

The runtime `events` table is an append-only SHA-256 chain. It is never pruned:
the retention decision of [ADR 0017](0017-bounded-retention-and-publish-durability.md)
does not cover it. Before this decision, every chain check replayed the whole
chain from sequence 1, rehashing each row: the owner's `verify_plan_pin`, called
inside the `IMMEDIATE` transaction of each workflow registration and before a
generated dispatch, and the escalation history read by `escalation_history`
and `escalation_event`. The cost of one operation therefore grew with the lifetime
history, while holding the SQLite write lock that other connections wait for
with a 2 second `busy_timeout`.

Measured on a release build with the ignored test
`store::tests::measure_owner_chain_checks_against_history` (one macOS
workstation, one run each, 100 calls after the given history):

| History | `verify_plan_pin` before | after | `register` + `verify_plan_pin` before | after |
| --- | --- | --- | --- | --- |
| 1,000 events | 102 ms | 2.3 ms | 121 ms | 23 ms |
| 10,000 events | 1.11 s | 3.3 ms | 2.12 s | 57 ms |
| 100,000 events | 11.5 s | 2.4 ms | 10.4 s | 15 ms |

`register` alone does not check the chain; the pair runs one commit and one
check per iteration, so its after column is dominated by the commit.

## Decision

The `Store` that holds the exclusive supervision flock keeps a trust point in
memory: the sequence and hash of the last event it verified, and the plan
authority anchors recorded up to it.

- Opening the store and the public `verify_events` (`Runtime::verify_integrity`)
  still replay the whole chain, in one read snapshot. A replay can only extend
  the trust point: the row at the remembered sequence must still carry the
  remembered hash, and every remembered anchor must be replayed with equal
  values. Otherwise the replay fails with an integrity error and the trust point
  is kept. On success the trust point moves to the last row that replay saw.
- Every other owner check verifies only the tail after the trust point. It
  rereads and rehashes the trust point row and compares it with the remembered
  hash, and it requires the chain to start at sequence 1.
- Every plan authority anchor found in the tail must match its instance row
  (contract and pin), as in the full replay.
- The owner's plan pin check, and the escalation history for its source run's
  instance, compare that one instance with its event anchor, taken from the
  tail or from the remembered anchors. The full replay compares every instance.
- The trust point advances after a successful check, never after an append. A
  check that reads the transaction's own uncommitted events advances the point
  only after commit, so a rollback cannot leave a point on a missing row. The
  escalation paths open their transaction through a helper that verifies the
  tail before returning it, so no write of that transaction can precede the
  check.

Connections without the flock have no trust point and keep the full replay:
`observe`, `observe_cleanup`, `observe_events`, `request_cancel` and the passive
diagnostics.

## What an owner check detects

- Any change, insertion, gap or reordering among the rows after the trust point.
- A rewrite of the trust point row, including one whose stored hash was
  recomputed to stay self-consistent. The previous full replay accepted such a
  rewrite of the last row.
- Truncation that removes the trust point row, including deletion of the whole
  table. The previous full replay accepted an empty table.
- A row with a sequence below 1.
- A duplicate plan authority anchor in the tail or against a remembered anchor,
  and a tail anchor without a matching instance row.
- A pin or contract change for the instance being authorized, or for the
  instance of the source run of an escalation history or claim.
- On `verify_integrity` during the owner's lifetime, a whole chain rewritten
  with recomputed hashes, or truncated below the trust point: the replay is
  consistent but no longer extends the trust point. After a reopen there is no
  trust point, and such a rewrite stays undetectable as before this decision.

## What it no longer detects before the next open or `verify_integrity`

- A rewrite, deletion or reordering of rows strictly before the trust point
  that keeps the trust point row and a row at sequence 1. The trust point row
  itself is rehashed, and its hash covers its previous hash.
- A changed pin or contract row for a workflow instance other than the one the
  check authorizes. Using that instance fails its own check.

Neither check detects truncation of events appended after the last check,
before or after this decision: the chain has no external anchor.

## Why this is acceptable

The runtime directory is private to one user. The hash chain detects changes;
it does not authenticate a writer who controls the private storage
([runtime storage](../runtime.md)). [ADR 0008](0008-shared-admission.md) already
places an arbitrary same-UID rewrite or rollback of all authoritative stores
outside this local integrity model. The undetected cases need such a process to
rewrite an old row during the owner's lifetime; that same process could rewrite
the whole database and its hashes. The full replay at every open and on
`verify_integrity` still catches the old-row rewrites that leave the chain
inconsistent, and during the owner's lifetime also those that recompute every
later hash.

## Consequences

- The owner's chain work is proportional to events appended since its last
  check, so the whole lifetime of a store costs one pass per event instead of
  one pass per event per check.
- `request_cancel` still replays the full chain inside its `IMMEDIATE`
  transaction and so holds the write lock for time proportional to history.
- The escalation history still materializes every event before filtering by run
  (`history` in `store/escalation.rs`). It is not a chain check, and its cost
  still grows with history: 4.9 s and 7.0 s for 100 calls at 100,000 events in
  two runs of the measurement above, against 4.9 s for 100 calls of
  `Store::events` in the same run as the first figure. An index or a
  filtered query is a separate change.
