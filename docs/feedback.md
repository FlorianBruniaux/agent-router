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
unknown; none become zero by omission.

`statistics(now)` is available only with learning consent. It rebuilds a
versioned, content-hashed report from retained immutable events:

- Beta(1,1) competence by agent/skill, with human and automated observations
  separate; unknown objective outcomes update neither alpha nor beta.
- Exact count, sum, sum of squares and empirical min/median/p95/max for cost,
  latency and tokens by model/effort/harness; missing values have their own count.
- Attempts, escalations, transport failures and failure classes.
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
Router will feed this store. None of it is merged yet; until its batches land,
the behavior described above is the current one, and the automatic record keeps
validation and objective `Unknown`, so real runs alone cannot pass the proposal
gate.

What will change for an operator:

- A run will count as an automated success only when its integration is applied
  and a recorded verification has a `Passed` objective. A transport exit code of
  0 will still never count.
- `verify`, `artifacts apply` and `artifacts discard` will append late facts,
  one per run and kind, without rewriting the completion verdict.
- Token usage reported by the host will be stored with `host_reported`
  provenance, for reports only. Admission budgets will not use it.
- The store will migrate to schema version 2 on open. Consent, retention, export
  and purge will cover the new facts.
- The data will feed three metrics per accepted task: accepted success rate,
  wall-clock time from run start to `integration.applied` of the accepted
  result and attempts, and tokens with computed cost. Late facts will carry a
  timestamp for the time metric. None of them alone proves a gain from the
  router; that needs the paired A/B
  [router value protocol](bench/router-value-protocol.md) against fixed
  baselines.
- A `feedback report` command will show verified success, usage, computed cost
  and `UNKNOWN` counts, with an advisory suggestion. Activating a policy will
  still go through propose, evaluate, trial and signed apply.

Collection stays opt-in through `feedback_store`, and the product defaults do
not change. A longer retention, such as 90 days on a single workstation, is an
explicit configuration choice made in the preferences passed to `feedback
configure`.

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
