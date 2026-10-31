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

## Advisory outcomes

`agent-router feedback advisory-record` stores what you report after doing a task
by hand against a saved [`advise`](advisory-route.md) recommendation
([ADR 0052](adr/0052-advisory-outcome-record.md)). It is a separate record, not a
`FeedbackInput`: a manual task has no supervised run, receipt or approval, so no
such hash is invented. The record binds to the saved decision by the
`decision_id` of `advice.json`.

It holds the catalog and policy versions, the task kind, the effective risk, the
`selection_basis`, the recommended phases, your `adherence` (`followed` or
`overridden`), your `result` (`accepted_as_is`, `accepted_after_rework` or
`abandoned`) and optional host-reported `cost_microusd` and `tokens`. An override
names at least one of `--used-model`, `--used-effort` and `--used-skill` (or
`--used-no-skill`). There is no field for a prompt, objective, path or comment,
and every string is an enum value, a hash or an identifier. The identifier check
is syntax, not catalog membership: letters, digits and `._:-`, at most 96
characters, with no `/`, no `\` and no leading `.` or `~`, so free text and
paths (with or without spaces) are refused with `feedback.invalid`. The task
kind, risk, `selection_basis` and recommended phases are copied from the
`advice.json` you pass, a file you can edit; only `decision_id` is a digest of
the request, so the per-kind grouping rests on your own file. An `overridden`
record that states a model, effort and skills all equal to every recommended
phase is refused as incoherent. A value you do not state is `null`, which is
UNKNOWN: a missing cost is never zero, and an omitted skills list is not "no
skill".

Collection consent, the clock rules and retention apply as for other feedback.
Advising the same task again gives the same `decision_id`. Repeating the latest
record is idempotent; a different outcome for that decision (a retry or a
correction) is stored as its next attempt, and nothing is rewritten. The report
counts each decision once, with its latest attempt, and shows `records` beside
`decisions`. The store migrates from version 2 to 3 when
opened, in one transaction that only adds the outcome table. A version 3 store
is refused by binaries that predate it (`future schema`); there is no downgrade. `feedback export`
lists the outcomes under `advisory_outcomes`, omitted when empty so an earlier
export keeps its hash. Disabling collection, expiry and `feedback purge` delete
them; the purge count includes them. `report`, `summary`, statistics, drift and
policy learning do not read them.

`feedback advisory-report` groups the outcomes by task kind and compares
followed with overridden. Next to every rate it prints the numerator, the
denominator and the sample size (decisions, with their record count), and it prints the UNKNOWN counts for cost, for
tokens and, for overridden outcomes, for the model, effort and skills used. The
result is a required field, so it has no UNKNOWN count. A group with no outcome
has a null rate, not zero. The report counts `selection_basis` values, so a
tie-break among unrated models is visible. It does not sum or compare costs, and
`attribution` is always `not_established`.

A personal report is not evidence of a general gain. The records are
self-reported and unverified, the sample is yours and self-selected, and the
tasks differ. Comparative evaluation is tracked in
[issue #5](https://github.com/FlorianBruniaux/agent-router/issues/5).

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

To include durable runtime attempts whose feedback was never written, add
`--runtime-store <private-runtime-dir>` to `report` or `summary`:

```sh
agent-router feedback --store <feedback-dir> --json report --runtime-store <runtime-dir>
agent-router feedback --store <feedback-dir> --json summary --runtime-store <runtime-dir>
```

With legacy v1 pricing or no pricing, this opt-in path returns report schema 4 or summary schema 3 with
`task_measurement`. Without that option, the schema 3 report and schema 2 summary
retain their shapes and hash domains. The ordinary per-run groups remain present;
`task_measurement` is a separate total scoped to the supplied runtime store.

The inventory reads a coherent SQLite transaction and validates journal and
attempt consistency. It does not update runtime rows, approvals, grants or the
journal. SQLite may create private WAL/SHM coordination sidecars when reading an
idle WAL database; this is not a guarantee of zero filesystem writes. The
feedback reader retains its existing consent, expiry and watermark behavior.

A task occurrence uses its workflow instance/node, or the root of its recorded
escalation chain. Repeated task text is never a join key. Legacy feedback keys
that change during retries are reconciled from retained runtime evidence; absent
identity evidence remains unlinked. Snapshots do not become additional attempts.
The inventory is bounded to 10,000 runs, 100,000 events, 64 MiB of projected
payload and at most five attempts per run. Inconsistent or corrupt inventories
are refused; a feedback snapshot ahead of its runtime snapshot is not truncated.

Each occurrence exposes expected, observed, missing-feedback and priced attempt
counts. Missing feedback stays in the denominator with unknown consumption.
`known_subtotal_microusd` describes the known portion; `total_microusd` remains
null unless coverage is complete. Acceptance still requires the corresponding
feedback facts; a verified runtime without an acceptance fact stays unknown.
Abandoned and failed occurrences remain visible. `per_accepted_microusd` is
undefined when no occurrence was accepted or total cost is incomplete.

`unmatched_feedback_runs` preserves feedback outside this runtime inventory,
including external native sessions. The command cannot manufacture runtime
receipts for those sessions. This reconciliation measures recorded attempts;
it does not establish a gain attributable to routing.

### Qualified pricing

`--pricing` also accepts `schema_version: 2`. This produces report schema 5 or
summary schema 4, with `pricing.contract_version: 2`, whether or not
`--runtime-store` is present. The v1 pricing contract and its report hash domains
remain unchanged. Pricing provenance is declared by the caller; the command
does not retrieve tariffs or verify an invoice.

Each model explicitly supplies nullable rates in microUSD per million tokens:
`input_microusd_per_million`, `cache_read_microusd_per_million`,
`cache_write_microusd_per_million` and `output_microusd_per_million`.
A cache-write rate requires `cache_write_scope: "all_reported_writes"`.
Host usage has no TTL classification, so a TTL-specific price cannot be applied
to all writes by inference. `reasoning` is either
`{"mode":"included_in_output"}` or
`{"mode":"separate","microusd_per_million":4000000}`.
The value in this example is synthetic, not a provider quote.

Input totals include cache reads and writes; output totals include reasoning.
The calculator subtracts those categories before charging separate rates and
rounds upward once after adding the known charges for each attempt. Missing
counts, scope or rates retain the known subtotal but leave the complete total
null. An absent model price stays unknown; overlapping categories cannot
produce a complete price.

For the synthetic rates input 1, cache read 0.5, cache write 2, output 3 and
separate reasoning 4 microUSD per token, usage of 100 input, 20 cache read,
30 cache write, 40 output and 10 reasoning tokens costs
`50 + 10 + 60 + 90 + 40 = 250` microUSD. Removing the cache-write rate leaves
a known subtotal of 190 and an unknown total. This arithmetic fixture does
not measure provider cost or routing benefit.

`feedback replay-export` explicitly rejects pricing v2 with
`feedback.replay.qualified_pricing_unsupported`: the existing replay usage
format cannot preserve every required category. Detailed qualified replay
remains unsupported; historical v1 replay exports retain their contract.

### Retained feedback and report scope

The report requires collection consent. It does not execute a task or change a
routing policy. Like the other feedback readers, it may expire retained rows
and advance the clock watermark. The same retained
events, facts, pricing file and `--now` give byte-identical output: lists are
sorted and every figure is an integer or a fixed-precision value in micros.

Schema 3 includes `runs`: one row per retained automated run, ordered by event
sequence, with its task/run binding, optional stable run key, observation time,
model, effort, task kind, verdict, transport status, failure, latest usage and
attempt cost coverage. Human-origin feedback remains in separate counts.
Published schemas 1 and 2 remain accepted with their original shapes. Existing
feedback events and store format are unchanged. The report hash uses
`feedback-report-v3`; summary schema 2 uses `feedback-summary-v2`.

Each row's `evidence.source` is `feedback_store`. `feedback_event_hash`,
`verification_fact_hash` and `integration_fact_hash` identify the retained source
records; absent facts have null hashes. The binding is copied from feedback,
including its declared receipt hash and producer provenance. These hashes are
references, not proof that a runtime receipt or journal has been verified.
`runtime_evidence` and `native_external_evidence` therefore remain `unknown`.
This report does not import native external sessions or fabricate AR receipts
for them. Authenticating those sources requires a separate capture/verification
path.

`usage: null` means no usage was stored. When present, every token field retains
its `host_reported` or `unknown` provenance. Transport `passed` alone does not
imply an accepted task; transport failures and failure reasons remain visible
even if the outcome has no verification/integration fact and stays `unknown`.
Omit `--json` for the human view, which renders `unknown` as `UNKNOWN` and null
as `not set (null)`.

The rows reuse the existing bounded store snapshot (at most 10,000 events and
10,000 facts); no second authoritative store is created. Verdicts and token or
latency percentiles use the latest snapshot. Attempt costs are separate from
those latest-snapshot metrics. This provides no measurement of router gains.

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
number of others as `superseded_events`. Its groups and `automated_events`
count that one verdict. Report schema 3 and summary schema 2 price attempts
separately; the published report 1/2 and summary 1 schemas remain accepted.

The automatic producer sets `measurements.usage_scope: "per_attempt"`: usage
belongs to the one-based ordinal in `measurements.attempts`. The report groups
these snapshots by `(run_key_sha256, attempts)`, counting identical usage once.
Conflicting usage or models for the same ordinal make that attempt UNKNOWN.
The greatest observed ordinal determines expected coverage; missing ordinals,
missing usage and missing prices prevent a complete total. Old events omit the
marker and retain their bytes and hashes. Unmarked single-attempt runs retain
legacy latest-snapshot pricing. During mixed-format histories, an unmarked
ordinal one can contribute known cost, but disagreement with marked usage is a
conflict. Any unmarked retry leaves scope UNKNOWN and blocks a complete total.
Each v3 run row includes its own `cost` and coverage. Attempt prices use their
own model; group attribution follows the latest run snapshot.

Coverage applies only to attempts represented or implied by retained feedback.
This does not detect dispatches with no feedback record, join escalated runs,
or establish task occurrence identity. Identical task hashes never merge runs.
Host declarations and cache-write approximation remain distinct from billed cost.

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
- `cost`, only with `--pricing`, cites the table's `version` and `source`.
  Each attempt uses uncached input, cached input and output rates, rounded up
  to a micro-dollar. `known_subtotal_microusd` sums unambiguous priced attempts,
  including failed attempts. `total_microusd` is null unless every expected
  attempt is priced. `per_accepted_microusd` divides that complete total by
  accepted runs and is null with zero acceptance or incomplete coverage.
  `expected_attempts`, `observed_attempts`, `priced_attempts`, `missing_attempts`,
  `conflicting_attempts` and `scope_unknown_runs` expose coverage. `priced_runs`
  counts runs with complete cost; `cost_unknown` counts the rest. Cache writes
  use the input rate and reasoning tokens use the output rate without double
  charging. `runs_with_cache_writes_priced_as_input` counts runs with at least
  one priced attempt declaring writes. Without `--pricing`, monetary figures
  are null with reason `no_pricing_table`.

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

## Separate exact-revision calculation

[`eval accepted-changes`](accepted-changes.md) consumes a sealed offline input
without opening this store. It retains exact revision and revocation history,
physical work identities and incomplete costs. Existing `report` and `summary`
acceptance semantics are unchanged. Supplied evidence is not authenticated by
the calculator.

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
- `cost`: with `--pricing`, `known_subtotal_microusd` retains priced attempts.
  `total_microusd` and `per_accepted_microusd` require complete coverage across
  every group; zero accepted runs leaves the ratio null. Coverage counters sum
  across groups. Without any priced attempt, monetary figures are null;
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
