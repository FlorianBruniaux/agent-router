# Shared concurrency admission

Shared admission is opt-in. A parent selects one private registry and pins its
policy hash. Only runtimes configured for that registry share its global limit.
Separate registries and legacy runtimes do not form a machine-wide domain.

## Configuration

Create each runtime directory with mode `0700`. Obtain its canonical root hash
with `agent-router admission root-hash --root /absolute/runtime --json`. A parent
policy contains explicit capacities and identity mappings:

```json
{
  "schema_version": 1,
  "global_limit": 4,
  "runtimes": {
    "project-worker": {"root_sha256": "<root hash>", "limit": 2}
  },
  "providers": {"<validated asset provider>": 2},
  "agents": {"project-agent": 1},
  "agent_mappings": [
    {"runtime_id": "project-worker", "plan_owner": "<approved node owner>",
     "candidate_agent": null, "quota_id": "project-agent"}
  ]
}
```

`candidate_agent: null` maps a selected candidate that explicitly has no agent.
It is not a wildcard. Provider identity comes from validated v3 asset
requirements. Harness names and child output cannot create quota identities.
Unknown runtime, provider or logical-agent mappings are refused.

Use `admission policy-hash --policy POLICY --json` to validate the policy, then
`admission init --registry /absolute/new-registry --policy POLICY --json` to
create the registry. No hook, provider request or execution occurs. Add this
field to the execution definition or preparation manifest:

```json
{
  "admission": {
    "schema_version": 1,
    "registry": "/absolute/registry",
    "expected_policy_sha256": "<externally approved policy hash>",
    "runtime_id": "project-worker"
  }
}
```

Preparation preserves this field in its exported definition. Review binds the
registry location and file identity, policy, runtime identity and local capacity
limits into the approval. Coordinator steps use the same field; a wave or
control-service batch containing different admission configurations is refused.
An existing bound runtime cannot be reopened as legacy or against a cloned
registry. Schema 8 databases migrate to schema 9, but existing legacy runs must
remain legacy; enabling shared admission requires a new runtime store and review.

`admission status --registry PATH --expected-policy-sha256 HASH --json` reports
aggregate waiting, granted and released tickets. These counts do not assert
model success, provider-side quota compliance or objective verification.

## Scheduling and release

A single SQLite immediate transaction reserves global, runtime, provider,
logical-agent and workflow capacities together. No partial permits are held.
Workflow groups receive round-robin turns with FIFO ordering inside each group;
a blocked head does not reserve capacity or block an eligible different group.
A poll first reads the queue in a deferred transaction and takes the write lock
only when it must grant its ticket or release a dead waiter; the decision is then
made again inside the immediate transaction. The pinned policy and budget policy
rows are compared by SHA-256 digest of the stored JSON, computed once at open. A
waiting run polls every 20 ms, doubling up to 100 ms while its queue reasons are
unchanged and returning to 20 ms when they change. Trade-off: a waiter whose
reasons stay constant is polled every 100 ms, so a hand-off between waiters and
the observation of a cancellation can each be delayed by at most 100 ms (plus the
cost of one poll), against 20 ms before.
Restrictions of active tickets cannot be enlarged by a later, less restrictive
ticket. Queue events list the actual binding capacity dimensions or `fair_turn`.

The runtime journals its exact admission intent before enqueue. A ticket binds
run, definition, approval, workflow instance/contract/generation, an admission
sequence, a preparation cycle and an expected model attempt. Waiting owns a
per-ticket lock, so dropping its future cannot indefinitely block other groups.
Losing that lock can remove only an ungranted waiter. A granted reservation is
never reclaimed from elapsed time, PID death, a terminal label or a public DTO.

Release requires an internal opaque proof for the exact intent: completed
preparation/probe evidence, the actual model lease if an attempt started, and no
open Git finalization window. Cleanup locks survive the registry release and
runtime journal commits. Restart reconciles these two stores idempotently;
UNKNOWN retains capacity. Authority, source state, v3 assets and current decision
gates are checked again after queueing before a process can start.

## Bounds and limitations

Registry and runtime files are private (`0700` directories, `0600` files), with
no-follow opens and pinned directory/database identities. SQLite uses WAL,
FULL synchronous commits and a 250 ms busy bound. A queued run retries a busy
registry inside its wait, for at most 10 seconds of consecutive contention,
before failing with `runtime.limit`. Capacities are 1–32, mappings
are bounded to 128 entries per category, and a registry holds at most 4096 live
(waiting or granted) tickets. Released tickets beyond the newest 512 are pruned
inside the enqueue transaction, so the cap bounds live rows, not lifetime rows
(docs/adr/0017-bounded-retention-and-publish-durability.md). Exhaustion of live
capacity refuses new work; no capacity reset is provided. Duplicate runtime
owners are refused.

The runtime writer lock compares workspaces and scopes lexically (slashes and `.`
components); `..` is refused, and symlinked or nested workspaces are not detected.
Upgrade effect: a `writer_locks` row persisted by an older build with `..` in its
workspace makes every new writer lock fail (CLI exit 7) with `runtime.integrity: writer lock
workspace` until that row is released.

The registry schema is version 3 (per-session `budget_folded` totals and a bounded
`budget_pruned` tombstone list of 4096 ids). Opening a version 2 registry migrates
it under the write lock, so concurrent opens are safe; a future version is refused.
A replayed armed release of a pruned ticket succeeds only while its tombstone is
retained; otherwise it fails closed as `session_budget.accounting.unknown`.

This is a local concurrency contract. Optional [session allocations](session-budgets.md)
add persistent parent-declared cost/token reservations in the same registry.
Neither feature authenticates provider accounting, discovers provider quota
allowances or coordinates independent machines. A malicious process with the same UID can
rewrite the entire registry and runtime history; no external anti-rollback
anchor is claimed. Open/untracked Git windows can require manual investigation
and remain quarantined, without an override that fabricates cleanup.
