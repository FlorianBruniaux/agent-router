# Escalation from durable attempts

The runtime can propose a different model or effort after at least two distinct
completed worker attempts. The caller supplies an ordered, bounded
`EscalationPolicy` containing explicit candidate-to-candidate transitions and
permitted failure classes. The runtime does not infer that a model is stronger,
alter the deterministic receipt, or manufacture a provider outage.

Policies allow 2 to 5 required failures, at most 4 permitted classes and 32
ordered transitions. An escalation history is bounded to 5 attempts, 6 run
generations and 2,048 per-run-family events. New audit-event details are bounded
to 16 KiB. Full-store hash-chain validation retains the existing store behavior.

`Runtime::review_escalation` returns an opaque `EscalationProof`.
`proof.decision(router, context)` creates a distinct effective execution decision.
Its deterministic baseline is unchanged. The effective selection, original
context and pool, attempt identities, policy, source execution, accounting basis
and remaining budget are bound to a separate decision digest. Ordinary review
and approval are still required for the new run. The existing snapshot,
permissions, assets, source, sandbox and dispatch gates remain in force.

## Evidence and accounting

Each captured completion now writes `attempt.evidence.v1` in the same SQLite
transaction as the attempt finish and transport transition. It records the
run/attempt identity, original definition and authorization, requested model/effort,
context/pool, source and task execution hashes, classified failure, measured
worker elapsed time and cleanup evidence. References to the corresponding
started, finished and transition events join this record to the append-only
hash chain. Prompt, stdout and stderr bytes are not recorded. A requested model remains
separate from a model actually observed by the host; this journal does not
upgrade unknown host evidence.

This is an additive event schema. There is no database migration or backfill.
An old finished attempt without this record cannot support escalation. A crash
before this transaction also leaves no eligible completion proof. Two error
messages from one process remain one attempt. Authentication, cancellation,
policy, output-limit, verification, spawn and interrupted failures never count.
The configurable subset is limited to rate limit, network, timeout and crash.
Cleanup must be `group_terminated` or `namespace_terminated`; the scope of those
proofs remains as documented in `runtime.md`.

Every attempt also records its original sandbox and effective decision origin.
All source attempts must be `read_only` and come from a deterministic or activated
decision. Discarding a failed writing worktree never changes that recorded
authority. Other decision origins are rejected during journal reconstruction,
including when a caller constructs an alternative trusted-boundary object.

Observed cost and tokens are currently `null` (UNKNOWN). Candidate estimates do
not become actual spending or a guaranteed cost ceiling. UNKNOWN blocks the
remaining-budget calculation. The parent may explicitly attest an upper bound
for a completed attempt using:

1. `review_attempt_bound(pending, attempt_identity, upper_bound)`;
2. inspect its `AttemptBoundPreview` and approval digest;
3. `approve_attempt_bound(reviewed, digest)`.

This writes a separate `attempt.bound.v1` event. It does not modify the attempt
facts or call the attestation an observation. The preview is opaque, tied to the
current ledger and canonical runtime storage path, and single-use; concurrent history changes invalidate it.
Each attempt can receive at most one bound. The correctness of a parent-provided
upper bound is a trusted parent assertion, not something the runtime can measure.

Every recorded attempt in the same run, or in earlier generations of the same
workflow instance and node, consumes the budget. Missing accounting, arithmetic
overflow or an exhausted budget blocks the proposal. At least the configured
number of most recent attempts must have the same prior candidate/model/effort
and an allowed terminal failure. Candidate estimates are admission checks against
the remaining bound; they do not enforce the provider's eventual charge.

## Approval, restart and reservation

The proposed target must already be a core-validated selection in the baseline
or its exposed alternatives. The current API conservatively refuses candidates
outside that bounded set, even if another unexposed eligible candidate exists.
Workspace, source, scope, prompt, mode and execution target must remain the same.
Task data cannot acquire broader authority through an escalation.

New runtime definitions use the `runtime-definition-v6-escalation` domain.
Definitions without escalation retain their existing domains. Review reserves
the source evidence transactionally for one exact target run and definition in
`escalation.claim.v1`. A copied proof cannot spend the same remaining budget on a
second run. The source cannot retry or receive new accounting attestations after
reservation. A failed target registration is not execution authorization.

Copying the storage to another path changes its ledger digest and requires a new
review and approval. At dispatch the runtime rebuilds the proof from its own current journal and
checks the reservation again. JSON audit bindings and implementations of the
trusted `EscalationAuthority` integration trait cannot bypass this independent
runtime check. The runtime's process ownership lock and transactional reservation
protect the source against competing supervisors; source retry and accounting
mutations are rejected once claimed.

An escalation permits one target attempt. Retrying that target through the
ordinary retry API is refused; further escalation generations are not implicit.
The cumulative runtime attempt limit still applies. Its default of two must be
explicitly raised by the parent to permit a third attempt. Reopening an existing
target for historical inspection does not treat that inspection as permission
to execute again. No running, verified, successful or writing source run is
eligible for escalation.

Tests use local synthetic workers. Native canaries exercise the real OS sandbox
and resource helper, but do not establish provider behavior, model quality,
objective completion, native pricing or token accounting.
