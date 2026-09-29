# Review provider separation

Status: library only. No CLI command, runtime gate or contract field uses it yet.

## Problem

`record_verification` in `router-runtime` refuses only `verifier == node.owner`
(`crates/router-runtime/src/lib.rs:1617-1621`). That compares logical node
identities. It does not stop a producer and its reviewer from running on the
same provider with two different models.

## What this adds

`router-policy` exports a pure function:

```rust
distinct_provider_pairs(source, target, producers, reviewers, base)
    -> Result<ReviewSeparation, ReviewSeparationError>
```

- `producers` and `reviewers` map an eligible candidate ID to its declared
  provider, `None` meaning UNKNOWN. `candidate_providers(&context)` builds that
  map for the eligible candidates only, the set the generator joins over
  (`eligible_selection`), from
  `ValidatedPhaseContext::asset_requirements(id).provider`
  (`crates/router-policy/src/asset_phases.rs`). An ineligible candidate is
  absent, even if its contract declares a distinct provider.
- `base` is the compatibility already declared for the edge.
- The result carries a `GenerationEdgeCompatibility` for
  `PlanGenerationInput::compatibility`, a verdict per pair and one edge verdict.

| Pair verdict | Meaning | Kept |
| --- | --- | --- |
| `Distinct` | both providers declared and different | yes |
| `SameProvider` | both declared and equal | no |
| `UnknownProvider` | a side has no assets or a blank label | no |

The edge verdict describes the edge in `compatibility`, computed over the base
pairs, not over the full producers x reviewers product. `pairs` keeps the
per-pair verdicts of the full product for diagnosis.

| Edge verdict | Condition |
| --- | --- |
| `ReviewerUnavailable` | no producer or no reviewer candidate |
| `CompatibilityUnknown` | `base` is `None`; the result is `None` and asserts no usable pair |
| `NoDeclaredPair` | `base` is a known empty set |
| `Distinct` | a base pair has distinct providers and survives |
| `SameProvider` | no distinct base pair, at least one same-provider base pair |
| `UnknownProvider` | no distinct or same-provider base pair (unknown label, or pair outside the candidate maps) |

## Rules

- Monotone: the output is a subset of `base`. A `None` base stays `None`
  (UNKNOWN, the generator abstains with `unknown_compatibility`). A pair absent
  from `base` is never created.
- An explicit empty set is returned when nothing survives, so the generator
  reports `no_feasible_plan` rather than UNKNOWN.
- Labels compare after trimming, ASCII case-insensitively, so `Vendor-A` and
  `vendor-a` are the same provider. This fails closed: it can refuse a
  legitimate plan if two distinct organisations differ only by case.
- The pair product is bounded by `MAX_GENERATION_EDGE_PAIRS`.
- Output is deterministic: sorted maps, no clock, no I/O.

## Trust limit

The provider is a string declared in the asset contracts document and checked
in two places. `crates/router-asset-contracts/src/gate.rs:130` compares
`model.provider` from the contracts with the request provider and
`policy.allowed_providers`. `gate.rs:138` then requires an availability binding
for the same model, provider and host (`documents.rs:85-89`).
It is not an authenticated identity. Two different labels can reach one
backend, and an operator who mislabels a model defeats the check. This is a
guard against configuration mistakes, not proof of independence.

The caller is responsible for the provenance of the `base` compatibility, as in
`docs/plan-generation.md`.

## Not covered

- Wiring into the CLI generation path or the runtime verification record.
- Multi-hop chains: only the edge given is restricted.
- Transport, account or organisation identity behind a provider.
