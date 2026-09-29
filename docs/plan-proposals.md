# Plan proposals (`router.plan_proposal.v1`)

Library-only intake of a plan proposed by an external source, such as a model.
There is no CLI entry point: a command taking a model plan would be a sensitive
surface and needs its own decision.

## What is trusted

A proposal is untrusted data. `review_plan_proposal(bytes, task, limits)` takes
the `NormalizedTask` from the caller. The task carries scope, budgets, risk and
allowed mutations, so a proposal can never widen them.

## Wire shape

```json
{
  "schema": "router.plan_proposal.v1",
  "proposer": "free text, at most 128 bytes",
  "preset": "plan-execute-verify",
  "nodes": [ "WorkflowNode ..." ],
  "max_depth": 16,
  "max_width": 8
}
```

Rules:

- Input above `MAX_PROPOSAL_BYTES` (256 KiB) is refused before parsing.
- Duplicate object keys are refused.
- Unknown fields are refused at the envelope and inside nodes.
- `plan_id` is refused if present. It is recomputed locally.
- `task` is refused as an unknown field.
- `proposer` is recorded as data. It never selects, weakens or grants anything.
- `requires_explicit_integration` is always `true` and cannot be supplied.

## Result

`PlanProposalReview` holds the locally sealed plan, its waves, the proposer
label and a `PlanDivergence` against `build_plan(task, preset, limits)`. The
report lists missing, extra and changed nodes (with changed field names) and
depth or width differences. If the baseline cannot be built, `baseline_error`
says why. `is_approved()` is always `false`: acceptance is not approval.

## Provenance is not established here

`validate_plan` checks that `plan_id` is the SHA-256 of the plan. Anyone who can
compute a hash can seal a plan, so a passing `validate_plan` shows integrity and
not provenance. The test `documents_that_validate_plan_accepts_any_recomputed_hash`
records this existing behavior.

Sealing locally does not settle provenance either. The returned plan is an
ordinary `WorkflowPlan`, and `seal_execution_contract` accepts it like any other.
Approval by digest must be bound downstream to this `plan_id`. `is_approved()`
is a constant `false` on the review value, not a guarantee about later stages.

## Free text from the model

`purpose`, `stop_rule`, `owner`, `inputs`, `outputs` and `verification` are free
text written by the proposer. They can carry instructions aimed at workers
(injection surface). The divergence report flags a changed field but the text
is not filtered. A human must read it before approval.

## Limits

- The caller's `PlanLimits` bound the proposal: `max_depth`, `max_width` and the
  node count above them are refused with `workflow.limit`.
- Input above `MAX_PROPOSAL_BYTES` is refused before parsing, then parsed with
  `router_adapter_api::strict_json`, which rejects duplicate object keys.

## Not covered

- No approval digest, execution or host request is produced here.
