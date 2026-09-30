# Local feedback and offline policies

`router-feedback` records redacted observations in an explicitly selected private
directory. Collection and learning are both disabled initially. Nothing in this
crate launches a model, changes a receipt, grants a permission or starts an
experiment against production traffic.

## Consent and data

`FeedbackStore::open(path)` creates `feedback.sqlite3` and a writer lock. The
directory must be private (0700); database files must be private regular files
(0600). Final symlinks, hard links and public directories are refused. SQLite
uses WAL, foreign keys, FULL synchronization, monotone schema versions and
transactions. A second writer is refused. Feedback rows and policy artifacts
cannot be updated through SQL; policy state transitions append an audit event.

`configure(FeedbackPreferences, now)` explicitly enables collection and,
separately, learning. Learning cannot be enabled without collection. Consent
revisions and observation times are attached to every event. Observations before
the latest consent revision, in the future or older than retention are refused.
The default retention is 30 days; an explicit value must be between one second
and 365 days. There are at most 10,000 retained events and 128 policy artifacts.

Inputs contain only schema/version identifiers, SHA-256 links to task, run,
immutable receipt, policy, catalog, snapshot and producer evidence, registered
asset identifiers, discrete task/risk classes, enums and bounded measurements.
Use `redacted_digest(id)` when the source task/run identifier is not already a
hash. These are correlation identifiers, not evidence that a run exists. The
caller must obtain the binding from its immutable runtime records. Hashes do
not authenticate an observation, a reviewer or a successful task.

There is no field for prompts, command output, workspaces, raw payloads or raw
comments. A caller accepting a comment must visibly use the digest-only mode:
`comment_digest(text)` produces `comment_sha256`; the text is never stored.
Digest-only comments preserve correlation and integrity, not readable commentary.
A digest is not anonymization and can reveal equality or allow guessing of
short text. Identifiers must refer to registered assets, never contain personal
text or credentials. No sensitive input is included in library error messages.

An automated verdict and a human verdict may each be recorded once per run.
Repeating the identical input is idempotent; changing it fails. Both verdicts
must retain the same route/context binding. Human corrections are explicit
`correction_required` observations, never edits to historical receipts.

## Results and statistics

Transport, command validation, objective verification and human success are
separate fields. A successful transport never updates verified objective
success. A claimed validation/objective result requires an evidence hash;
objective success additionally requires successful validation. This verifies
the contract of the claim, not the underlying evidence. Human quality uses an
explicit 0–100 scale. Perceived cost, failure class and required correction stay
separate from measured cost. Missing cost, tokens, latency and propensity remain
unknown; none become zero by omission. `measurements.tokens` is the total
declared by the caller. The automatic CLI feedback sets it only to input plus
output tokens the host declared in its own stream (`host_reported`, decoded
into `RuntimeEvidence.host_usage`); manual and sidecar feedback set it
themselves. The per-field detail is not in the feedback record.

