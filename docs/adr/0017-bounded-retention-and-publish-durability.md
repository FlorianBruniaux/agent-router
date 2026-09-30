---
adr: "0017"
title: Bounded retention and publish durability for stores
status: accepted
created: 2026-09-29
last_modified: 2026-09-29
scope: "Persistent stores: lifetime caps and publish durability."
amended_by: []
supersedes: []
superseded_by: []
related: ["0008"]
---

# ADR 0017: Bounded retention and publish durability for stores

## Context

A global review of `main` (ac24365) reported two defects in the persistent
stores. The review report is not reread in this branch. Only the catalog store
was re-checked against its code here.

- Lifetime caps dead-end. Catalog store: after 128 events, `review` and `apply`
  refused every change, including a revocation, so the cap removed the ability
  to withdraw authority.
- Publish is write, file fsync, atomic rename, directory fsync. Catalog store: a
  failure of the last step was returned as a plain `Io` although the new state
  was already visible.

The other stores named in the review (runtime admission registry and budget
reservations, backend store promotions, advisory cache, coordinator materials)
were not reread in this branch.

## Decision

Decided A. Caps bound live rows, not lifetime changes.

- Terminal and superseded history beyond a bounded retention is pruned, so the
  cap applies to live rows.
- Capacity for revocation is always reserved: an active authority can always be
  revoked whatever the history length. Other changes may be refused first.
- Where history is hash chained, pruning keeps the chain verifiable with an
  anchor, the head of the last pruned entry, that retained entries chain from.
- Review or admission and apply run the same plan, so an approval is not granted
  for a change that cannot apply.

Decided B. Published but not durable is its own outcome.

- A store that renamed its new state into place and then failed the final
  directory fsync returns a dedicated typed error meaning "published,
  durability UNKNOWN". It is distinct from `Io` and from a pre-publication
  failure, and has a stable code.
- Callers must not retry blindly. They read the current state and decide from
  what they observe.

## Consequences

- Pruned history cannot be reconstructed, and a trusted checkpoint older than the
  retained window is refused. Operators advance the external checkpoint within
  each retention window.
- Each store that adopts this decision must document its retention bound and
  reserved revocation slot, and must keep pre-publication errors distinct from
  the durability-unknown error. Adoption by the other stores is decided here and
  implemented on other branches, not verified in this one.

## Catalog store (implemented and tested in this branch)

- 128 retained events, 16 retained documents, 1 MiB reserved for revocation.
- Snapshot `anchor` field, absent until history is first pruned.
- Error code `published_durability_unknown`.
- Replay protection does not depend on history. Approvals bind the exact current
  head hash and revision, and a head hash commits to its predecessor and
  revision, so a pruned head never becomes current again.

## Other stores (decided, not reread in this branch)

- Backend store: decided not to prune. Its state is rebuilt by replaying every
  event from genesis, and that replay is understood to enforce nonce uniqueness
  and version immutability (`router-backend-policy/src/activation.rs:184-219`,
  not reread in this branch). Decided: raise its promotion cap to `MAX_EVENTS/2`;
  its lifetime cap of 128 events remains until a signed checkpoint compaction is
  designed (deferred).
- Advisory cache: decided to sweep expired entries (other branch).
- Runtime registry: decided to prune released tickets (other branch).

## Runtime admission registry (implemented and tested in the runtime-store branch)

- Released tickets beyond the 512 most recent are pruned inside the
  transaction that enqueues a new ticket, so `MAX_TICKETS` bounds live
  (waiting or granted) tickets instead of every ticket ever enqueued.
- A ticket whose reservation is still `reserved` is never pruned. A
  `committed` reservation is folded into a per-session total
  (`budget_folded`) before deletion, and loading adds that total back, so
  session accounting is unchanged.
- The identifiers of the 4096 most recently pruned tickets are kept
  (`budget_pruned`). Replaying an armed release for one of them is a no-op;
  any other unknown ticket, including one older than that window, still
  fails with `BudgetError::Unknown`.
- These tables arrive with registry schema version 3 through an explicit
  migration, re-checked inside the migration transaction so concurrent opens
  of a version 2 registry both succeed; a future version is refused.
- The budget event journal keeps its lifetime cap of 16384 events: it is
  hash-chained, and compacting it needs an anchored checkpoint (deferred).
