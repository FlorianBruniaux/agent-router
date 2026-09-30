# Router value protocol

Status: draft for pre-registration. Nothing below has run. No figure in this
document is a measured gain.

This protocol asks one question: does Agent Router help ship better quality,
faster, or cheaper, with AI FinOps as the main concern. It consumes the outcome
data decided in [ADR 0019](../adr/0019-outcome-measurement.md) and extends the
offline tools described in [evaluation](../evaluation.md).

Code references were reread on the branch based on `e4c5329`. Evidence markers
in the Source columns:

| Marker | Meaning |
| --- | --- |
| code | Read in the code at the cited `file:line` |
| doc | Repository documentation, ADR or cited web page |
| decision | Maintainer decision of 2026-09-30 |
| inferred | Reasoning in this protocol, not a measurement |
| assumption | To be verified, usually by a fixture before any costing |

## Summary

- A replay is not enough; each arm needs real runs. `router-eval` refuses to
  cost a selected model without observed usage for that model.
- `eval tradeoffs` is the closest existing base: 2 to 8 arms on the same cases,
  functional success, total cost, regret and a quality/cost frontier. It has no
  confidence interval, paired delta, strata or price sensitivity.
- `eval compare` does not fit fixed baselines: it needs a `RouteAccuracy` rule
  backed by human labels and pairs that differ only by `PhasePolicy`. "Always
  the frontier model" is not a `PhasePolicy`.
- Losing the prompt cache between phases is the main mechanism by which a
  per-phase router can cost more than a single model. Each approved phase runs
  in a separate host process. Arm E measures it.
- Reasoning tokens can invert the cost ranking. Arm G varies effort at a fixed
  model to separate the two effects.
