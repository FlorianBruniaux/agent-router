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
`not_requested`, `collection_disabled`, `inconclusive`, `invalid_fact`,
`already_recorded` (a fact with the same content is already stored, possibly
under another `observed_at`, and is left as is), `verification_record_unavailable`
(`coordinate service` could not read the stored record) or
`consent_changed_or_storage_error`. Once the operation succeeded, a feedback
failure never fails the command. There is no retry: if the process dies between
the runtime operation and the fact write, or the write fails, the fact is lost
and nothing records it later.

A coordinated run records its verification fact in `coordinate service`, which
applies the queued verification (`coordinate verify` only queues it). For each
`--bundle` it reads `feedback_store` from that bundle's own sandbox
configuration, the source the automated event of the same run used, so no new
opt-in is involved. A run gets a fact only when this service pass moved it to
`verified` or `verification_failed`, from the record the runtime stored, under
the same run key as its automated event. The result is the `feedback` array of
the service output, `{"run_id":...,"feedback":{...}}` per such run, with the
reasons above. A feedback failure, including an unreadable bundle
configuration, is reported there and never fails the service. Limits: a run
concluded by a service pass that did not list its bundle, or before this
behavior, has no fact; and a lost write is not retried.

## Exporting a run as a replay record

`agent-router feedback --store <dir> --json replay-export --definition
<definition.json> --runtime-store <dir> --pricing <pricing.json> --case-id <id>
--family-id <id> --split <training|validation|held_out> [--output <file>]`
turns one executed run into a sealed `ReplayRecord` that `eval`, `replay` and
`policy evaluate` accept. It changes nothing in either store. The record is an
explicit local file made on demand. It is never kept in the feedback store and
it is not covered by `purge`: delete the file yourself.

What it reads:

- the request, host snapshot, catalog and policy from the run definition, as
  `run` loads them. Nothing is re-routed. The runtime store reopens the run
  against that definition, so a changed definition is refused, and the executed
  candidate is the one the runtime recorded. The store lock applies: a run that
  is still being supervised makes the command fail;
- the run's automated event, late facts and human event from the feedback store,
  found through the run key that `run` and the late-fact commands already share.

Label rules. Only a fact or a human verdict labels; a transport exit code never
does.

| Recorded | Result |
| --- | --- |
| Integration `applied` and verification objective `passed` | Executed candidate expected, `human_validated: false` |
| Human verdict `success: passed` | Executed candidate expected, `human_validated: true`, `reviewer_id` set to the reviewer digest |
| Both | Expected, `human_validated: true` |
| Nothing, or only part of the first row | Refused, `feedback.replay.unlabeled` |
| Integration `discarded`, validation or objective `failed`, or human `failed` | Refused, `feedback.replay.rejected` |

A rejection is refused because `CaseLabel` cannot say "this candidate is not
expected": `expect_abstention` is true exactly when the expected set is empty,
which claims that the router should abstain, a different statement. Any negative
signal wins over a positive one. `independent` is always false, since nothing
here attests a reviewer's independence, so the record cannot satisfy the
independent-label gate of `compare_shadow`. `evidence_sha256` is the
verification fact's evidence digest, or the human event's own when there is no
fact. A run can hold several automated events, since `run_hash` covers state and
attempts: like `feedback report`, the export takes the one with the highest
sequence for the run key, and `transport`, `usage` and `elapsed_ms` come from it.
A human verdict is linked to the run through the `run_hash` of any of its
automated events. Human verdicts that disagree on `success` make the export
fail with `feedback.replay.ambiguous`; identical ones do not. With `--output`
the record is written to a private temporary file next to the target, synced
and linked into place, so a failure leaves no partial file and an existing file
is never replaced.

Observations. `outcomes.transport` comes from the automated event, and
`command_validation` and `functional` from the verification fact; a missing
source stays `unknown`. `elapsed_ms` is the transport latency, not the time to
acceptance. `usage` is one row for the executed candidate's model, read from
`measurements.usage` of the automated event: `input_tokens` is the total input,
cache included, as `price_usage` subtracts `cached_input_tokens` itself, and
`output_tokens` includes reasoning. If the event has no `usage`, or one of these
three counts is undeclared, `usage` is an empty list and the report says
`"usage": "unknown"`: UNKNOWN, never a row of zeros. Replay prices the selected
model from `usage`, so `replay_deterministic` refuses a record with an empty
`usage` (`selected model usage missing`) until usage is added by hand and the
record is sealed again with `eval seal`. Events written before batch 6 carry no
usage.

