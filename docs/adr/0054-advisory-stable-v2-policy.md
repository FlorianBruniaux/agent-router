---
adr: "0054"
title: Risk- and kind-aware advisory scoring as a compiled stable-v2 policy
status: accepted
created: 2026-11-02
last_modified: 2026-11-02
scope: "The advisory scoring policy behind `advise` (stage targets, rank key, effort table, skill cap, report reason codes) and the audit of Codex catalog costs. `route` keeps `stable-v1` by default; phase routing and the `PhasePolicy` lifecycle are unchanged."
amended_by: []
supersedes: []
superseded_by: []
related: ["0002", "0019", "0051", "0052", "0053"]
---

# ADR 0054: Risk- and kind-aware advisory scoring as a compiled stable-v2 policy

## Context

Issue #242 asks which engine carries risk- and kind-aware advice, and asks for
an audit of the catalog costs, before #235 and #236 write any code. Facts below
were read at `c61529c`.

### Why advice is flat today

- `advise` and `route` both call `Router::stable_v1()`
  (`crates/agent-router-cli/src/advisory.rs:852`,
  `crates/agent-router-cli/src/main.rs:692`). The policy is a version string,
  `stable-v1`, with no data (`crates/router-policy/src/route.rs:20-25`).
- Preferred effort is fixed for every task: plan `high`, execute `medium`,
  verify `high` (`crates/router-policy/src/preset.rs:47-51`), then the nearest
  level the host supports (`preset.rs:92-128`).
- The plan score is `min(stage quality) - sum(cost)/4 - sum(latency)/10 +
  sum(skill score)/20` (`crates/router-policy/src/score.rs:166-172`). Risk is
  not a term. Ties go to lower total cost, then to the lexical model key
  (`score.rs:220-227`).
- Risk enters only through two hard rules: for `high` and `critical`, the
  verifier quality must not be below the executor's (`score.rs:65-68`) and the
  verify stage gets `fresh_instance` (`score.rs:189-195`). Kind enters only
  through the max-quality verifier for `security` and `architecture`
  (`score.rs:31-61`); `critical` risk does not require it.
