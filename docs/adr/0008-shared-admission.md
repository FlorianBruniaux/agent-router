# ADR 0008: Explicit shared admission with opaque release

Status: implemented. Amended by [ADR 0009](0009-conservative-session-allocations.md)
(session cost and token allocations) and
[ADR 0017](0017-bounded-retention-and-publish-durability.md) (pruning of settled
tickets), which supersede the "no implicit pruning" and "no cost/token
accounting" sentences below.

Local semaphores cannot coordinate independent runtime owners and can release
capacity when an interrupted future still has unproven descendants. R12 adds an
explicit parent-pinned SQLite registry. The existing runtime retains ownership
of execution authority and cleanup evidence.

The admission registry atomically reserves the complete capacity vector. Parent
policy fixes runtime roots and identity mappings. Logical labels alone grant no
quota. Fairness is persistent round robin across workflow groups and FIFO within
a group. A per-ticket waiting lock handles abandoned futures without converting
process death into cleanup authority for granted work.

SQLite cannot atomically commit two independent stores. A durable runtime intent
precedes registry enqueue and grant; no child starts before both. Release uses a
non-serializable internal proof, commits to the registry first and then closes
the runtime intent while retaining model cleanup locks. Reopening replays only
this reconciliation, never the model operation. Independent admission and
preparation sequences prevent a crash before preparation from reusing a released
ticket. Historical released intents retain their original approval identity.

Runtime schema 9 stores the immutable registry binding and admission intents.
Existing runs receive no shared authority during migration. Finalization Git
operations use the existing capture observer protocol and cannot inherit the
model's cleanup proof. UNKNOWN holds reservations and writer protection.

The registry is bounded and has no implicit pruning, reset, global discovery,
network transport or cost/token accounting. An arbitrary same-UID rewrite or
rollback of all authoritative stores remains outside this local integrity model.
See [shared admission](../admission.md) for configuration, limits and CLI paths.