Automated events also carry optional route facts in `context.route`: the
executed candidate id, its one-based rank and utility margin over the next
ranked candidate (both taken from the receipt's own order), the number of other
ranked candidates and the baseline abstention when the execution overrode it.
No text is stored. Events written before these facts existed omit the key and
read back unchanged. `margin_micros` can be negative, because the receipt ranks
the preferred model first even when its utility is lower. `escalations` is 1
when the run is the target of an escalation and 0 otherwise; a chain cannot
exceed 1 because an escalation source must be a deterministic or activated run.
An escalated run therefore has one attempt and one escalation, and the validator
rejects `escalations > attempts` instead of `escalations >= attempts`.

Automated events written by the CLI also carry `run_key_sha256` and
`started_at_unix_seconds`, both optional and omitted when absent, so an earlier
event keeps its bytes and hash. The run key is the digest of the run id, the
definition hash and the approval digest. It does not change with the run state
or the attempt count, which `verify` and `artifacts apply` change and which
`run_hash` covers, so `run_hash` cannot serve later. A retry issues a new
approval digest (`retry_projection`), so it starts a new key: a late fact
attaches to the latest attempt, and the earlier attempt's event keeps its own
key. The start time is read before the native process
starts; a coordinated wave shares one start time for its steps. It is refused
when later than `observed_at_unix_seconds`.

Late facts are appended after the automated verdict, keyed by the run key and
holding no free text:

- `verification`: the validation and objective outcomes of the record the runtime
  stored, and the digest of that record. It is written only once the run left
  `Verifying` (`verified` or `verification_failed`). The runtime may replace an
  inconclusive record while the run stays `verifying`, so such a record is not a
  fact, and a stored fact is never rewritten. A record that fails the contract
  (a passed objective with a validation that did not pass) is refused.
- `integration`: `applied` or `discarded`.

There is at most one fact per run key and kind. Repeating an identical fact is
idempotent; a different fact for the same key and kind fails with an integrity
error, and nothing is rewritten. Identical includes `observed_at`, the time of
recording: a repeat is idempotent only within the same second. A fact does not require a matching automated
event, which may have failed to record. The human outcome is not a fact kind: it
stays the human-origin event, once per run. Consent, the clock watermark and
retention (by observation time) apply as for events. Disabling collection,
`purge_all` and expiry delete facts, and `export` lists them under `facts`,
omitted when empty. The sidecar projection never includes a fact. The store
migrates from schema version 1 to 2 when opened, in one transaction that only
adds the fact table; events are not read or rewritten, and a database with a
newer version is refused.

`agent-router verify`, `artifacts apply`, `artifacts discard` and their
`coordinate artifacts` counterparts accept `--config <sandbox.json>`. Only its
`feedback_store` is read, but the whole file is validated by the same loader as
`run`, and an invalid configuration fails the command before the operation.
Without the option, or with collection disabled, nothing is recorded. The JSON output gains a `feedback` object, either
`{"recorded":true,"fact_hash":...}` or `{"recorded":false,"reason":...}` with
`not_requested`, `collection_disabled`, `inconclusive`, `invalid_fact` or
`consent_changed_or_storage_error`. Once the operation succeeded, a feedback
failure never fails the command. There is no retry: if the process dies between
the runtime operation and the fact write, or the write fails, the fact is lost
and nothing records it later.

Limit: no verification fact is recorded for a coordinated run. `coordinate
verify` only queues the verification, and `service`, which applies it, does not
receive the sandbox configuration.

`statistics(now)` is available only with learning consent. It rebuilds a
versioned, content-hashed report from retained immutable events:

- Beta(1,1) competence by agent/skill, with human and automated observations
  separate; unknown objective outcomes update neither alpha nor beta.
- Exact count, sum, sum of squares and empirical min/median/p95/max for cost,
  latency and tokens by model/effort/harness; missing values have their own count.
- Attempts, escalations, transport failures and failure classes.
- Route facts recorded and how many of them overrode an abstention (`route`,
  omitted while no event carries one).
- Human plan preference under a discrete task/risk context, quality moments and
  known versus unknown decision propensities.

These are descriptive associations. They do not establish that an asset caused
success, that a human label is authentic, that unobserved alternatives are worse,
or that a policy improvement has been measured. The statistics hash changes
when retained evidence changes. Statistics are rebuilt rather than persisted
in an additional cache.

`drift(rules, now)` compares verified objective success in two adjacent windows.
Minimum sample counts, window and decline threshold are explicit caller-supplied
experimental rules. Unknown outcomes are excluded from success/failure counts.
A threshold crossing returns a quarantine recommendation. It changes no policy.
Timestamped observations permit later decay analyses; this implementation uses
fixed windows and makes no claim to implement an online bandit.

## Reversible policy protocol

1. Explicitly `register_baseline(policy, now)` once. This installs a deterministic
   baseline. Re-registering the identical policy is idempotent; replacing it is
   refused.
2. `propose_policy(candidate, rules, now)` binds the complete candidate, baseline,
   rules and statistics hash. It requires the configured number of verified
   observations. Weight deltas are bounded; minimum utility and ambiguity
   thresholds cannot decrease. Alternative count is fixed. The patchable policy
   has no permission or constraint fields. A proposal cannot activate itself.
3. `evaluate_proposal(id, paired_traces, criteria, now)` calls `router-eval` on
   audited, non-deserializable traces. Baseline/candidate policy hashes must
   match the artifacts. Fixed held-out data, independent labels, pre-registered
   metrics, a demonstrated gain and zero hard-constraint violations are
   required. A JSON report claiming success cannot bypass this gate.
4. `record_trial` accepts, in order, shadow then bounded canary evidence. Evidence
   declares independent review, acceptance, case count, zero hard violations,
   zero subgroup regressions and ordered timestamps. Shadow has a configured
   minimum sample count; canary has an explicit maximum of at most 100 cases.
   Trial hashes and attestation fields are preserved. The library does not
   execute these trials or authenticate the content behind an evidence hash.
5. Obtain `promotion_digest(id)`. Fill `SignedPolicyApproval` with that digest,
   the exact candidate and current active artifact, action, fresh nonce, issue
   and expiry times. Sign `approval_message(approval)` with an Ed25519 key.
   Approvals last at most 24 hours. `apply_signed_action` requires an independently
   trusted public-key allowlist supplied by the caller, verifies the signature,
   checks freshness and exact current state, and refuses reused approval nonces.
6. Explicit signed `Rollback` returns to the immediate previously accepted
   deterministic policy. Signed `Quarantine` prevents reactivation and restores
   the accepted predecessor when the quarantined artifact was active.

The active artifact is a local policy pointer. Applications must deliberately
read it through `active_policy()` and continue enforcing ordinary hard
constraints, permissions, explicit overrides and organization restrictions.
Contextual preference statistics remain shadow information; no exploration or
learned asset selection enters production through this crate. No improvement
is claimed by the synthetic tests. Their reviewers, prices and signatures are
test data only.

## Export, retention and deletion

`export(now)` returns a deterministic, versioned feedback snapshot including
consent and a full-content hash. `policies()` and `policy(id)` export policy
artifacts and their separate audit trail without receipt/prompt bodies.

Expired feedback is purged on reads/writes or explicitly with `purge_expired`.
Disabling learning deletes candidate policies and their learning history and
restores the explicitly registered baseline. Disabling collection also removes
retained feedback. `purge_all(now)` removes feedback, all policy artifacts and
history, resets consent/settings and leaves both consents disabled. Only a
timestamp watermark survives; it prevents replaying an older clock after
deletion. It contains no task, run, identity, observation or policy data. Fresh
consent is required and observations predating that consent remain refused.
Deletion uses SQLite secure deletion, checkpoint/truncate and vacuum to remove
deleted records from this database and its WAL. It cannot erase caller-created
exports, filesystem snapshots, external backups or copies already held in
another process's memory. Such copies remain the caller's responsibility.

Clock values are explicit for deterministic tests. Production callers must use
a trusted wall clock. On-disk hashes detect accidental modification; they do not
protect against a local owner replacing the entire database, keys or program.

## Planned: outcome measurement for executed runs

[ADR 0019](adr/0019-outcome-measurement.md) decides how runs executed by Agent
Router will feed this store. Batch 2 (host-declared tokens in
`measurements.tokens`), batch 3 (late facts and the run start time), batch 4
(route facts and the escalation count) and batch 6 (`feedback report`, see
[the report](#feedback-report)) are described in this file. The automatic record
still keeps validation and objective `Unknown`, and the proposal gate does not
read the facts, so real runs alone cannot pass it. Only the report reads them.

What will change for an operator:

- A run will count as an automated success only when its integration is applied
  and a recorded verification has a `Passed` objective. A transport exit code of
  0 will still never count.
- The per-field token detail (input, cached input, cache writes, output,
  reasoning), from `RuntimeEvidence.host_usage`, is stored in
  `measurements.usage` with its `host_reported` provenance, for reports only.
  Admission budgets do not use it, and the sidecar export omits it.
- The data will feed three metrics per accepted task: accepted success rate,
  wall-clock time from run start to `integration.applied` of the accepted
  result and attempts, and tokens with computed cost. Late facts carry a
  timestamp for the time metric. None of them alone proves a gain from the
  router; that needs the paired A/B
  [router value protocol](bench/router-value-protocol.md) against fixed
  baselines.
- `feedback report` shows accepted rate, usage, computed cost and `UNKNOWN`
  counts. Activating a policy still goes through propose, evaluate, trial and
  signed apply.

Collection stays opt-in through `feedback_store`, and the product defaults do
not change. A longer retention, such as 90 days on a single workstation, is an
explicit configuration choice made in the preferences passed to `feedback
configure`.

## Feedback report

```sh
agent-router feedback --store <dir> --json report [--pricing <file>] [--now <unix>]
```

The report needs collection consent and writes nothing. Like the other readers
it may expire retained rows and advance the clock watermark. The same retained
events, facts, pricing file and `--now` give byte-identical output: lists are
sorted and every figure is an integer or a fixed-precision value in micros.

Runs are grouped by `(model, effort, task_kind)`. Automated events join late
facts by `run_key_sha256`:

- accepted: an `integration` fact `applied` and a `verification` fact with
  objective `passed`;
- rejected: an `integration` fact `discarded`, or a `verification` fact with
  objective `failed`. A rejection signal wins over an acceptance signal;
- unknown: everything else, including a run with only one of the two facts. It
  is counted and never treated as a failure;
- `unlinked`: automated events without a run key, which no fact can refer to.
  They are also counted as unknown.

Human-origin events are counted apart, by their `human.success`, and never
change an automated outcome. Facts that match no retained automated event are
counted as `orphan_facts`.

Per group the report gives:

- `accepted_rate`: accepted over accepted plus rejected, with a 95% equal-tailed
  Jeffreys interval (Beta prior 1/2, 1/2) in micros. Unknown runs are excluded.
  It is null when no run is decided;
- p50 and p90 by nearest rank, with `samples` and `missing`, for `tokens`,
  `latency_ms`, `attempts` and `time_to_acceptance_seconds`. The last one covers
  accepted runs only: the applied fact's `observed_at` minus the run's
  `started_at_unix_seconds`. A run without a start time is counted in `missing`;
- `integration`: applied, discarded, unknown, and `rate_micros` as applied over
  applied plus discarded;
- `cost`, only with `--pricing`, a `PricingTable` file whose `version` and
  `source` the report cites. Each run is priced from its stored usage split, at
  the uncached input, cached input and output rates, rounded up to a
  micro-dollar per run like `router_eval::price_usage`. A run with no stored
  usage, an undeclared input, cached input or output count, or a model missing
  from the table is `cost_unknown`, never zero. `total_microusd` sums priced runs
  only. `per_accepted_microusd` divides the total over all runs by accepted
  tasks, and is null while any run of the group is unpriced. Cache writes are
  priced as input, since the table has no cache write price, and reasoning tokens
  as output. Without `--pricing` every cost is null with reason
  `no_pricing_table`.

The ratio between catalog estimate and observed usage that ADR 0019 lists is not
computed: no catalog estimate is stored in feedback. It is reported as
`catalog_estimate_ratio` with a null value and a reason, and no source is
invented for it.

`advisory` is `{"advisory":null,"reason":"learning_disabled"}` unless learning is
enabled. With learning enabled the report carries `"advisory":true` and a null
`suggestion`. `propose_policy` accepts only score-weight changes within
`maximum_weight_delta` and never a lower threshold. A sound weight suggestion
needs the active policy, the catalog and the propensity of each route, and the
report has none of them: grouped outcomes cannot say which weight caused a
route. A missing suggestion is the honest output; a human still writes any
candidate.

What the report does not prove:

- A gain attributable to the router. That needs the paired A/B
  [router value protocol](bench/router-value-protocol.md); these figures are
  associations from routes whose propensity is unknown.
- Small samples: read the interval, not the point value.
- Spending: usage is `host_reported` and prices come from the table given, so the
  cost is neither an invoice nor a metered figure.
- Cross-platform bit equality of the interval: it is computed in floating point
  and rounded to micros, so it is stable on one build and can differ in the last
  digit elsewhere.

## Explicit sidecar export

Collection or learning consent alone never causes network transmission.
`sidecar outcome` and `sidecar feedback` additionally require an explicit
`--allow-sidecar-export`, destination, artifact pin, event hash and current consent
revision. They send exactly one retained event under a live consent guard; no
runtime completion path invokes them. The projection removes free identifiers
and keeps automated, human, validation and objective outcomes distinct. The guard
holds the store lock across the exchange; any store call from the thread holding
it returns `feedback.storage_locked` instead of deadlocking, and other threads wait
until the guard drops.

See [standard sidecar export](standard-sidecar.md) for the authority boundary,
redaction, retention checks and `delivery_unknown` after an ambiguous POST. A
server ACK does not prove learning. Local purge does not claim deletion of data
already delivered to an external process.
