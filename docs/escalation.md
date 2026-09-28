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

## CLI: standalone run definitions

The CLI supports accounting review and approval, an inert proposal file, selected
asset preparation, run review and a separately approved dispatch. All inputs are
strict bounded JSON. The source is an existing standalone run definition and its
private runtime store. A new proposal requires the source's v3 asset authority.
This does not retrofit v3 authority or attempt evidence onto an old run.

First inspect a bound for one completed attempt. The numeric ceilings below are
operator inputs, not values estimated or measured by this command:

```sh
agent-router escalation bound --store runtime --source-definition source/definition.json \
  --attempt-run-id source --attempt 1 \
  --max-cost-microusd "$COST_BOUND" --max-tokens "$TOKEN_BOUND" --json
```

Repeat that exact command with `--approve DIGEST` using the displayed
`preview.approval_digest`. Repeat the review/approval pair for attempt 2. Review
and approve one bound at a time: approving another bound changes the ledger and
invalidates previews created against its previous state. An approved ceiling is
reported as `parent_attested_upper_bound`; observed cost and tokens remain null.
A bound cannot be approved twice or replaced with a smaller value.

A policy explicitly lists allowed transitions and failures:

```json
{
  "schema_version": 1,
  "minimum_failed_attempts": 2,
  "retryable_failures": ["crash", "timeout"],
  "transitions": [
    {"from_candidate_id": "primary", "to_candidate_id": "secondary"}
  ]
}
```

```sh
agent-router escalation propose --store runtime --source-definition source/definition.json \
  --policy escalation-policy.json --max-attempts 3 --output proposal.json --json
```

`--max-attempts` is required, accepts 3 through 5 and is a total attempt ceiling
for this escalation family. It does not increase the ordinary run default of two
attempts. Proposal output contains `document` and `preview`, with `executed:false`
and `approved:false`. `--output` writes only the document, using a private new
file and refusing overwrites. Proposing does not reserve a target or launch a
worker. UNKNOWN accounting or cleanup, exhausted budgets, inadmissible targets
and unsupported source origins remain refusals.

Add `"escalation_decision":"proposal.json"` to the existing preparation manifest,
use a new `run_id`, and retain the original task, prompt variables, workspace,
source and scope. The field is exclusive with `backend_decision` and
`generated_plan`. `prepare` uses the existing selected-asset pipeline and checks
the actual prepared request against the source evidence before writing a bundle:

```sh
agent-router prepare --manifest prepare-target.json --output-directory target --json
agent-router run --definition target/definition.json --store runtime --sandbox sandbox.json --json
agent-router run --definition target/definition.json --store runtime --sandbox sandbox.json \
  --approve FINAL_RUN_DIGEST --json
```

The final digest is the new run review's `approval_digest`, not an accounting
approval or a proposal hash. The source and target must use the same output
retention configuration for historical source reconstruction. Snapshot/asset
freshness, the approved binaries and the OS sandbox are still checked through
ordinary execution. A changed template or agent instruction that changes the
prompt requires a new task instead of escalation. Each run review exposes
`decision_origin`, `escalation_decision` and the unchanged `baseline_receipt_id`.
The baseline receipt written to the bundle is never replaced with a fabricated
selection or unavailability report.

Proposal schema 1 pins the canonical runtime store, source definition, source
material hash, policy, attempt ceiling, ledger hash and decision identifier. The
source material includes the reopened immutable runtime definition and any
historical backend authority locator. The CLI reads the proposal once, rebuilds
its opaque proof from the currently owned journal, and binds its bytes and
canonical paths into the final CLI approval. Copying a store, substituting a
source locator, changing the ledger or changing the proposal invalidates an old
approval or causes reconstruction to fail. No serialized `EscalationProof` is
accepted and source manifests are not followed recursively. Generated and
already escalated source decisions are rejected before their decision files are
read. A historically activated source can contribute its completed attempts;
this never reactivates a revoked semantic backend for dispatch.

After restart, verification/artifact commands reopen the same source and proposal
against the same store. Review reserves one exact target through the runtime's
existing transactional claim; neither a second target nor `run --retry` can
spend the escalation again. A changed or missing input does not fall back to the
baseline silently.