- v1 routes per provider: `A_codex` and `A_claude` each run on their own host,
  because a routing context is bound to one host on `e4c5329`. Cross-host
  routing is a separate product work item and the analyses that need it are v2.
  See [arms](#arms).

| Fact | Marker | Source |
| --- | --- | --- |
| `"selected model usage missing"` refusal | code | `crates/router-eval/src/replay.rs:339-345` |
| `evaluate_tradeoffs` entry point | code | `crates/router-eval/src/tradeoffs.rs:168` |
| No `bootstrap`, `wilson` or `confidence_interval` in `router-eval` or `router-feedback` | code | `grep -rn` over both crates, empty |
| `compare_shadow` requires a `RouteAccuracy` rule | code | `crates/router-eval/src/promotion.rs:268-270` |
| Pairs must share `input_hash` and differ by policy | code | `crates/router-eval/src/promotion.rs:226-234` |
| Each approved phase runs as a separate host process | doc | `README.md:92-93` |
| Codex runs with `--ephemeral` | code | `crates/adapter-codex/src/execution.rs:44` |
| Claude runs with `--no-session-persistence` | code | `crates/adapter-claude/src/execution.rs:53` |
| Reasoning tokens can invert the cost ranking | doc (vendor benchmark) | https://docs.litellm.ai/docs/proxy/auto_routing_benchmark, consulted 2026-09-30 |

## Metrics

Unit: the task. A task succeeds when `integration.applied` is recorded and a
recorded verification has a `Passed` objective, which the feedback contract
already ties to `Passed` validation and an evidence hash (decision, ADR 0019).
In the bench the verifier is the frozen acceptance script, never the agent. A
task may have three phases (plan, execute, verify); its tokens, time and
attempts are summed over all its phases.

### Quality

| Metric | Source | Remaining `UNKNOWN` |
| --- | --- | --- |
| Q1. Accepted and verified success rate | `integration.applied`: `crates/router-runtime/src/lib.rs:1853`. `VerificationRecord.objective`: `crates/router-types/src/runtime.rs:183-189`, written by `record_verification` (`crates/router-runtime/src/lib.rs:1721`) through `verify` (`crates/agent-router-cli/src/execution.rs:1042-1055`). In `router-eval`: `VerificationOutcomes.functional` (`crates/router-eval/src/replay.rs:31-35`, `RecordedPhase.outcomes` at `:79`) | The automatic feedback record keeps `validation` and `objective` at `Unknown` (`crates/agent-router-cli/src/execution_feedback.rs:128-129`), so the bench reads the runtime journal. Outcomes still unknown are counted separately, never as failures |
| Q2. Escalation rate | `FeedbackMeasurements.escalations` (`crates/router-types/src/feedback.rs:102-108`) | Hard-coded `0` (`crates/agent-router-cli/src/execution_feedback.rs:138`). `UNKNOWN` until ADR 0019 batch 4 lands. The real count is 0 or 1 per chain: a run derived from an escalation or a generated plan cannot be escalated again (`crates/agent-router-cli/src/escalation.rs:135`, `:157-162`) |
| Q3. Human rework rate | `HumanVerdict.correction_required` (`crates/router-types/src/feedback.rs:117`), entered by hand | `UNKNOWN` without review. Bench: blind review, arm masked, on a subsample |
| Q4. Quality beyond tests (secondary) | `HumanVerdict.quality`, 0 to 100 (`crates/router-types/src/feedback.rs:112-119`) | Subsample only. Weak tests overstate Q1 (inferred) |

### Speed

| Metric | Source | Remaining `UNKNOWN` |
| --- | --- | --- |
| V1. Wall-clock time to the accepted result | Stamped by the bench runner on a monotonic clock, from the start of the first phase to `integration.applied`, which follows the `Passed` verification | `RuntimeEvent` has no timestamp (`crates/router-types/src/runtime.rs:103-111`). ADR 0019 batch 3 plans timestamped late facts and a run start time. Human review time: `UNKNOWN` |
| V2. Machine time | `TransportOutcome.elapsed_ms` (`crates/router-types/src/execution.rs:176`), copied to `latency_ms` (`crates/agent-router-cli/src/execution_feedback.rs:135`) | Acceptance script time: measured by the runner |
| V3. Attempts | `FeedbackMeasurements.attempts`; default ceiling `max_attempts: 2` (`crates/router-runtime/src/lib.rs:73`) | None |
| V4. Delay between phases | Stamped by the runner: end of one phase, start of the next | Absent from the product. Used to interpret C4 and C5, and shortened by the batch approval (see [approval](#approval-of-the-bench)) |

### Cost

| Metric | Source | Remaining `UNKNOWN` |
| --- | --- | --- |
| C1. Tokens per task, per phase | Codex `turn.completed` is recognized by the decoders (`crates/router-adapter-api/src/execution.rs:804`, `crates/router-runtime/src/artifacts.rs:353`) without reading `usage`. Its usage fields and the Claude `result` usage fields are assumptions to fix by fixture | `observed_cost_microusd` and `observed_tokens` are `None` (`crates/router-runtime/src/lib.rs:1309-1310`). ADR 0019 batch 2 plans the capture with `host_reported` provenance |
| C2. Cost per accepted task | Cost of every run and phase, failures included, divided by successes: `cost_microusd_per_functional_success` (`crates/router-eval/src/metrics.rs:144`, `docs/evaluation.md:137`); prices applied at report time by `price_usage` (`crates/router-eval/src/replay.rs:393`) | None once C1 is captured |
| C3. Credits per accepted task (Codex) | Tokens times the Codex credit rate card (see [FinOps](#finops-analysis)) | That plan usage draws on the same credits is an assumption |
| C4. Cached share, per arm and phase | `cached_input_tokens / input_tokens` (`TokenUsage`, `crates/router-eval/src/replay.rs:66-71`) | Claude cache semantics to fix by fixture |
| C5. Input overhead of phase splitting | Input cost per accepted task of B minus E, per provider, with a 95% interval. Full and cached input prices are in `TokenPrice` (`crates/router-eval/src/replay.rs:48-52`) | Depends on C1 and C4 |
| C6. Reasoning share and cost, per arm | `reasoning_output_tokens / output_tokens` (Codex name, assumption) | `TokenUsage` has no such field (`crates/router-eval/src/replay.rs:66-71`). `UNKNOWN` until P2 |

Pitfalls to lock by fixture before any costing:

- Claude cache. `price_usage` refuses `cached > input`
  (`crates/router-eval/src/replay.rs:407-408`): it assumes input includes the
  cache. Whether Claude's `input_tokens` excludes cache reads and writes is an
  assumption; the API documentation was not reread. Without a correct mapping,
  Claude costs are wrong.
- `TokenPrice` has no cache write price (`crates/router-eval/src/replay.rs:48-52`).
- Whether Codex `output_tokens` includes reasoning tokens is `UNKNOWN`: risk of
  double counting or omission.
- A host-reported total cost is a `host_reported` cross-check, never the cost
  source (decision).
- The ratio between an uncached reread and a cache hit comes from the full and
  cached input prices of the versioned `PricingTable`, not from vendor claims.

## Arms

Codex and Claude are both in v1, and routing is per provider first (decisions).
For each provider `p`, the arms A(p), B(p), G(p), C(p) and E(p) run on the host
of `p`; D(p) is secondary. The pool of A(p) is limited to the B, G and C
configurations of `p`, so the oracle, the random mix and the static rule stay
computable from the fixed-arm runs of the same provider. `A_codex` and
`A_claude` below mean A(Codex) and A(Claude).

| Arm | Model | Effort | Phases | Launch |
| --- | --- | --- | --- | --- |
| A(p). Router | Chosen per phase among B(p), G(p) and C(p) | Chosen | Separate processes | Full pipeline on the host of `p` |
| B(p). Frontier, high effort | Frontier of `p` | `high` | Separate processes | Pipeline, single candidate |
| G(p). Frontier, medium effort | Frontier of `p` | `medium` | Separate processes | Pipeline, single candidate |
| C(p). Cheaper, medium effort | Cheaper model of `p` | `medium` | Separate processes | Pipeline, single candidate |
| E(p). Single model, one session | Frontier of `p` | `high` | One session | Host CLI directly |
| D(p). Host default (secondary) | Host default | Host default | One session | Host CLI directly, if budget allows |

Ten main arms, twelve with D. The model identifiers are fixed in the manifest,
not here.

| Fact | Marker | Source |
| --- | --- | --- |
| Fixed arms need no product code: a context with one candidate is routed by `prepare` | code | `crates/agent-router-cli/src/preparation.rs:136-149`; `PhaseRouteRequest.candidates` at `crates/router-types/src/phases.rs:115-127` |
| Adapters force `--model` and effort, one process per phase, so E and D cannot use the runner | code | `crates/adapter-codex/src/execution.rs:37-40`, `crates/adapter-claude/src/execution.rs:45-48` |
| Claude maps `medium` and `high` to its native effort flag | code | `crates/adapter-claude/src/execution.rs:24-30` |
| A routing context holds one host observation | code | `crates/router-policy/src/phases.rs:69` |
| A candidate whose model the snapshot host does not support is rejected | code | `crates/router-policy/src/phases.rs:468-469` |
| The execution request takes its host from the snapshot | code | `crates/router-prepare/src/lib.rs:444` |
| `PhaseCandidate` has no host field | code | `crates/router-types/src/phases.rs:94-111` |
| A replay input carries one snapshot | code | `crates/router-eval/src/replay.rs:104` |

Consequence (code, inferred): on `e4c5329`, one routing decision cannot choose a
Claude candidate and a Codex candidate from the same pool. v1 therefore routes
per provider (decision). A cross-host candidate pool is a separate product work
item, for a future ADR 0020. Composing two single-host routings in the runner
would not be the router's decision. Every analysis in which the router moves
work between providers is v2, blocked by cross-host routing (see
[v2](#v2-blocked-by-cross-host-routing)).

E and D receive the same statement, worktree and acceptance. The statement of
E(p) is the frozen, hashed concatenation of the phase prompts prepared for
B(p), so that only session continuity differs.

### Contrasts

Each contrast varies one factor (inferred):

| Contrast | Isolated factor | Question |
| --- | --- | --- |
| B(p) vs E(p) | Separate phases or one session, model and effort fixed | What does losing the cache between phases cost? |
| B(p) vs G(p) | Effort, model fixed | Does reasoning invert the cost ranking? |
| G(p) vs C(p) | Model, effort fixed | Effect of model choice alone |
| A(p) vs B(p), G(p), C(p) | Per-phase and per-task choice within `p` | Does routing bring anything? |
| A(p) vs D(p) | Whole product vs status quo | Mixes routing, prompt assembly, skills and sandbox: not attributable to routing alone |

Comparison between providers is allowed only as a descriptive estimate without
threshold, on the same tasks: for example cost per accepted task of `A_codex`
vs `A_claude`, and of B(Codex) vs B(Claude) (E4). It says nothing about the
value of the router.

A "cheaper, high effort" arm would complete a 2 x 2 design (model x effort
interaction). Optional.

### Controls computed without extra runs

Valid because the pool of A(p) is limited to fixed-arm configurations of `p`
(inferred).
At phase granularity this assumes a phase's cost and outcome do not depend on
which configuration ran the previous phase; the report states this
approximation.

- Oracle: per task and provider, the cheapest accepted fixed arm of `p`; ceiling of the possible
  gain. Functional and cost regret already exist in `tradeoffs.rs`.
- Random mix: seeded draw following the observed distribution of A(p)'s
  choices over the three configurations of `p`. Separates routing intelligence from a mixing
  effect. Precedent: in the LiteLLM benchmark the rule-based router did no better
  than the random control (vendor source, same URL as above).
- Static rule, per provider: risk `High` or `Critical` to B(p), otherwise C(p)
  (`RiskLevel`, `crates/router-types/src/task.rs:9-14`). If A does not beat
  this one-line rule, routing adds nothing over it.

Why these baselines: B is the cautious configuration, C the economical one, G
separates effort from model, E measures the cost of splitting, D is the current
habit. A router adds value only if it sits above the B to C line on the
quality/cost frontier and the B vs E overhead does not cancel the gain.

Why a counterfactual replay is not enough (inferred): tokens depend on the model
(verbosity, reasoning, number of turns and tool calls), on the trajectory (a
failing model loops, retries, escalates) and on cache state, and success itself
is unknown in the counterfactual. `router-eval` already blocks this case
(`crates/router-eval/src/replay.rs:339-345`). Still legitimate: repricing the
observed tokens of the same model with another price grid.

## Corpus

- Source: merged commits or pull requests in the maintainer's repositories that
  modify tests. Task: parent commit plus a frozen statement. Acceptance: the
  target commit's tests, hidden from the agent, plus the repository check
  command. Statement: issue text when it exists, otherwise written without the
  solution. Two independent reviewers check for solution leaks and feasibility
  before any run; the maintainer arbitrates.
- Selection by a frozen rule: the N most recent eligible commits per repository,
  by date, with no filter on results. Exclusions are kept with their reason.
- Leak prevention: worktree at the parent commit, without remote or future refs,
  without network in the sandbox. Otherwise the agent can read the solution in
  history (inferred).
- Multi-phase: each task goes through plan, execute and verify for A, B, G and
  C; otherwise the cross-phase cache loss does not exist in the bench.
- Strata: `TaskKind` (`crates/router-types/src/task.rs:18-25`: explore,
  implement, review, architecture, security, migration), grouped in the manifest,
  crossed with low/medium vs high/critical risk. Quotas per stratum. Each task is
  a block that receives every arm, in a seeded random order.

Minimum size. Normal approximations, power 80%, one-sided tests, recomputed
with Python's `statistics.NormalDist`:

| Test | Unknown parameter | Tasks, alpha 5% (single test) | Tasks, alpha 1.25% (first Holm step) |
| --- | --- | --- | --- |
| Success, 5 point distance between null and assumed truth | Discordance between arms 10%, 20%, 30% | 248, 495, 742 | not computed |
| Success, 10 point distance: H1 non-inferiority at -10 points, H2 superiority at +10 points | Same discordance | 62, 124, 186 | 96, 191, 286 |
| Cost ratio 0.80 against a null of 1.0, per-task paired log ratio | Standard deviation of the log ratio 0.5, 0.8, 1.0 | 32, 80, 125 | 48, 123, 191 |

Alpha 1.25% is the first Holm step for the four adoption hypotheses below, and
N uses it. Two approximations apply. The success rows use the discordance
formula, which depends only on the distance between the null and the assumed
true difference, so H1's non-inferiority and H2's superiority get the same
numbers. The cost row sizes a per-task log ratio of costs, whereas H1 tests a
ratio of sums over all tasks with failed runs in the numerator; a per-task cost
per accepted task is undefined when the task fails. The cost row therefore only
approximates H1.

Rule (decision for the 10-task pilot and N): a pilot of 10 tasks estimates discordance and standard
deviation, with 3 repetitions per arm (300 task-arm runs for ten arms). Pilot
tasks are excluded from the main bench. N is fixed by the formula, at the
Holm-adjusted alpha, before any main run. A 5 point margin is out of reach for a
personal corpus (inferred). A stratum with fewer than 10 tasks is described,
never concluded.

Degenerate control: B, G and C are constant routes. If A(p) picks the
same configuration for at least 90% of phases, the verdict for `p` is "corpus
not discriminating": the bench measures nothing about routing on that provider.

## Pre-registration

Frozen before the first main run, in a dated and committed SHA-256 manifest:
task commits, statements (including E's), acceptance scripts; catalog, policy
and the pools of A(p); `PricingTable` and its variants, the credit rate card; host CLI
versions for Codex and Claude; `agent-router` binary hash; analysis script;
seed; the delay between phases imposed by the runner; the budget cap per
provider. The concept exists in `PromotionCriteria`
(`registered_at_unix_seconds`, `registration_evidence_sha256`,
`crates/router-eval/src/promotion.rs:66-78`) and is reused for arms.

### Adoption hypotheses

Four hypotheses, H1(p) and H2(p) for p in {codex, claude}, one Holm family at
a familywise alpha of 5% (decision). Notation: R(X, Y) is the cost per accepted
task of X divided by that of Y, each a ratio of sums over the bench tasks;
D(X, Y) is the success rate of X minus that of Y.

- H1(p), economy, A(p) vs B(p). Point conditions: R(A(p), B(p)) at most 0.80.
  Tests: R(A(p), B(p)) below 1.0 (null: at least 1.0), and D(A(p), B(p)) above
  -10 points (null: at most -10 points).
- H2(p), quality, A(p) vs C(p). Point conditions: D(A(p), C(p)) at least 10
  points, and R(A(p), C(p)) at most 2.0. Test: D(A(p), C(p)) above 0 (null: at
  most 0). The cost guardrail is read on the point estimate of the ratio of
  sums over the whole bench, with no interval.

Decision rule. Each test gets a one-sided bootstrap p-value: the share of
resamples whose statistic falls on the null side. A hypothesis with two tests
takes the larger of the two p-values (intersection-union, valid without further
adjustment). Holm is applied to the four hypothesis p-values at 5%. A
hypothesis is adopted only when its point conditions hold and Holm rejects its
null. An economy or quality claim holds for provider `p` only when the
corresponding hypothesis is adopted. Reported intervals are two-sided 95%
bootstrap percentiles; they are descriptive and never decide.

### Estimates published without threshold

Published whatever the result:

- E1, cache: C4 and C5 on B(p) vs E(p), per provider;
- E2, effort: cost per accepted task, success and C6 on B(p) vs G(p);
- E3, model: the same on G(p) vs C(p);
- E4, providers, descriptive: on the same tasks, cost per accepted task,
  success and V1 of `A_codex` vs `A_claude` and of B(Codex) vs B(Claude). It
  says nothing about the value of the router.

Secondary, descriptive: V1 as median and p95; each A(p) vs static rule, random mix and
oracle. Beating the static rule and the random mix is required before speaking
of routing intelligence.

Bootstrap: 10,000 resamples, fixed seed.

- Clusters: tasks from the same pull request or feature form a cluster; every
  arm of a task stays with it.
- Strata: each cluster has one attachment stratum, the stratum of the majority
  of its tasks, ties broken by the first task in manifest order. A cluster
  spanning more than one stratum is resampled only within its attachment
  stratum.
- Resampling: within each stratum, clusters are drawn with replacement, as many
  as observed.
- Statistic: recomputed on each resample from the drawn tasks, paired across
  arms. A cost per accepted task is the sum of the arm's costs over the drawn
  tasks, failed runs included, divided by its number of accepted tasks; never a
  mean of per-task ratios.
- Zero denominator: a resample in which an arm of the tested pair has no
  accepted task counts on the null side, and the number of such resamples is
  reported. With no accepted task in the observed data, the hypothesis is not
  adopted.

### Invalid runs

A run is invalid, kept and marked when:

- the host CLI or binary version changes during the bench;
- the model announced in the stream differs from the requested model;
- the stream of an E run shows more than one session;
- the worktree is not clean at start;
- an infrastructure failure occurs (`SpawnFailed`, 429, provider 5xx, network
  outage): one retry;
- the machine is loaded: quality kept, times invalidated.

More than 10% invalid runs in an arm invalidates that arm. A model that finishes
with a wrong result is a failure, not an invalid run. A task failed by every arm
stays in the denominator.

Null result: an interval overlapping a threshold gives "inconclusive"; a worse
router gives "negative". The report format is the same in every case and is
published next to this protocol with the invalid runs and E1 to E4.

## Approval of the bench

Native execution requires an approval digest per run (`run --approve`,
`crates/agent-router-cli/src/command.rs:149-151`: "without it nothing
executes"), and the coordinator approves step by step (`docs/coordinator.md:3`).

Decision: a sealed manifest, approved per batch, lists every run digest. The
runner refuses any digest absent from it.

What this delegates to the runner:

- the moment of approval: the maintainer approves a list, not each run as it
  comes;
- matching each prepared definition to its manifest entry and passing the
  listed digest;
- ordering and timing between phases, including the imposed delay;
- stopping and marking runs invalid under the rules above.

What stays with the product: each digest still binds its exact definition, so a
listed digest cannot approve a different definition.

Open point (inferred from `docs/coordinator.md:3-5`): an Execute step becomes
ready only after the parent or user verifies the model-produced plan, so the
digest of a later phase may not be computable before the earlier phase runs. A
batch can then list only definitions whose digests are known, and the plan
verification before execution is a further delegation to settle in P5.

Consequence for speed and cost: phases follow each other faster than in real use,
where a human approval can exceed the provider's cache lifetime. The bench then
overstates cache hits for A, B, G and C against E. Countermeasures: the imposed
delay is published (V4), and a sensitivity analysis charges the cross-phase
prefix at the full input price.

## FinOps analysis

Price facts used by this protocol, each with a public source. No other price
fact is used.

| Fact | Status | Date | Source |
| --- | --- | --- | --- |
| Codex bills per token since 2026-04-02: credits per million input, cached input and output tokens | Official | Consulted 2026-09-26 | https://help.openai.com/en/articles/20001106-codex-rate-card |
| Promotional GPT-5.6 Sol rate on purchased credits, at least until 2026-11-21 | Official | Consulted 2026-09-26 | https://help.openai.com/en/articles/20001106-codex-rate-card |
| Pro 200 allocation from 20x to 10x Plus on 2026-10-30, from an OpenAI email quoted by the press; the help page publishes no allocation figure | Press | 2026-09-29 | https://www.engadget.com/2272106/openai-adds-dollar500-pro-subscription-nerfs-its-existing-dollar200-tier/ and https://thenextweb.com/news/openai-devday-pro-200-usage-cut-pro-500-plan |
| Claude plans include Claude Code with 5 hour and weekly limits, usage credits at API rates beyond | Official | Consulted 2026-09-30 | https://claude.com/pricing |
| Claude Code weekly limits at +25% over the pre-promotion base since 2026-09-14 | Official | Consulted 2026-09-30 | https://support.claude.com/en/articles/15910845-claude-code-may-august-2026-weekly-limits-promotion |

"Codex costs will rise" therefore translates into two levers, never into an
established fact. The 20x to 10x change stays a scenario until an official page
confirms it.

### Lever 1: price per token

- `PricingTable` variants: Codex x1.25, x1.5, x2 as stress tests; an "end of
  promotion" scenario replacing the promotional rate by the standard one.
- Static effect: same choices, new prices. Legitimate, since observed tokens do
  not change.
- Dynamic effect: routing of A(p) replayed with the repriced catalog
  (`ReplayInput.catalog`, `crates/router-eval/src/replay.rs:105`), counting the
  phases whose choice changes. Since the pool of A(p) only contains fixed-arm
  configurations of `p`, the new choice has always been observed and needs no new run
  (inferred, with the phase-granularity approximation above).
- Tipping point: the multiplier at which the ranking of arms inverts.
- Within one provider (inferred): a uniform increase multiplies every arm of
  `p` by the same factor and leaves ratios unchanged. Only a non-uniform change
  (frontier only, end of promotion on one model) moves A(p)'s choices.
- Between providers, v1: the Codex arms repriced under each multiplier are shown
  next to the Claude arms, as the descriptive estimate E4. This is a price
  comparison between providers, not a router effect.
- Between providers, v2: whether the router itself moves work from Codex to
  Claude when Codex gets more expensive (see
  [v2](#v2-blocked-by-cross-host-routing)).

### Lever 2: plan envelope

- Unit: credits per accepted task (C3), from tokens and the rate card. That plan
  usage draws on the same credits is an assumption.
- Envelope size: `UNKNOWN`. Result published as accepted tasks per 1,000
  credits, per Codex arm.
- 20x to 10x scenario: envelope halved. Per arm, the share of a window's tasks
  that exceed the envelope.
- Overrun: purchased credits, or waiting until the window resets. The wait adds
  to V1 in this scenario.
- Threshold effect: a router that lowers consumption per task can bring volume
  back under the envelope. The gain is nonlinear and invisible in a cost ratio
  (inferred).
- Claude envelope: the published limits have no credit unit in the sources
  above, so Claude envelope pressure stays `UNKNOWN`; its cost is computed at API
  rates only.

### v2, blocked by cross-host routing

These need a candidate pool spanning hosts, a separate product work item for a
future ADR 0020 (decision). They are not part of v1:

- a single arm A routing over the B, G and C configurations of both providers;
- the analysis "a Codex price increase moves A's choices to Claude": per Codex
  multiplier, the share of phases moving to Claude, the resulting cost per
  accepted task and the change in success estimated from the Claude fixed arms,
  under the assumption that Claude configurations perform on moved phases as in
  their fixed arms;
- the share of A's phases routed to each provider;
- the cross-provider part of P7. A replay input with one snapshot
  (`crates/router-eval/src/replay.rs:104`) cannot compute it today.

### Cost per merged pull request

Definition: cost of every run attributable to the pull request, abandoned ones
included, divided by merged pull requests. The bench has no pull requests, so the
cost per accepted task is the proxy. In real use: `UNKNOWN`, for lack of a run to
commit to pull request link.

### What a router can save mechanically

| Lever | Status |
| --- | --- |
| Model mix, routine work on the cheaper model | Measured: G(p) vs C(p), and A(p)'s choice shares times the observed delta |
| Lower effort | Measured: B(p) vs G(p), with C6 once reasoning is captured |
| Cross-phase cache loss, as a cost | Measured: B(p) vs E(p) (E1). The main countermechanism |
| Cheaper first, then escalate | Parameters measured. Profitable when P(failure of the cheaper) < 1 - c_cheaper / c_frontier, including the cost of detecting the failure (inferred) |
| Moving load to the other provider when one gets more expensive | v2, blocked by cross-host routing |
| Static prefix served from cache across separate processes | Assumed: depends on provider, model and cache lifetime. E1 settles the net effect |
| Tasks avoided by abstention | Assumed |

Cache nuance (inferred): splitting loses the conversation history, but each
phase starts from a prepared prompt and may read less context than a session
that accumulates. The sign of the net effect is unknown before E1.

## Implementation batches

ADR 0019 batches are noted "ADR n"; bench batches P1 to P8. No duration.

| Batch | Content | Acceptance tests | Size |
| --- | --- | --- | --- |
| ADR 2 | Usage capture from the stream: Codex `turn.completed`, Claude `result`; `crates/router-runtime/src/lib.rs:1309-1310`, `crates/agent-router-cli/src/execution_feedback.rs:134-136` | Fixture with usage gives `Some`; without, `None`, never 0; Claude cache mapping; Codex reasoning never counted twice; no assistant text kept | M |
| ADR 3 | Timestamped late facts and run start time | Prerequisite of V1 outside the runner | L |
| ADR 4 | Route facts and the real escalation count, 0 or 1 | Q2 stops being `UNKNOWN` | S |
| ADR 5 | Export of a run as `ReplayInput` | Label never taken from transport alone | M |
| v1 limit | Cross-host candidate pool (router-policy, router-prepare, replay snapshot): separate product work item, future ADR 0020, not part of v1 | Not a v1 prerequisite; it gates the v2 analyses only | UNKNOWN |
| P1 | This protocol and the manifest schema | The manifest seals; any hash change is refused | S |
| P2 | `TokenUsage` and `TokenPrice` v2 in `router-eval`: reasoning, cache write, `unit` `usd` or `credits` (`crates/router-eval/src/replay.rs:48-71`, `price_usage`) | Each component priced separately; the same usage priced in both units; v1 records still readable; numeric overflow refused | S |
| P3 | Per-phase observation attached to the task and aggregated, in the ADR 5 export | Sum of phases equals the task total; a phase without usage makes the total `UNKNOWN`, never 0 | M |
| P4 | Arm distinct from `PhasePolicy`: `TradeoffPolicy` will accept a hashed fixed candidate. Today it holds only `policy_id` and `policy_sha256` (`crates/router-eval/src/tradeoffs.rs:18-21`), checked against the serialized policy at `:216` | Fixed-arm cohort evaluated; substituted hash refused | M |
| P5 | Bench runner outside product crates (`scripts/bench/`; `xtask` is reserved for the harness): seeded order, fresh worktree per run, E and D through each host CLI, single-candidate contexts for B, G and C, V1 and V4 stamps, imposed delay, model purity check, single-session check for E, invalid runs marked, sentinel task, manifest digest refusal, both host CLIs pinned | Dry run with the fake host (`crates/agent-router-cli/tests/support/native_fixture.rs`); wrong model or two sessions in E gives an invalid run; an unlisted digest is refused | M |
| P6 | Paired analysis in `router-eval`: cluster bootstrap and one-sided p-values as defined above, deltas, Holm over four hypotheses, strata, E1 to E4, oracle, random mix and static rule; `eval bench-report` command | Fixture with a known analytic answer; byte-identical output; `UNKNOWN` is never a success | M |
| P7 | FinOps: price variants, end of promotion, per-provider dynamic effect, descriptive provider comparison, envelope model (tasks per 1,000 credits, halving, overrun). Cross-provider shift: v2 | Fixture where the expected overrun share is computed by hand | M |
| P8 | Human-readable and JSON report, null result template | Identical output on fixture | S |
| Later | Cost per merged pull request: run to commit to pull request link, forge reading | Outside the bench | M |

Dependencies: ADR 2 and P2 before P3; ADR 5 before P3; P3 and P4 before P6; P6
before P7 and P8; P5 in parallel from P1. The pilot needs ADR 2, P2, P3 and P5.
v1 does not need the cross-host pool; the v2 analyses do.

## Risks of a wrong conclusion

- Contamination between arms:
  - provider cache: arms sharing a model benefit from the previous run's cache.
    Seeded order, and a gap between two runs of one task on one model longer
    than the cache lifetime. The lifetime is `UNKNOWN` and may differ between
    providers; a conservative value per provider is pre-registered;
  - user configuration (global instruction files, `AGENTS.md`, hooks): the
    adapters do not isolate it in argv, and sandbox isolation is not verified
    here. Identical configuration hash for every arm of a provider;
  - git leak: worktree without future refs;
  - different instruction in E: statement built from B's phase prompts, frozen.
- Unrealistic delay between phases: the batch approval shortens it (see
  [approval](#approval-of-the-bench)). Imposed delay published, plus a full-price
  prefix sensitivity.
- Host variance, stronger with two providers: two model families, two tool loops
  and two rate-limit regimes widen the spread of cost and success. Pilot with
  repetitions, cluster bootstrap, conclusions on intervals only.
- Two host CLIs to pin: a version change on either invalidates that host's runs
  from the change on, including that provider's A runs. Versions and the
  announced model are logged; a sentinel task runs at the start, middle and end.
- Provider-side model change: arms of one task within a short window; invalidate
  on a version change.
- Bench cost: per provider, cost is about (pilot cost per task-arm run on that
  provider) x N x (arms on that provider: five, plus D if run) x (1 + invalid rate). Pre-registered cap per provider; if reached, the
  analysis covers only tasks complete in every arm. Amount `UNKNOWN` before the
  pilot.
- Overfitting: catalog and policy frozen before the bench, no tuning on test
  tasks.
- Experimenter bias: frozen analysis script, blind human review.
- Weak tests: human review on a subsample (Q4).
- Multiple comparisons: four adoption hypotheses under Holm; E1 to E4 published
  as estimates without threshold.
- Rate limits: a 429 invalidates the run; the infrastructure-failure rule under
  Invalid runs allows one retry.
- Price rumor: the 20x to 10x change remains a scenario while no official page
  confirms it.
- Cross-host gap: v1 shows only per-provider routing. Any claim that the
  router saves money by moving work between providers stays unmeasured until
  v2; the descriptive provider comparison (E4) must not be read as one.

## Not reread for this protocol

- Usage field names in Codex and Claude streams, and the provider APIs' token
  and cache semantics.
- Provider cache behavior across processes and its lifetime.
- Whether plan usage is counted in Codex credits.
- The runtime escalation store.
