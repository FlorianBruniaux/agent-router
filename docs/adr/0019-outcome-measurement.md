---
adr: "0019"
title: Outcome measurement
status: accepted
created: 2026-09-30
last_modified: 2026-09-30
scope: "Implementation planned; this ADR changes no code."
amended_by: []
supersedes: []
superseded_by: []
related: ["0009"]
---

# ADR 0019: Outcome measurement

## Context

The tuning loop exists: consented feedback, statistics, drift, bounded
proposal, replay evaluation, shadow and canary trials, then signed activation
([local feedback](../feedback.md)). Runs executed by Agent Router do not feed it.

- The automatic record written after a run sets `validation` and `objective` to
  `Unknown`, `cost_microusd` and `tokens` to `None`, and `escalations` to `0`
  (`crates/agent-router-cli/src/execution_feedback.rs:127-139`). Its comment
  states the reason: "A successful process says nothing about validation or the
  objective."
- `propose_policy` counts only verified objectives from automated events
  (`crates/router-feedback/src/learning.rs:160-163`,
  `crates/router-feedback/src/policy.rs:201`). The test
  `transport_success_and_unknown_prices_never_become_objective_success_or_zero_cost`
  (`crates/router-feedback/tests/feedback.rs:153`) shows an `Unknown` objective
  refused by that gate; `cargo test -p router-feedback --test feedback --locked
  transport_success_and_unknown_prices` gives `1 passed`. No proposal can pass
  from real runs alone.
- The automated verdict is immutable per `(run_hash, origin)`
  (`crates/router-feedback/src/store.rs:175-195`) and is written at completion,
  by a single run (`crates/agent-router-cli/src/execution.rs:953`) and by a
  coordinated wave (`crates/agent-router-cli/src/coordinator.rs:626`), before
  `verify` and before integration. A later automated verdict for the same run is refused.
- The runtime journal already holds the missing signals:
  `verification.recorded` (`crates/router-runtime/src/store.rs:734`),
  `integration.applied` and `integration.discarded`
  (`crates/router-runtime/src/lib.rs:1853`, `:1873`). Nothing reads them back
  into feedback. `RuntimeEvent` carries no timestamp
  (`crates/router-types/src/runtime.rs:103-111`).
- Observed usage stays `UNKNOWN` and is not an authority for admission budgets
  ([ADR 0009](0009-conservative-session-allocations.md)). Attempt evidence sets
  `observed_cost_microusd` and `observed_tokens` to `None`
  (`crates/router-runtime/src/lib.rs:1309-1310`).
- Adapters launch hosts without a transcript: `--no-session-persistence`
  (`crates/adapter-claude/src/execution.rs:53`) and `--ephemeral`
  (`crates/adapter-codex/src/execution.rs:44`). The only host output available
  to the router is the stdout stream its supervisor already captures.
- Automatic learning remains open under R15
  (`docs/remaining-acceptance.md:352`): shadow or canary exploration only,
  logged propensity and no regression by subgroup before signed activation.

## Decision

Extend the existing feedback chain and its store, rather than adding a separate
outcome registry or observing the hosts; both alternatives are rejected below.
Every item below is planned and delivered by the batches listed at the end.

### What this measurement must answer

The data exists to tell whether the router helps deliver better, faster or
cheaper work. Three metrics consume it, each computed per accepted task, with
the definition of acceptance given below:

- quality: the accepted success rate, with runs whose outcome is still unknown
  counted separately rather than as failures;
- speed: wall-clock time from the start of the run to `integration.applied` of
  the accepted result, and the number of attempts;
- cost: `host_reported` tokens and the cost computed at report time from a
  versioned `PricingTable`, including the cost per accepted task.

None of these figures, taken alone, proves that the router brings a gain. That
claim needs a paired A/B comparison against fixed baselines, on a frozen corpus,
with pre-registered thresholds. That protocol is specified in the
[router value protocol](../bench/router-value-protocol.md), not here.

The runtime journal has no timestamp (`RuntimeEvent`,
`crates/router-types/src/runtime.rs:103-111`). The feedback store stamps each
event with `recorded_at_unix_seconds`
(`crates/router-types/src/feedback.rs:146`), and the automated verdict is
recorded at completion, not at start. Wall-clock time to acceptance therefore
requires each late fact to carry its own observation timestamp, and the run a
start time on the same clock. Until both exist, speed is limited to transport
latency and attempts.

### Recorded facts

Definition of an accepted result. A run counts as an automated success when
`integration.applied` is recorded and a recorded verification for that run has
a `Passed` objective. The feedback contract already requires `Passed`
validation and an evidence hash for a `Passed` objective
(`crates/router-feedback/src/lib.rs:163-167`). A human verdict remains a
separate signal. A transport exit code of 0 never counts as success.

`FeedbackInput` schema version 2. The planned input adds, as typed and bounded
fields only:

- usage: input, cached input, output and reasoning output tokens, each with a
  provenance of `host_reported` or `unknown`;
- route facts without text: selected candidate id, its rank, the margin in
  micros over the runner-up, the number of alternatives and whether the router
  abstained;
