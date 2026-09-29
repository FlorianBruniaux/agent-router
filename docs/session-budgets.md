# Shared session allocations

Session budgets extend the explicitly pinned [admission registry](admission.md).
They reserve parent-declared cost and token allocations across processes and
runtime roots. They do not meter invoices, authenticate provider usage or enforce
an account quota. `observed_cost_microusd` and `observed_tokens` remain `null`
(UNKNOWN); child output and feedback cannot reduce the allocation counters.

## Initialize an explicit domain

First create a fresh admission registry and its runtime directories. Before any
runtime owner opens it, provide a budget policy whose runtime keys exactly match
the admission policy. Session names are parent mappings, not host aliases.

```json
{
  "schema_version": 1,
  "sessions": {
    "project-session": {
      "initial_limit": {"cost_microusd": 1000000, "tokens": 50000},
      "parent_ceiling": {"cost_microusd": 2000000, "tokens": 100000},
      "warning_basis_points": [5000, 9000]
    }
  },
  "runtime_sessions": {"project-worker": "project-session"}
}
```

```sh
agent-router admission budget policy-hash --policy budget-policy.json --json
agent-router admission budget init --registry /absolute/registry \
  --expected-policy-sha256 ADMISSION_POLICY_HASH --policy budget-policy.json --json
```

Initialization acquires every configured runtime owner lock and refuses any
ticket history, existing budget policy or live owner. It upgrades this empty
admission database from schema 1 to schema 2 atomically. It does not backfill old
runs with zero consumption. Older binaries reject schema 2. A legacy registry
without this extension still accepts only legacy admission definitions.

Add `session_budget` inside the definition or preparation manifest's `admission`
object, preserving its registry, admission policy hash and runtime ID:

```json
{
  "session_budget": {
    "schema_version": 1,
    "expected_policy_sha256": "BUDGET_POLICY_HASH",
    "session_id": "project-session",
    "approved_limit": {"cost_microusd": 1000000, "tokens": 50000},
    "attempt_allocation": {"cost_microusd": 100000, "tokens": 5000}
  }
}
```

The complete configuration is bound to the execution review. The allocation must
fit the node and routing restrictions and cover the selected candidate estimates.
These estimates are admission inputs, not measured usage. Missing session
configuration, unknown mappings, changed pins or reopening a bound runtime as
legacy are refused. Preparation preserves the configuration; coordinator waves
and control batches require identical admission configurations, including these
budget fields. Every model attempt, retry and escalated target receives its own
allocation. Local version probes and Git finalization hold concurrency capacity
but do not introduce a second model allocation.

## Reservation, dispatch and recovery

One SQLite immediate transaction grants all concurrency dimensions and reserves
both budget dimensions. A request exceeding either dimension gets a stable
`session_budget.cost.exhausted` or `session_budget.tokens.exhausted` refusal;
it does not partially reserve the other dimension. An ineligible queued request
cannot prevent a different eligible workflow from advancing.

The effective limit is the minimum, independently for each dimension, of the
current session limit and the limit bound to the approved definition. Raising
the current limit cannot enlarge an old definition's authority. To benefit from
a larger approved limit, prepare a new definition/run and obtain a new execution
approval. Historical definitions cannot be overwritten in place.

Immediately before spawn, the runtime durably marks the exact intent as armed,
then moves the full reservation to committed allocation. A shared budget guard
protects the final current-limit check through `spawn`; limit changes require an
exclusive guard. Authority and assets are revalidated after queueing as before.
If the registry refuses that final commitment (for example after a limit
reduction), the run never spawns: the runtime clears the armed marker, so the
release refunds the reservation. A crash between the two commits still leaves
the marker set and is charged conservatively, as the table below states.

| Interrupted point | Recovery |
| --- | --- |
| Intent recorded, no registry grant | No allocation; release still requires exact cleanup evidence. |
| Granted, not armed, or armed then refused by the registry | Reserved until internal cleanup proof establishes no dispatch; then refunded. |
| Armed, central commitment not recorded | Treat the dispatch window as ambiguous; commit the full allocation during proven cleanup. |
| Committed, including spawn failure or supervisor death | Keep the full allocation permanently committed. |
| Cleanup UNKNOWN | Retain capacity and the existing reserved or committed allocation. |
| Registry release committed, runtime release journal missing | Reconcile idempotently without a second charge or refund. |

The runtime and admission database do not form one distributed transaction.
The durable armed marker closes the unsafe refund window conservatively. It can
charge an attempt that never spawned. No postdispatch refund, force release,
reset or silent purge is provided. A success label, cancellation, zero usage in
stdout, provider error or retry approval does not restore budget.

## Inspect and change a limit

```sh
agent-router admission budget status --registry /absolute/registry \
  --expected-policy-sha256 ADMISSION_POLICY_HASH \
  --expected-budget-policy-sha256 BUDGET_POLICY_HASH \
  --session project-session --json
```

Status distinguishes reserved and committed allocations and reads its journal
and projections in one SQLite snapshot. Warning thresholds use basis points
against their sum. Each threshold is recorded once per dimension and limit
revision, surviving restart; they are status data, not external notifications.

Write the proposed `BudgetAmount` to `new-limit.json`, then review it:

```sh
agent-router admission budget limit --registry /absolute/registry \
  --expected-policy-sha256 ADMISSION_POLICY_HASH \
  --expected-budget-policy-sha256 BUDGET_POLICY_HASH \
  --session project-session --new-limit new-limit.json --json
```

The preview includes the exact current counters, registry identity, ledger head,
limit revision, requested amount and `approval_digest`. Apply the same command
with `--approve APPROVAL_DIGEST --expected-head LEDGER_HEAD`. A concurrent
reservation, commitment, release or limit change invalidates that review.
The parent ceiling is immutable. A reduction can fall below already encumbered
allocation; it blocks further admission/dispatch without refunding it. A change
never launches a process or rewrites an execution approval.

## Bounds and evidence

Policies allow at most 128 sessions and runtime mappings, eight sorted unique
thresholds per session, and integer amounts at most `i64::MAX`. Inputs are strict
bounded JSON: policy 128 KiB, new limit 2 KiB. The registry holds at most 4096
live tickets, 16384 budget events and 1024 limit revisions per session. Hitting a
bound refuses work without silently deleting evidence. Released tickets beyond the
newest 512 are pruned with their settled reservations
(docs/adr/0017-bounded-retention-and-publish-durability.md): a committed
reservation is first folded into a per-session `budget_folded` total, so counters
stay verifiable. That table is registry schema version 3: opening a version 2
registry migrates it, and a future version is refused. The budget event journal
is hash-chained evidence and is not pruned, so its 16384 lifetime cap remains;
anchored compaction is deferred (ADR 0017). Hashes detect corruption and
bind projections; a hostile same-UID replacement of all history still requires
an external anti-rollback anchor to detect.

Database, WAL and SHM checks use descriptor-relative no-follow metadata reads.
Opening and closing a second descriptor for a live SQLite inode can drop the
process's POSIX locks, so validation must not do so. The multiprocess regression
keeps a writer open while independent observers come and go and verifies its
committed allocation remains visible.

Behavior tests cover concurrent vector reservation, stale reviews, restart,
armed recovery, private paths, old approval caps, warnings and tampering. Native
fake-host canaries cover competing runtime roots, SIGKILL cleanup, and a workflow
with real artifact handoff, two failed attempts and an approved escalated target.
They establish local supervision and allocation behavior, not provider billing
or model quality. Linux execution is a separate CI gate.
