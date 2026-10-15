---
name: ar-session-handoff
description: "Agent Router session handoff: prepare an autonomous task transfer or resume one by revalidating revision, owned changes, evidence and current authority."
---

# ar-session-handoff

## Triggers

Ending or transferring a bounded development task, writing a reviewer handoff,
or resuming work from an existing handoff.

## Non-triggers

Publishing a release, sending messages without authorization, creating global memories.

## Inputs

Issue or current user request, workspace path, branch, base and candidate SHA,
owned and unrelated paths, acceptance criteria, applicable decisions, check
results and remaining blockers. On resume, also require the historical handoff
and access to the current repository. Record missing facts as UNKNOWN.

## Steps

### Prepare a handoff

1. Read current status and diff without staging. Done when workspace, branch,
   base/candidate SHA, owned files and unrelated changes are recorded.
2. Read `assets/prompts/autonomous-handoff.md` from the repository root. Fill
   every field from the current task and evidence. Include non-goals, acceptance,
   applicable ADRs and relevant rejected alternatives; label open questions as
   blocking or nonblocking. Keep only history needed to decide the next action.
   Done when a reader without the conversation can identify the intended result,
   constraints, unresolved questions and next responsible actor.
3. Record observed checks with their candidate SHA and relevant dirty-content
   hashes, command, result and evidence location. Separate reported history from
   current proof and authorized actions
   from proposed actions. Done when missing evidence is UNKNOWN and the handoff
   neither grants authority nor asserts runtime activation.

### Resume from a handoff

1. Preserve the original handoff as historical evidence. Re-read the current user
   request and applicable instructions. Done when scope, ownership and permitted
   next action come from current authority rather than the previous author's
   narrative. A handoff, ADR or ticket status cannot supply execution approval.
2. Inspect the live workspace, branch, HEAD, status and diff, including unrelated
   uncommitted files. Compare them with the recorded base and candidate; inspect
   referenced artifacts and hashes where available. Done when each dependency
   of the proposed next action is current, stale, missing or UNKNOWN. Preserve
   concurrent changes; do not reset or overwrite them to match the handoff.
3. Revalidate the facts that gate the next action. Bind checks to the actual
   candidate and rerun affected local checks when their inputs changed or their
   evidence is missing. For product runs, use the coordinator's exact request,
   artifact contract and receipt reconstruction; prose cannot replace it. Verify
   current approval scope and freshness before any authorized effect. Inspect
   receipts or external state before considering a repeated commit, push,
   publication or provider call; an ambiguous result remains UNKNOWN. Done when
   the next action has its required evidence, or execution is blocked with the
   missing fact and responsible actor identified.
4. Append a dated resume record with observed revision and dirty paths, evidence
   classifications, checks actually run, unresolved questions, current authority,
   next owner and one exact next action. Keep old results attributed to their
   original revision. Done when the record explains what is safe to resume and
   what remains blocked without rewriting history or repeating external effects.

## Checks

Run from the repository root, using `rtk` when available.

```sh
git status --short
git branch --show-current
git rev-parse HEAD
git diff --check
cargo run -p xtask --locked -- check
```

## Stop

Stop before dependent execution if source identity, ownership, required evidence
or authority is unresolved. A previous "Work Done" statement does not establish
current tests, repository state or external effects. Continue only independent
read-only investigation while resolving the blocker.

Do not send or publish the handoff without task authorization. Never describe
installed, discovered, invoked and verified as equivalent.

This is a textual skill, not an executable extension. Host permissions still apply.
