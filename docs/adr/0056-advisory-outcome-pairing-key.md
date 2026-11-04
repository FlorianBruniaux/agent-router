---
adr: "0056"
title: Store the request hash of an advisory outcome as its pairing key
status: amended
created: 2026-11-02
last_modified: 2026-11-03
scope: "The `request_hash` of the advisory outcome record, the feedback store schema 4 migration, the `request_hash` field `advise` saves in `advice.json`, and the `paired_trials` section of `feedback advisory-report`. Amends ADR 0052."
amended_by: ["0058"]
supersedes: []
superseded_by: []
related: ["0051", "0052", "0054"]
---

# ADR 0056: Store the request hash of an advisory outcome as its pairing key

## Context

ADR 0052 binds an advisory outcome to one `decision_id`. That digest hashes the
request together with the catalog and policy versions, so two policies deciding
the same task give two unrelated ids. `advise --compare-policies` already writes
a shared `request_hash` in `compare.json`: SHA-256 with domain
`agent-router.advise.request.v1` over the serialized route request and the hash
of the host observation that binds it, empty when the host is declared
(`request_hash` in `crates/agent-router-cli/src/advisory.rs`). The store did not
keep it, so `feedback advisory-report` printed `paired_trials: "not_available"`
(issue #239) and could not tell two arms of one task from two tasks (issue
#255). A single `advise` export did not save the hash at all.

## Decision

- `AdvisoryBinding` gains an optional `request_hash` (64 lowercase hex
  characters), omitted when absent. The record keeps `schema_version` 1, as
  `FeedbackInput` did for its optional keys: a record without the field keeps
  its bytes and its `outcome_hash`. Absent means UNKNOWN.
- The feedback store moves to `user_version` 4. The migration adds a nullable
  `request_hash` column to `advisory_outcomes` in one transaction. It reads,
  rewrites and backfills no row, so a version 3 outcome keeps its bytes and
  reads with an UNKNOWN hash. Every read checks the column against the hashed
  event, like `decision_id`.
- `advise` adds `request_hash` to `advice.json`, the same value for a single
  advice and for each arm of a comparison of the same request. `advisory-record`
  copies it from `advice.json`. For a comparison it takes the hash of
  `compare.json` for every arm and refuses an arm whose `advice.json` names
  another one, so two arms of one comparison cannot get different hashes. An
  export that predates the field records UNKNOWN; a malformed value is refused.
- No attempt of a decision may state a request hash other than one an earlier
  attempt stated (`feedback.invalid`). An attempt without a hash stays allowed.
- `feedback advisory-report` moves to `schema_version` 3 (hash domain
  `advisory-outcome-report-v3`). `paired_trials` becomes an object keyed by
  `request_hash` over the latest attempt of each decision. An arm is one
  configuration, the pair of `policy_version` and `catalog_version`. A trial is
  a request hash with two or more configurations; each arm counts it once and
  arms are never merged, so a trial with three configurations adds one to each
  of three arms. A request hash where two decisions share one configuration is
  counted as `ambiguous_trials` and left out. Decisions with a single
  configuration count as `unpaired_decisions`, and decisions without a hash as
  `unknown_request_hash`. Arm rates follow the `minimum_sample` rule of the
  groups.
- Following neither plan of a comparison stays as before: one `--overridden`
  record per policy, each with the `--used-*` fields. Both records carry the
  request hash, so the pair is visible without a new adherence value.

## What the hash binds and what it does not

The hash binds the serialized route request (task context, host snapshot,
skill candidates, capability knowledge) and, when the host is an unmodified
observation, that observation's hash. It does not bind the catalog, the policy,
the decision, the outcome, the time, or anything the user did. Like the other
fields of the binding, it is copied from a file the user can edit and is not
recomputed: it correlates records and authenticates nothing.

## Consequences

The report can count the arms of one task side by side without inferring the
pairing from other fields. Records written before this change, or from an older
export, stay in every other section and appear only as `unknown_request_hash`
in the paired one. A version 4 store is refused by binaries that predate this
change (`future schema`); there is no downgrade. Report schema 2 consumers that
read `paired_trials` as a string must follow version 3. Paired counts remain a
personal, self-selected sample and carry no attribution.

## Rejected options

- Recompute the hash from `route-request.json` at record time: it depends on a
  re-serialization of an editable file, and the observation hash it needs is not
  in the export.
- Infer pairs from task kind, risk and recording time: it would pair different
  tasks and cannot be checked.
- Key arms by policy version alone: two catalogs of one policy on the same
  request would then count one trial twice in one arm.
- A new `neither` adherence for a comparison: it would need a record without a
  decision, which ADR 0052 excludes, and the per-policy override already says it.