## CLI: coordinator source bundles

`--source-bundle` selects a registered coordinator step instead of a standalone
run definition. It is exclusive with `--source-definition`. Here `--store` is the
coordinate store containing both `runtime/` and `coordinator/`:

```sh
agent-router escalation bound --store coordinate-store --source-bundle failed-1/bundle.json \
  --attempt-run-id "$FIRST_RUN_ID" --attempt 1 \
  --max-cost-microusd "$COST_BOUND" --max-tokens "$TOKEN_BOUND" --json
```

Approve the displayed bound digest, then review and approve the other completed
attempt. Different generations have different `wf-...` run IDs; each actual
process attempt counts once. Use the latest failed source bundle to include the
same node's earlier generations. A verified predecessor is a dependency, not a
failed attempt of this node.

```sh
agent-router escalation propose --store coordinate-store --source-bundle failed-1/bundle.json \
  --policy escalation-policy.json --max-attempts 3 --output proposal.json --json
```

Proposal schema 2 binds the source bundle, its resolved locators, workflow,
registered run/generation, final request and artifact contract. Reopening checks
the coordinator's step row, immutable definition hash and runtime storage root.
It uses the original final request and recorded grants, not the latest dependency
text. Generated and escalated sources are rejected before their decision files
are followed. Proposal schema 1 remains the standalone format.

The target preparation manifest names `escalation_decision`, the same sealed
`workflow_contract`, and `generation` equal to the source generation plus one.
Keep the same base prompt template and variables. Preparation reconstructs a copy
with the recorded dependency section, validates the v3 input budget on these
final bytes, and exports only the base request. The runtime still validates the
actual adapter stdin at dispatch. Missing or purged input artifacts refuse
preparation; the CLI does not reconstruct missing content from a model answer.

```sh
agent-router prepare --manifest prepare-target.json --output-directory target --json
agent-router coordinate review --definition workflow.json --source-definition target/definition.json \
  --node-id execute --generation 2 --store coordinate-store --sandbox sandbox.json \
  --output-directory target-step --json
```

The generation `2` example follows source generation `1`; it is not a default.
Coordinate review computes the final dependency inputs again and requires their
exact artifact-contract hash to match the source. A different grant is rejected
even if it would render the same text. The runtime also compares the source and
target contract hashes before reserving a claim. New workflow, node, source,
permissions or prompt authority cannot be introduced by changing the proposal.

Create the existing wave manifest with `target-step/bundle.json` and this review's
new `approval_digest`, then explicitly execute it:

```sh
agent-router coordinate run-wave --definition workflow.json --wave target-wave.json \
  --store coordinate-store --json
```

A proposal for a coordinator step is refused by standalone `run`. Within a wave,
all steps must have the same approved attempt ceiling; ordinary steps retain two.
Use a separate wave for an escalation with a higher ceiling. This avoids silently
raising another step's allowance through a shared runtime. Output-retention
settings must also match. Fail-fast or explicitly cancelled workflows cannot be
revived by an escalation.

Coordinate `verify`, `service` and `artifacts` reopen the same bundle after a
restart. Verification control submission binds the proposal's input projection;
it does not treat that projection as a proof. The owner reconstructs the opaque
runtime decision and confidential final request before applying the queued
control. A successful transport remains UNKNOWN objective evidence until an
explicit verification record is applied.

Admission is serialized within a coordinator owner. Cumulative declared
cost/token reservations are checked before runtime review and checked again when
the coordinator records the step. The runtime claim and coordinator step row live
in separate databases: they are not a distributed transaction. If a crash occurs
between them, reviewing the exact target again reuses that durable claim and
finishes the step registration. A cancellation after reservation conservatively
keeps the reservation consumed and blocks dispatch; it does not refund authority.

Ordinary CLI tests use local fake worker processes and deliberately retain UNKNOWN
cleanup. They establish refusal and accounting behavior. The ignored native
canaries use a built `router-sandbox-helper`, real OS containment and a synthetic
Claude-speaking executable to establish the positive path, restart, source/store
pins and final approval. They invoke no real provider and establish no live model
availability, success quality or measured token/cost claim.
