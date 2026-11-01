---
adr: "0052"
title: Record the manual outcome of an advisory recommendation
status: implemented
created: 2026-10-31
last_modified: 2026-10-31
scope: "The `feedback advisory-record` and `feedback advisory-report` commands and the advisory outcome table of the feedback store. Personal, descriptive records: no execution, no learning, no savings claim."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0052: Record the manual outcome of an advisory recommendation

## Context

Issue #219 asks for a way to record, after doing a task by hand, whether the
`advise` recommendation (ADR 0051) was followed and how it went, then to read a
local report of followed against overridden recommendations.

The existing record does not fit. `FeedbackInput` requires a `FeedbackBinding`
with `run_hash`, `receipt_hash` and `snapshot_hash`, and `validate_input`
(`crates/router-feedback/src/lib.rs`) rejects any of them that is not a 64-digit
hash. A manual task has no supervised run, no receipt and no approval, so those
hashes would have to be invented, and the report would then present an invented
run next to a real one. `record` also keys events on `(run_hash, origin)` and the
report groups by executed model and effort, which an advisory outcome does not
have for an overridden recommendation.

`advise` already writes a stable identity: `decision.decision_id`, a SHA-256 over
the canonical route request, the catalog version and the policy version
(`crates/router-policy/src/route.rs`, `decision_id`), saved in `advice.json`.

## Decision

Add a separately typed record, `AdvisoryOutcomeInput` (schema 1), stored in a new
`advisory_outcomes` table of the same consented feedback store (`user_version`
3, added in one transaction that leaves events and facts untouched).

- The record binds to the saved decision by `decision_id`, with the catalog and
  policy versions, the task kind, the effective risk, the `selection_basis` and
  the recommended phases (stage, model, effort, skills), all read from
  `advice.json`. It holds no run, receipt or approval hash. Only `decision_id`
  is a digest of the request; the other fields are copied from a file the user
  can edit and are not bound to it, so the per-kind grouping rests on the user's
  own file.
- The user states `adherence` (`followed` or `overridden`), `result`
  (`accepted_as_is`, `accepted_after_rework` or `abandoned`) and optional host
  usage (`cost_microusd`, `tokens`). An override states at least one of the
  model, effort and skills used instead. An unstated value is `null`, which means
  UNKNOWN, and an unstated skills list is not "no skill".
- No field holds a prompt, objective, path or comment. Every string is an enum
  value, a hash or an identifier. The identifier check is syntax only, not
  catalog membership: the feedback identifier rule (at most 96 characters of
  `A-Za-z0-9._:/-`) minus `/`, `\`, a leading `.` and a leading `~`, so a path
  without spaces is refused too. It applies to the used model and skills and to
  the model, skill and version strings copied from `advice.json`.
- An `overridden` record whose used model, effort and skills are all stated and
  equal every recommended phase restates the recommendation and is refused.
- The same task advised again gives the same `decision_id` (the id hashes the
  request, catalog and policy, not the time). Repeating the latest record, even
  at another second, returns it. A different outcome for that decision is stored
  as its next `attempt`, so a retry or a correction needs no purge and nothing is
  rewritten. The report counts one sample per decision, its latest attempt, and
  shows the record count beside it. Attempts of a decision are not compared.
- Recording needs the same collection consent, clock and retention rules as
  other feedback. `export` lists the outcomes under `advisory_outcomes`, omitted
  when empty so an earlier export keeps its hash. Disabling collection, expiry
  and `purge` delete them.
- `feedback advisory-report` groups by task kind, with followed and overridden
  side by side. Every rate carries its numerator and denominator, the sample size
  sits next to it, and UNKNOWN counts (cost, tokens, and for overrides the model,
  effort and skills) and the `selection_basis` counts accompany it. An empty
  sample has a null rate, never zero. Costs are counted as known or unknown, not
  summed or compared. `attribution` is `not_established`.

The ordinary `feedback report`, `summary`, statistics, drift and policy learning
do not read these records.

## Consequences

A user can run advise, do the task, record the outcome and read a local report
without fabricating run evidence. The records are self-reported: the router
verifies neither the adherence, the result nor the cost, and `decision_id`
correlates records, it does not authenticate them. The sample is personal and
self-selected, so the report is not evidence of a general gain; comparative
evaluation stays with issue #5. A recommendation that was
`no_recommendation` has nothing to follow and is refused
(`advisory.outcome.not_recommended`), and a file that is not a version 1 advice
report is refused (`advisory.outcome.advice_invalid`). The reader accepts at most
1 MiB, the same bound `advise` applies to each document it publishes. A version 3
store is refused by binaries that predate this change (`future schema`); there
is no downgrade.

## Rejected options

- Reuse `FeedbackInput` with synthetic run, receipt and snapshot hashes: it fakes
  evidence, and the existing report would count these records as runs.
- Add the fields to `feedback report`: its published schemas are strict and
  versioned (v3 to v5), so a new section would change a hashed contract.
- Sum the reported costs of followed and overridden work: with a self-selected
  sample it invites the savings claim the issue excludes.