Privacy. The record carries whatever the definition carries: the request's task
objective and any deliverables, constraints, scope, dependencies and acceptance
text. The report lists the non-empty ones in `request_free_text_fields`
(`task.objective` is always present). The execution prompt in the execution
request is not part of the record. Without `--output`, the record is printed to
standard output; with it, the file is created private (mode 0600) and an
existing file is refused.

Limits. The replay context is the request, snapshot, catalog and policy without
the run's asset authority: `replay_context` reads `without_asset_authority` when
the definition used one. The policy id under `policies` and `observations` is the
policy `version`. `out_of_domain` is false.

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

## Outcome measurement for executed runs

[ADR 0019](adr/0019-outcome-measurement.md) decides how runs executed by Agent
Router feed this store. All six batches are merged and described in this file:
host-declared tokens (batch 2), late facts and the run start time (batch 3),
route facts and the escalation count (batch 4), replay export (batch 5) and
[`feedback report`](#feedback-report) (batch 6). The automatic record still
keeps validation and objective `Unknown`, and the proposal gate does not read
the facts, so real runs alone cannot pass it. The report and the replay export
read them.

For an operator:

- A run counts as an automated success only when its integration is applied
  and a recorded verification has a `Passed` objective. A transport exit code of
  0 never counts.
- The per-field token detail (input, cached input, cache writes, output,
  reasoning), from `RuntimeEvidence.host_usage`, is stored in
  `measurements.usage` with its `host_reported` provenance, for reports only.
  Admission budgets do not use it, and the sidecar export omits it.
- The data feeds three metrics per accepted task: accepted success rate,
  wall-clock time from run start to `integration.applied` of the accepted
  result and attempts, and tokens with computed cost. None of them alone
  proves a gain from the router; that needs the paired A/B
  [router value protocol](bench/router-value-protocol.md) against fixed
  baselines.
- A labelled run can be exported as a sealed replay record and evaluated with
  the existing chain. Activating a policy still goes through propose, evaluate,
  trial and signed apply.

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

The verdict reads the verification fact's `objective` only. That is enough
because a fact with objective `passed` and validation other than `passed` is
refused when recorded, so a passed objective always follows a passed validation.

`run_hash` covers run state and attempts, so one run can hold several automated
events. The report counts each run key once, on its latest event, and gives the
number of others as `superseded_events`. Its groups, `automated_events` and costs
count that one event.

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
  priced as input, since the table has no cache write price, and
  `runs_with_cache_writes_priced_as_input` counts the priced runs that declared
  some. Reasoning tokens are priced as output. Without `--pricing` every cost is null with reason
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

## Feedback summary

```sh
agent-router feedback --store <dir> summary [--pricing <file>] [--now <unix>] [--json]
```

`summary` is the one-line view of [`feedback report`](#feedback-report): a single
global total over every group. It reads the same snapshot and classifies the runs
with the same code, so `runs`, `accepted`, `rejected` and `unknown` equal the sum
of the report's groups. `runs` counts automated events only, as the groups do;
human-origin events are counted apart in the `human` block (`passed`, `failed`,
`unknown`) and never change an automated outcome. It needs the same collection
consent and has the same read effects. Without `--json` it prints the labeled
human view. Each view is byte-identical for the same retained events, facts,
pricing file and `--now`; the two views differ from each other. The versioned
schema is `agent-router schema feedback-summary`.

The rate, the percentiles and the cost are recomputed over the merged runs, never
averaged from group figures:

- `accepted_rate`: accepted over accepted plus rejected with the 95% Jeffreys
  interval, null when no run is decided. Unknown runs are excluded and never
  counted as failures;
- `time_to_acceptance_seconds`: p50 and p90 by nearest rank over accepted runs,
  with `samples` and `missing`;
- `cost`: with `--pricing`, the sum over priced runs as `total_microusd`.
  `per_accepted_microusd` divides the total over all runs by accepted tasks and
  is null, with `reason`, while any run of any group is unpriced. Without
  `--pricing`, or when no run is priced, the total is null with its reason;
- `unknown_counts`: `verdict`, `unlinked`, `cost`, `time_to_acceptance`,
  `integration`, `orphan_facts` and `superseded_events`, each a count of what the
  figures above could not establish;
- `report_content_hash`: the `content_hash` of the report from the same events
  and facts, so a caller can tie the two documents together.

`attribution` is always `not_established`, with a note: the summary shows
descriptive associations, and a gain attributable to the router needs the paired
A/B [router value protocol](bench/router-value-protocol.md). It is not a savings
figure.

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
