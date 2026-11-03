---
adr: "0057"
title: Declared ratings and price-based costs for the Claude models in claude-core-v2
status: proposed
created: 2026-11-03
last_modified: 2026-11-03
scope: "The `claude-core-v2` catalog: proposed quality ordering and stages, and costs derived from published output prices. Not a measurement of quality or latency."
amended_by: []
supersedes: []
superseded_by: []
related: ["0053", "0054"]
---

# ADR 0057: Declared ratings and price-based costs for the Claude models in claude-core-v2

## Context

`claude-core-v1` lists five Claude models as unrated (quality 0, cost and latency
1,000,000), so `advise` picks by identifier order and returns
`claude-haiku-4-5-20251001` for every stage with `selection_basis:
unrated_tie_break` (golden matrix `advise-matrix-v2.json`, variant
`claude-v1-flat-declared-tools`). A Claude Code user gets a name, not a
recommendation. Issue #218 asks for a usable Claude catalog; unit U07 of #241
left the rating to the owner.

## Decision

Add `assets/catalogs/claude-core-v2.json` (version `claude-core-v2`, schema 1).
It is `claude-core-v1` with four fields changed per entry: `quality_micros`,
`cost_micros`, `stages` and `source` (`operator-declared-policy-prior`). Ids,
efforts, capabilities, latency and skills are unchanged. v1 stays for existing
fixtures and examples.

| Entry | Quality | Cost | Stages | Output price (USD per MTok) |
| --- | --- | --- | --- | --- |
| `claude-fable-5-1` | 990000 | 1000000 | plan, execute, verify | 50 |
| `claude-opus-5-5` | 970000 | 400000 | plan, execute, verify | 20 |
| `claude-sonnet-5-5` | 860000 | 200000 | execute, verify | 10 |
| `claude-haiku-5-5` | 690000 | 10000 | execute | 0.50 |
| `claude-haiku-4-5-20251001` | 650000 | 100000 | execute | 5 |

- Cost is the published first-party output price divided by the highest one
  (Fable 5.1, 50 USD per million output tokens), times 1,000,000. Source: the
  model table of the Claude API reference bundled with the coding agent, cached
  2026-10-06. Input prices keep the same ratios except Haiku 5.5, whose 0.10/0.50
  pair holds up to 100K prompt tokens. Cost is a price ratio, not the cost of a
  task: token counts and effort are not modelled (ADR 0054, `effort_cost:
  not_modelled`).
- Quality and stages are a proposed operator declaration that mirrors the
  `codex-core-v3` tiers (ADR 0053): Fable and Opus take the Sol role (990000,
  970000, all stages), Sonnet the Terra role (860000, execute and verify), the
  Haiku models the Luna role (690000 and 650000, execute only). They are not
  measured. The owner confirms or changes them before this ADR is accepted.
- Latency stays at the unrated bound 1,000,000 for every entry: no timing source
  exists, and `stable-v2` ranks latency only after shortfall, cost and quality.
- `claude-haiku-4-5-20251001` is rated below `claude-haiku-5-5` and costs more,
  so `stable-v2` never prefers it over Haiku 5.5 when the host lists both. Its
  effort list is unchanged; the API reference says effort is not accepted on
  Haiku 4.5, and whether Claude Code passes `--effort` to it is not established.

## Consequences

With a host listing Fable 5.1, Opus 5.5, Sonnet 5.5 and Haiku 5.5, `stable-v2`
returns six distinct plans over the seven golden cases (variant
`claude-v2-rated-declared-tools`), for example
`opus/high -> sonnet/medium -> sonnet/medium` for a medium implementation and
`fable/max -> opus/high -> fable/xhigh` for a critical migration, all with
`selection_basis: catalog_policy_prior`. `stable-v1` returns Opus for most
stages because its plan score takes the minimum quality over three stages.

Haiku is chosen only for an execute stage whose target is at most 690000, that
is a low effective risk; the normalizer raises `implement` and `internal`
sensitivity to medium, so quick-entry tasks rarely reach it.

The numbers are a declaration and a price ratio. Check them against
`feedback advisory-report` (ADR 0052) and revise the catalog if followed and
overridden outcomes disagree. The catalog authenticates no host, grants no
permission and authorizes no model execution.
