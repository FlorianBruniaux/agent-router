---
adr: "0057"
title: Report known cost and token totals per advisory policy
status: implemented
created: 2026-11-03
last_modified: 2026-11-03
scope: "The `cost_microusd` and `tokens` aggregates and the fifth interpretation note of `feedback advisory-report`, and the move to report schema 4. Amends ADR 0056."
amended_by: []
supersedes: []
superseded_by: []
related: ["0052", "0056"]
---

# ADR 0057: Report known cost and token totals per advisory policy

## Context

ADR 0052 stores optional `cost_microusd` and `tokens` on an advisory outcome and
the report counted only how many were UNKNOWN. ADR 0056 added per policy arms
but an owner still could not compare what two policies cost.

## Decision

- Every `followed` and `overridden` object of the report (`overall`,
  `task_kinds`, `groups[]`, `paired_trials.arms[]`) gains `cost_microusd` and
  `tokens`, each `{records, total, mean}`. `records` counts the latest outcomes of
  decisions that state the value, `total` is their sum, `mean` is the total over
  `records` rounded half up.
- `mean` is null below `minimum_sample` (5) known values. This counts known
  values, not decisions, so a side of six decisions with four costs has a null
  cost mean while its rates are printed. Counts and totals are always printed.
- A missing value is in `unknown` and in no sum: it is never a zero.
- The report moves to `schema_version` 4 and hash domain
  `advisory-outcome-report-v4`. Earlier additive changes (v2 for `groups`, v3 for
  `paired_trials` and a fourth note) moved the version, the content hash covers
  every field, and `interpretation_notes` grows to five entries, so a consumer
  that expects the v3 bytes or a four entry array must follow the new version.
- A fifth interpretation note states that totals are host-reported or
  wrapper-estimated and self-reported, and that cost per decision is not
  comparable across task mixes.

## Consequences

Two policies can be read side by side on spending without a new command. The
totals describe a personal, self-selected sample and carry no attribution. The
store, the record and the export are unchanged. No JSON schema file covers this
report yet, so none changes.