- the actual escalation count, replacing the hard-coded `0`. It is 0 or 1: a
  run derived from an escalation or a generated plan cannot be the source of
  another escalation (`crates/agent-router-cli/src/escalation.rs:135`,
  `:157-162`).

Host-reported usage is accepted for reports only. It never feeds admission
budgets; ADR 0009 is unchanged. Missing usage stays `unknown` with its own
count and never becomes zero.

Late facts. Verification, integration and human outcomes are appended as
separate facts, at most one per `(run_hash, kind)` with kind `verification`,
`integration` or `human`. Repeating an identical fact is idempotent; a
conflicting one is refused. No existing verdict is rewritten, so the
immutability of `(run_hash, origin)` holds.

Storage. The same `feedback.sqlite3`, migrated from `user_version` 1
(`crates/router-feedback/src/store.rs:81`) to 2 by an explicit migration. A
version 1 database opens and migrates; a future version is refused as today.
Consent, retention, export and `purge_all` cover the new facts.

Money. Cost is computed at report time from a versioned `PricingTable`
(`crates/router-eval/src/replay.rs:56`, `price_usage` at `:393`) cited by its
version in the report. It is never stored as a fact.

Tuning. A planned `feedback report` shows, per model, effort and task kind,
verified success with a Beta interval, token and latency percentiles, computed
cost, integration rate, the ratio between catalog estimate and observed usage,
and visible `UNKNOWN` counts. Any policy suggestion it prints is advisory. A
human writes the candidate, then the existing chain applies: propose, evaluate,
trial, signed apply.

Scope. Runs executed by Agent Router come first. Host-side hooks for
interactive sessions where the router only advises are deferred and require
their own ADR.

Consent. Collection stays disabled by default and opt-in through
`feedback_store`. The product defaults are unchanged: collection and learning
disabled, 30 day retention (`crates/router-types/src/feedback.rs:22-24`), at
most 365 days (`crates/router-feedback/src/lib.rs:19`), and no record without a
configured store (`crates/agent-router-cli/src/execution_feedback.rs:28-29`).
A 90 day retention is recommended for the maintainer's own workstation to reach
useful sample sizes. That figure is a configuration choice, not a measurement
and not a product default.

## Rejected options

B, a separate outcome registry (new crate, chained JSONL). It duplicates
consent, retention and purge, and adds a second source of truth beside a runtime
journal that is already chained. The privacy surface doubles for no data that
the extended feedback chain cannot carry.

C, host-side observation (host hooks or notifications, transcripts). It is
the only option that covers advisory interactive sessions. Transcripts contain
prompts and raw responses, which conflicts with the rule that no raw response
or secret value is logged. Host formats are unstable, and trust in a hook
cannot be inferred from its presence. Deferred to its own ADR. If adopted, the
starting point is a minimal hook that calls `agent-router feedback record` with
enums only, without reading a transcript.

## Why a reviewed proposal and not auto-tuning

- One user produces small samples.
- Most labels are `Unknown` today.
- Propensity is unknown (`propensity_micros: None`,
  `crates/agent-router-cli/src/execution_feedback.rs:116`), so no causal
  estimate is possible from the chosen routes alone.
- The product promises deterministic output.
- Activation already requires shadow and canary trials and an Ed25519
  signature.
- R15 lists automatic learning as remaining work under stricter conditions.

## Consequences

Invariants kept by every batch:

- deterministic output: the same retained events and pricing table produce a
  byte-identical report;
- bounded input and work: new fields have fixed types and limits, and the
  existing event caps apply;
- explicit `UNKNOWN`: absent usage, verification or integration stays unknown
  and is counted, never zero or failure by omission;
- no prompt, command output, raw host response or free text is stored; route
  facts are ids, enums and integers.

Still not measured after this decision:

- advisory interactive sessions (deferred hooks);
- human time spent reviewing or correcting a result;
- wall-clock time to acceptance, until late facts carry a timestamp and the run
  a start time, since runtime events carry none;
- a gain attributable to the router, until the paired A/B
  [router value protocol](../bench/router-value-protocol.md) runs;
- causal effect of a route, since propensity stays unknown;
- authenticated spending: `host_reported` usage is a host claim, not an invoice.

A policy change still depends on a human candidate and the signed chain. The
report makes the evidence visible; it does not decide.

## Implementation batches

1. This ADR and the operator note in [feedback](../feedback.md).
2. Usage capture from the captured stream, with `host_reported` provenance and
   no effect on admission budgets.
3. Late facts: schema v2 migration, timestamped `verification`, `integration`
   and `human` facts wired from `verify` and `artifacts apply` or `discard`,
   and a run start time for the speed metric, for runs recorded by both writers
   (`crates/agent-router-cli/src/execution.rs:953`,
   `crates/agent-router-cli/src/coordinator.rs:626`).
4. Route facts in `FeedbackContext` (rank, margin, alternatives, abstention) and
   the actual escalation count, 0 or 1, in both writers.
5. Export of a real run as a sealed `ReplayInput`, labelled only from a human or
   integration fact, never from transport alone.
6. `feedback report` with an advisory suggestion that respects the existing
   delta bounds.

Batches 2 and 4 can proceed in parallel after this ADR. Batch 5 depends on
batch 3; batch 6 depends on batches 2 and 3.
