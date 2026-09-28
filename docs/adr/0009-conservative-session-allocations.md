# ADR 0009: Conservative shared session allocations

Status: accepted for implementation.

## Context

Shared admission already serializes concurrency in a private, explicitly pinned
SQLite registry. Per-node estimates and escalation bounds do not impose a
persistent session allocation across independent runtime roots. Provider usage
is not authenticated by a child response or caller estimate.

## Decision

Add an optional, separately pinned parent budget policy to the same registry.
Map every configured runtime to a parent-defined session. Initialize only an
empty domain while all runtime owner locks are held; never invent historical
zero consumption. Keep parent ceilings immutable and bind the full attempt
allocation and approved limit into each execution definition.

Reserve cost and tokens atomically with concurrency admission. Apply
`min(current_limit, definition.approved_limit)` independently to each dimension,
both at reservation and before dispatch. Limit mutations require an opaque review
of the exact current state, a matching approval digest, and transactional CAS.
A larger current limit cannot benefit an older, lower approved definition.

Journal an armed marker in the runtime before committing the complete allocation
in the registry, then spawn while holding a shared budget guard. Limit changes
require the corresponding exclusive guard. Cleanup may refund an unarmed
reservation only through the existing internal opaque cleanup proof. Armed
ambiguity becomes committed allocation. Committed amounts never decrease.

The two databases cannot commit atomically. Prefer over-accounting a possible
spawn to refunding work that might have occurred. UNKNOWN cleanup keeps capacity
and accounting encumbered. Recovery is idempotent at both journal boundaries.
Status exposes reserved and committed amounts separately from observed usage,
which remains UNKNOWN. Warnings persist once per threshold, dimension and limit
revision. Exhausted bounded history refuses further mutation, without a reset.

Use no-follow `statat` checks for existing SQLite DB/WAL/SHM inodes. Do not open
and close auxiliary descriptors while SQLite connections exist: POSIX record
locks belong to a process and can be released by closing another descriptor to
the same inode. Continue checking private mode, regular-file type, single link
and pinned database identity without interfering with SQLite's descriptor life.

## Consequences

Old definitions and registries retain their explicit legacy behavior, but cannot
silently join or escape a budget-bound domain. Coordinator attempts and R05
escalations use the same allocation protocol; a wave requires identical budget
configuration. A new approval is required to benefit from a larger definition
cap. Preparation and Git finalization retain their existing concurrency cleanup
contract and do not represent extra model charges.

This bounds parent allocations rather than actual spending or tokens consumed.
It does not authenticate invoices, add provider quota authority, refund failed
calls, or protect against a hostile same-UID rollback of all local history.
Separate provider accounting and external anchors would need separate evidence.

## Validation

Contract tests include two independent processes competing for a shared vector,
exact-state review invalidation, old-cap monotonicity, dispatch lock exclusion,
restart and armed recovery, schema initialization refusal, and journal tampering.
A RED/GREEN observer regression demonstrates the POSIX lock failure and its
metadata-only correction. Native fake-host tests cover concurrency, supervisor
SIGKILL and a coordinator escalation after real failed attempts and artifact
handoff. See [session budget operations](../session-budgets.md).