- Because of `min`, the plan stage does not count whenever execute or verify is
  weaker, and the lexical key then picks `gpt-5.6-sol` (ADR 0053,
  Consequences). On 2026-11-02, the #234 run on seven tasks gave the same plan
  for five of them (issue #234, Context table).

### Data finding: the catalog pins the plan stage

In `assets/catalogs/codex-core-v3.json`, the plan-capable rated models are
`gpt-5.6-sol` (quality 950000), `gpt-6-sol` (970000) and `gpt-6.1-sol`
(990000). All three have `cost_micros` 900000 and `latency_micros` 900000. The
other plan-capable entries (`codex-auto-review`, `gpt-6-astra`, `gpt-reserve`)
are unrated: quality 0, cost and latency 1000000. Terra and Luna entries do not
list the `plan` stage.

So `gpt-6.1-sol` weakly dominates every plan candidate: higher or equal
quality, equal or lower cost and latency. Any rule that is non-decreasing in
quality and non-increasing in cost and latency, with the model key only as a
last tie-break, selects it for the plan stage, whatever the margins.

Two Python reimplementations of the draft rule below, neither of them the
Rust code and neither committed, agree:

- the orchestrator's run picked `gpt-6.1-sol` for the plan stage in 8 of 8
  example tasks (issue #242);
- a run written for this ADR, over all 24 kind and risk cells (6 kinds x 4
  risks, every v3 model eligible, hard rules applied), picked `gpt-6.1-sol` for
  the plan in 24 of 24 cells. Its dominance check can be redone by hand from
  the cost audit table. Non-dominated models per stage: plan
  `gpt-6.1-sol`; execute `gpt-6-luna`, `gpt-6.1-terra`, `gpt-6.1-sol`;
  verify `gpt-6.1-terra`, `gpt-6.1-sol`. The same check on
  `codex-core-v2.json` gives `gpt-5.6-sol` alone for the plan stage, and on
  `claude-core-v1.json` five identical entries, decided by the model key.

No scoring rule can vary the plan stage on these catalogs. Execute and verify
can vary. The report must say so instead of implying differentiation.

### The two engines

Two scoring engines exist.

- `route` (and `advise`): the compiled `stable-v1` above, over catalog
  `ModelSpec` entries, producing one `DecisionEnvelope` for the
  `plan-execute-verify` preset, schema `"1"`.
- `route-phase`: `PhasePolicy` data (`crates/router-types/src/phases.rs:131-163`):
  `quality_weight`, `relevance_weight`, `cost_weight`, `latency_weight`,
  `minimum_utility_micros`, `ambiguity_margin_micros`, `max_alternatives`,
  default `phase-v2`. It scores one phase over a caller-supplied candidate pool
  (`PhaseRouteRequest`) and produces a `PhaseReceipt`. Its lifecycle in
  `crates/router-feedback/src/policy.rs` is baseline, propose, evaluate on
  replay records, shadow and canary trials, signed apply (ADR 0019).

## Options

### Option A: compiled `stable-v2` in the `route` engine

A second compiled policy beside `stable-v1`, selected by version string. Its
tables (margins, tightening, efforts, skill cap) are `const` data indexed by
enums, with a published canonical digest. `stable-v1` stays byte-identical.

Costs: tables change only by a new policy version and a code review; no
signed promotion and no offline comparison through `eval compare`.

### Option B: advisory scoring as `PhasePolicy` data

Reuse `PhasePolicy` storage, promotion and `eval compare`, so v1 and v2 can be
compared offline on sealed records. Read against the code, B costs more than it
looks:

- `PhasePolicy` cannot express per-stage targets by risk and kind. Its fields
  are four global weights and two global thresholds, with no stage, risk or
  kind index, and the struct is `deny_unknown_fields` (`phases.rs:131-146`).
  The tables need a new `PhasePolicy` schema version.
- The promotion safety gate does not cover new fields. `validate_delta`
  bounds only the four weights, the two thresholds and `max_alternatives`
  (`policy.rs:546-570`). A table field would pass unbounded until the gate is
  rewritten.
- A declared prior cannot enter the lifecycle. `propose_policy` requires an
  active baseline, learning consent and at least `minimum_verified_samples`
  verified objectives in the feedback store (`policy.rs:202-211`). An
  owner-declared table with zero samples is refused by design.
- There are no records to compare. `evaluate_proposal` replays `PairedTrace`
  values whose receipts carry a `PhasePolicy` hash (`policy.rs:259-292`), and a
  `ReplayInput` holds a `PhaseRouteRequest` and a map of `PhasePolicy`
  (`crates/router-eval/src/replay.rs:97-112`). `advise` produces a
  `DecisionEnvelope` and no replay record, so v1 and v2 advice cannot be
  sealed and compared without a new plan-level record format.
- The contracts differ: a phase receipt selects one candidate for one phase
  from a supplied pool with priced estimates; the advisory decision selects a
  three-stage plan from catalog ratings with plan-level hard rules
  (verify not below execute, max-quality verifier). Either `advise` moves to
  phase routing, which needs candidate pools, permissions and a registry that
  the advisory path does not have, or the `Router` learns to read
  `PhasePolicy`, which mixes two decision contracts kept apart since ADR 0002.

What B would buy: one lifecycle for every policy change, a content hash
instead of a version string, and a path for outcome-driven changes (#240).

## Decision

Recommended: Option A. Option B is deferred, not rejected; see the trigger in
Rejected and deferred options.

### Policy selection and compatibility

- Add `PolicyConfig::stable_v2()` and `Router::stable_v2()` with
  `policy_version` `stable-v2`. `stable-v1` stays frozen: same code path, same
  output for every existing fixture.
- `advise --policy stable-v1|stable-v2`, default `stable-v2`.
  `route --policy stable-v1|stable-v2`, default `stable-v1`. The local route
  API (ADR 0012) stays on `stable-v1`.
- ADR 0051 promises that `decision.json` equals what `route` prints for the
  saved `route-request.json`. `RouteRequest` carries no policy, so that replay
  becomes `route --policy <policy_version of decision.json>`. On acceptance,
  this ADR amends ADR 0051 on that point.
- Decision schema stays `"1"` and gains no field. `decision_id` already hashes
  `policy_version` (`route.rs:86-91`, `route.rs:222-239`), so no v2 id collides
  with a v1 id for the same request and catalog.
- `utility_micros` under `stable-v2` is minus the total shortfall (0 when every
  target is met). It is documented as policy-specific and not comparable across
  policy versions.
- The `advise` report stays at `schema_version` 1 and gains an additive
  `scoring` object. `feedback advisory-record` reads only known fields of the
  saved report (`crates/agent-router-cli/src/advisory_outcome.rs:70-80`).
- Rollback: `advise --policy stable-v1` reproduces the #234 v1 golden byte for
  byte.

### Stage targets

For each stage `s`, with `maxq_s` the highest quality among the eligible
models at that stage, after hard constraints:

```text
tau_s = max(0, min(maxq_s, maxq_s - margin(risk, s) + tightening(kind, s)))
```

The cap at `maxq_s` matters. Without it, the draft values give targets above
any reachable quality on v3: 1010000 for the plan stage of `architecture` and
`migration` at `high` and `critical` risk, and 1030000 for their verify stage
at `critical`. The kind tightening would then be a dead parameter. Targets are relative to the eligible maximum, so an unrated catalog
(every quality 0) gets every target 0 and keeps today's tie-break behaviour.

Margins, declared priors, not measurements (plan / execute / verify, micros):

| Risk | Plan | Execute | Verify |
| --- | --- | --- | --- |
| low | 40000 | 340000 | 190000 |
| medium | 20000 | 190000 | 140000 |
| high | 0 | 140000 | 40000 |
| critical | 0 | 40000 | 0 |

Kind tightening, declared priors (added to the target before the cap):

| Kind | Plan | Execute | Verify |
| --- | --- | --- | --- |
| architecture | +20000 | +20000 | +40000 |
| migration | +20000 | 0 | +40000 |
| security | 0 | 0 | +40000 |
| review | 0 | 0 | +40000 |
| explore | 0 | -30000 | -30000 |
| implement | 0 | 0 | 0 |

Margins are non-increasing in risk for every stage, so targets never decrease
as risk increases.

### Rank key and hard rules

Plans are ranked by a total order:

1. total shortfall `sum_s max(0, tau_s - q_s)`, ascending;
2. total `cost_micros`, ascending;
3. sum of stage qualities, descending;
4. total `latency_micros`, ascending;
5. model key (stage model ids joined by NUL), ascending.

Key 3 replaces the `min` aggregate as the quality tie-break, which removes the
lexical `gpt-5.6-sol` pick documented in ADR 0053. Skill scores are not in the
key: skills are chosen after the plan (below), so a skill cannot flip a model.
The streamed top-3 selection is unchanged.

Hard rules stay, with their exclusion codes: verify not below execute for
`high` and `critical`, max-quality verifier for `security` and `architecture`,
`fresh_instance` for `high` and `critical`. New: `critical` risk also requires
the strongest eligible verifier, under the existing max-quality rule with a
`verification.critical.not_max_quality` exclusion code.

A model consulted inside the executor's session, such as a Claude Code
advisor, reads the executor's full transcript. It never satisfies
`fresh_instance` and never replaces a `verify` stage. Recommending an advisor
pairing is deferred to #251.

### Effort table

Preferred effort per stage, declared priors, not measurements:

| Risk | Plan | Execute | Verify |
| --- | --- | --- | --- |
| low | medium | low | medium |
| medium | high | medium | medium |
| high | high | medium | high |
| critical | xhigh | high | xhigh |

Kind adjustments: `architecture` and `migration` plan +1 level; `security` and
`review` verify +1; `explore` verify -1. Never below `low`, never above `max`.
Unsupported levels fall back through the existing `effort_preference`, and the
report names the missed target. Effort stays one candidate per model and
stage: it is not a search dimension and does not enter cost. Effort cost is
reported as not modelled.

### Skills under stable-v2

Skills attach after the plan is selected, to the selected model of each stage,
under the existing eligibility rules (`preset.rs:171-201`). At most 2 skills
per stage, ranked by operator `--skill` first, then score descending, then id;
candidates other than operator ones need a score of at least 300000 (declared
prior). The source of candidates is ADR 0055.

### Report reason codes

The `scoring` object reports per stage: target, chosen quality, shortfall,
effort and its provenance (`policy_table` or `fallback`), the policy table
digest, and a reason code when the stage cannot vary:

- `catalog.stage.dominated`: one eligible model weakly dominates every other on
  quality, cost and latency, so no target changes the pick (v3 plan stage);
- `catalog.cost.equal`: every model meeting the target has the same cost, so
  the pick comes from the quality tie-break;
- `catalog.unrated`: every eligible model has quality 0, so the pick is a
  tie-break.

These codes describe catalog data, not model quality.

### Determinism

Integer arithmetic only, with checked operations and the existing
`route.arithmetic.out_of_range` error. No clock, store, network or randomness.
The rank key ends on the model key, and effort is a function of kind, risk,
stage and the host's supported efforts, so the order never depends on
iteration order. The work limit is unchanged; under `stable-v2` the skill
payload per combination is at most 1 + 3 x 2 = 7.

### Cost audit

| Catalog | Model | Quality | Cost | Latency | Catalog `source` | Set by | Real relative price |
| --- | --- | --- | --- | --- | --- | --- | --- |
| v3 | `gpt-6.1-sol` | 990000 | 900000 | 900000 | `operator-declared-policy-prior` | ADR 0053 (cost mirrored from `gpt-5.6-sol`) | UNKNOWN |
| v3 | `gpt-6-sol` | 970000 | 900000 | 900000 | `operator-declared-policy-prior` | ADR 0053 (cost mirrored from `gpt-5.6-sol`) | UNKNOWN |
| v2, v3 | `gpt-5.6-sol` | 950000 | 900000 | 900000 | `builtin-policy-prior` | commit `684c526` (codex-core-v1, 2026-08-29), no ADR, no stated basis | UNKNOWN |
| v3 | `gpt-6.1-terra` | 860000 | 550000 | 550000 | `operator-declared-policy-prior` | ADR 0053 (cost mirrored from `gpt-5.6-terra`) | UNKNOWN |
| v2, v3 | `gpt-5.6-terra` | 820000 | 550000 | 550000 | `builtin-policy-prior` | commit `684c526`, no ADR, no stated basis | UNKNOWN |
| v3 | `gpt-6-luna` | 690000 | 200000 | 200000 | `operator-declared-policy-prior` | ADR 0053 (cost mirrored from `gpt-5.6-luna`) | UNKNOWN |
| v2, v3 | `gpt-5.6-luna` | 650000 | 200000 | 200000 | `builtin-policy-prior` | commit `684c526`, no ADR, no stated basis | UNKNOWN |
| v2, v3 | `codex-auto-review`, `gpt-6-astra`, `gpt-reserve` | 0 | 1000000 | 1000000 | `builtin-unrated-policy-prior` | conservative unrated bound (`assets/catalogs/README.md`) | UNKNOWN |
| v2 | `gpt-6-sol`, `gpt-6.1-sol`, `gpt-6-luna`, `gpt-6.1-terra` | 0 | 1000000 | 1000000 | `builtin-unrated-policy-prior` | commit `cecc000` (2026-10-26); `gpt-6.1-terra` by ADR 0053 | UNKNOWN |

No committed file states a real price for any Codex model. The only
`PricingTable` data in the repository are test fixtures (`fixture-alpha`,
`test-model`). ADR 0053 and the catalog README say the costs are "not priced"
and "mirrored". The equal Sol costs are therefore a copy, not a statement that
the three models cost the same.

Consequence: until a catalog revision changes these values, the plan stage on
v3 is `gpt-6.1-sol` for every task, and the report says so with
`catalog.stage.dominated`. This ADR does not change the catalog. Evidence that
would change the plan-stage result, each through a reviewed catalog revision:

- a committed price source (provider price list with date, or measured
  `cost_microusd` from `feedback advisory-report` per model) showing that the
  Sol models differ in price;
- an owner declaration, with its basis, that a cheaper tier (Terra) may plan,
  which is a `stages` change, not a cost change;
- outcomes (ADR 0052) showing that the quality order of the Sol models does
  not hold.

## Consequences

- Execute and verify vary with risk and kind on v3: in the Python run above,
  low-risk tasks get `gpt-6-luna` on execute and critical tasks `gpt-6.1-sol`
  on every stage. Whether these plans give better outcomes is UNKNOWN until
  outcomes accumulate per `policy_version`.
- The plan stage does not vary on v3, v2 or the Claude catalog. That is a
  catalog limit, stated in the report.
- Margins, tightening, efforts, the skill floor and the cap are declared
  priors. Changing one requires a new policy version, a golden update and a
  code review; there is no signed promotion for them.
- `utility_micros` changes meaning between policy versions. Consumers outside
  the repository were not inventoried.
- `route` users see no change unless they pass `--policy stable-v2`.
- `advisory-report` should split outcomes by `policy_version` (#239), or v1
  and v2 samples mix.

### UNKNOWN

- Real relative prices of every Codex model.
- Real effect of effort per Codex model; `gpt-6.1-terra` efforts are inferred
  (ADR 0053).
- Whether `stable-v2` advice is followed or accepted more often than
  `stable-v1`.
- External consumers of `utility_micros`.

### Acceptance

| Requirement | Test or check | Issue |
| --- | --- | --- |
| Each stage varies with risk or states why | `matrix_each_stage_varies_with_risk_or_states_why` (ignore removed) | #234, #235 |
| Low-risk execute cheaper than critical | `matrix_low_risk_execute_is_cheaper_than_critical` | #234, #235 |
| Critical uses the strongest verifier | `matrix_critical_uses_strongest_eligible_verifier` | #234, #235 |
| Effort never decreases with risk | `matrix_effort_never_decreases_with_risk`, `stage_policy_efforts_are_monotonic_in_risk` | #234, #236 |
| v1 frozen | `v1_policy_is_frozen_for_all_fixtures`; `advise --policy stable-v1` equals the #234 v1 golden | #235 |
| Target cap | `v2_targets_never_exceed_max_eligible_quality` | #235 |
| Identity | `v2_decision_id_differs_from_v1`; work-limit test on v1 and v2 | #235 |
| Effort table total and fallback | `v2_policy_table_is_total`, `effort_falls_back_to_nearest_supported_level` | #236 |
| Docs | ADR 0051 `amended_by` 0054; `docs/reference/json-contracts.md` lists policy versions and reason codes; CHANGELOG | #235 |

## Rejected and deferred options

- Option B, `PhasePolicy` data: deferred. The tables do not fit the
  `PhasePolicy` schema, the promotion gate refuses declared priors without
  verified samples, and no plan-level replay record exists. Trigger to revisit:
  when #240 has at least 30 recorded outcomes for `stable-v2` and the owner
  wants outcome-driven table changes, design a plan-level, hashed policy
  artifact with its own bounded delta gate, rather than stretching
  `PhasePolicy`.
- Changing `stable-v1` in place: breaks replay of saved requests and every
  existing `route` fixture.
- Mid-run escalation, handing the task to a stronger model after a failure:
  rejected. "The Handoff Tax" (arXiv 2608.24358, 2026-08-25) reports that
  escalation with full trajectory transfer recovers less than half of the
  quality gap between the cheaper and the stronger model, at a substantial
  cost premium, while downshifting gives a better trade-off. This is external
  evidence on full handoffs between Claude and GPT family pairs, not measured
  on this repository's tasks; it supports a strong plan stage followed by a
  cheaper execute stage, which `stable-v2` already encodes.
- A scalar `E[Q] - lambda(risk) x C`: needs a calibrated `lambda` that does not
  exist. An earlier in-memory exploration (not committed) converged to all-Sol
  plans for every `lambda` tried, so it would not offer cheaper models.
- Effort as a search dimension, one candidate per (model, effort): on v3 the
  arms reach 34 x 56 x 46 = 87,584 combinations (issue #236) and need per-effort
  cost and quality multipliers that are not measured.
