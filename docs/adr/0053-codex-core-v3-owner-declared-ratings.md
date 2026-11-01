---
adr: "0053"
title: Owner-declared ratings for the GPT-6 Codex models in codex-core-v3
status: implemented
created: 2026-11-01
last_modified: 2026-11-01
scope: "The `codex-core-v3` catalog and its declared ordering of the GPT-6 models. Not a measurement of quality, price or latency."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0053: Owner-declared ratings for the GPT-6 Codex models in codex-core-v3

## Context

`codex-core-v2` lists the GPT-6 models as unrated (quality 0, cost and latency
1,000,000, no capabilities), so `advise` never prefers them over GPT-5.6. The
owner's Codex model cache of 2026-11-01 also lists `gpt-6.1-terra`, which v2 did
not know; a declared host listing it was refused with `advisory.model.unknown`.
The catalog README says a later rating needs a separately reviewed policy
revision.

## Decision

Add `gpt-6.1-terra` to v2 as an unrated entry, so declared hosts listing it are
accepted. Add `assets/catalogs/codex-core-v3.json` (version `codex-core-v3`) for
the owner-declared ratings; v2 keeps every GPT-6 entry unrated.

- v3 is v2 with ratings on four entries. `gpt-6.1-terra` has `low` through `ultra` efforts inferred from
  the tier pattern (`gpt-5.6-terra` and `gpt-5.6-sol` share that set), not observed.
- The owner declares the ordering 6.1 before 6 before 5.6 within a tier, and Sol
  before Terra before Luna. Qualities: `gpt-6.1-sol` 990000, `gpt-6-sol` 970000,
  `gpt-6.1-terra` 860000, `gpt-6-luna` 690000. The 5.6 entries are unchanged.
- Cost, latency, capabilities and stages of each newly rated entry mirror its
  GPT-5.6 tier counterpart. Costs are not priced and latencies are not timed.
- The newly rated entries use the source `operator-declared-policy-prior`. The
  validator accepts any plain-text label, so no code changed.
- `gpt-6-astra`, `codex-auto-review` and `gpt-reserve` stay unrated.

## Consequences

The numbers are a declaration, not a measurement. Check them against
`feedback advisory-report` (ADR 0052) before relying on them, and revise the
catalog if followed and overridden outcomes disagree.

The plan score takes the minimum quality over its three stages (`score.rs`,
`to_ranked_plan`). When a plan also uses a terra model, a 6.1 sol and a 5.6 sol
give the same score for the plan stage, and the lexical model key then picks
`gpt-5.6-sol`. With the owner's eight-model pool, `advise` plans with
`gpt-5.6-sol` and executes and verifies with `gpt-6.1-terra`. A 6.1 sol is chosen
for the plan only when no GPT-5.6 sol is declared or the plan has no weaker stage.

Current docs point to v3; v1 and v2 remain for historical examples and fixtures.
