# Observe an existing run without executing work

`agent-router observe` reads one coherent, bounded projection of an existing runtime store. Default v1 remains unchanged; opt-in v2 adds requested selections and supervisor outcomes per attempt from validated durable evidence. Opt-in v3 adds qualified local supervisor capture intervals ([ADR 0030](adr/0030-qualified-local-capture-timing.md)). Opt-in v4 adds a bounded chronological timeline ([ADR 0048](adr/0048-run-timeline-projection.md)). Opt-in v5 adds advisory findings ([ADR 0049](adr/0049-run-findings.md)). It does not start or recover a run. F04b and a native, timestamped timeline remain open. The contract is recorded in [ADR 0025](adr/0025-run-observation.md).

## Read an existing store

Use a store and run identifier you already obtained from an authorised run. In these format examples, replace `ROOT` with that existing private store directory and `ID` with its existing run identifier:

```sh
agent-router observe --store ROOT --run-id ID
agent-router observe --store ROOT --run-id ID --json
agent-router observe --store ROOT --run-id ID --schema-version 1 --json
agent-router observe --store ROOT --run-id ID --schema-version 2
agent-router observe --store ROOT --run-id ID --schema-version 2 --json
agent-router observe --store ROOT --run-id ID --schema-version 3
agent-router observe --store ROOT --run-id ID --schema-version 3 --json
agent-router observe --store ROOT --run-id ID --schema-version 4
agent-router observe --store ROOT --run-id ID --schema-version 4 --json
agent-router observe --store ROOT --run-id ID --schema-version 5
agent-router observe --store ROOT --run-id ID --schema-version 5 --json
```

Omitting `--schema-version` and selecting version 1 produce byte-identical v1 output for the same durable snapshot. Versions 2 to 5 are explicit. All versions use one read transaction and the same store/journal bounds.

An absent directory, database or run is refused. The command does not create a store, migrate it, repair history, acquire supervision ownership, cancel work or release permits. SQLite may create or update WAL/SHM coordination files while reading; passive observation does not guarantee zero filesystem activity. Private artifacts and lease files are not opened.

## Interpret the two views

The human view prints the snapshot digest, hashed run key, recorded state/attempt count, journal horizon, recorded cleanup and explicit unknown fields. It shows the last **20 selected-run event references**, each with its sequence and recorded state, and counts omitted references. `--json` contains the complete bounded selected-run reference list, including event hashes. Both formats derive from the same projection; separately invoked commands may observe different committed snapshots.

V1 contains the following fields. V2 retains them with `schema_version: 2`, recalculates the digest over the entire v2 projection and adds a top-level `attempts` array:

| Field | Meaning |
| --- | --- |
| `snapshot_sha256` | Digest of the entire redacted projection, with its own digest field emptied during hashing |
| `identity` | Hashed run/definition/receipt/plan/node identities; no raw identifier or path |
| `horizon` | Last sequence/hash of the verified global journal, including other runs |
| `known` | Persisted state, durable attempts, finite failure/cleanup interpretation and selected event references |
| `unknown` | Aggregate model/effort, transport, validation, objective, duration, authenticity and current process liveness remain `unknown`; v2 per-attempt facts are separate |
| `completeness` | Journal coverage `complete` only through the retained checked horizon, external delivery `unknown`, private detail `redacted` |

A later committed append changes the horizon and snapshot digest even if it belongs to another run. Repeating an unchanged durable snapshot produces identical JSON bytes. The run key follows the current approval binding and can change with a retry; it is not a universal task/session/send identity.

### Read v2 attempts

Each durable `attempt.started` appears in ordinal order, including starts with no outcome evidence. Missing facts are not borrowed from another attempt.

| Attempt field | Meaning |
| --- | --- |
| `ordinal`, `attempt_key_sha256` | Ordinal and domain-separated hash of run, definition, ordinal and start-event hash; independent of later approval-binding changes |
| `evidence` | `durable_attempt` when validated `AttemptEvidence` binds this attempt, otherwise `unknown` |
| `requested` | Recorded requested model and effort, or `null`; not the model or effort observed in native execution |
| `transport` | Recorded supervisor termination and nullable exit code, or `null`; not native execution or objective success |
| `sources` | Start-event hash and nullable evidence, finish, transition, incomplete-output and protocol hashes; protocol source stays `null` |
| `capture` | `unknown`, `incomplete`, `output_limit` or `output_limit_incomplete`; none asserts complete capture |
| `protocol`, `native` | Protocol, observed model/effort, thread/turn IDs, usage, delivery, lost notifications, duration, validation and objective remain `unknown` |

Capture uses the bound output-limit failure and an `output.incomplete` marker with detail `stream_reader_deadline` inside the validated start/finish interval. Both causes remain visible when both occur. The legacy `protocol.observed` marker has no ordinal binding, so it cannot qualify an attempt even when it appears inside that interval.

The v2 human view adds each attempt's requested selection, supervisor termination, exit code, evidence, capture and protocol after the legacy summary and its last 20 event references. The aggregate unknown fields do not replace these attempt facts. JSON includes all bounded event references and source hashes. Separate invocations can observe different committed snapshots.

The event references have no trusted timestamps or external notification semantics. A recorded terminal state or cancellation request does not prove process shutdown, useful output, human acceptance, a physical-send budget or complete provider delivery. Chain integrity does not establish authenticity against a same-user writer. Hashes support local correlation, not anonymisation or export consent.

### Read v3 local capture intervals

V3 retains the v2 `attempts` and adds `timings`, at most five rows in ordinal order. Each row carries `ordinal`, `attempt_key_sha256`, nullable `timing_event_sha256`, and fixed scope/clock/boundary/precision tags. `measurement` is `{ "status": "known", "duration_ms": 123 }` or `{ "status": "unknown", "reason": "missing_record" }`. Other unknown reasons are `clock_regression` and `duration_overflow`.

Only an `attempt.timing.v1` record bound to the exact validated attempt evidence can qualify a duration. Its producer uses the same monotonic endpoints as the recorded transport elapsed value: immediately before dispatch/capability validation through wait/reap and both bounded output drains. Precision is truncated milliseconds. A measured submillisecond interval can be zero; absent or unusable measurements cannot.

This is local supervisor capture time. It includes checks, spawning, termination handling and drain waits; it excludes initial checks before the start, later cleanup confirmation, host-state cleanup, decoding, artifact/journal writes, preparation and human waits. It does not establish native-model/provider duration, physical cleanup completion or end-to-end task duration. Native and aggregate duration remain `unknown`.

The human v3 view adds one local capture duration or explicit unknown reason per attempt. For example, a synthetic known interval renders `Attempt 1 local supervisor capture: 123 ms`; a legacy attempt renders `unknown (missing_record)`. These are format examples, not benchmark results.

V3 does not import or upgrade external Codex exports such as M4 v2. Those retain their separate host-side collection and acceptance receipts.

Old elapsed fields alone remain unqualified. Interrupted or error paths without a completed persisted marker stay unknown; restart never manufactures endpoints. Completed markers survive reopening. A malformed, duplicate, mismatched, unsupported or oversized marker refuses v3 rather than falling back to unknown. V1/v2 views of the same unchanged snapshot keep their existing serialization and hash domains. New timing events change journal horizons normally.

### Read the v4 timeline

V4 retains every v3 field (`schema_version: 4`, digest domain `run-observation-v4`) and adds `timeline`. The human view is a dedicated renderer of the same value and prints the snapshot digest, so two renderings of one unchanged snapshot are byte-identical. The v1 to v3 projection code is not modified by v4 (only `project_v3` becomes crate-visible); the existing frozen-output test pins v1 bytes, and no golden file pins v2 or v3. `known.events` of the v3 part stays in the v4 JSON (up to 4,096 references); the limit of 64 bounds only `timeline.entries`.

| Timeline field | Meaning |
| --- | --- |
| `entries` | The most recent **64** selected-run events in ascending journal order: sequence, event hash, recorded state, `phase`, `kind`, a positional `attempt` and `after_cancel_request`. `kind` comes from a fixed allowlist; any other journal kind is `unclassified`. Event detail is never printed |
| `run_events`, `shown`, `omitted_earlier` | The bound in numbers. Omitted events are still counted by `phases`. The human view prints an explicit omission line |
| `later_global_events` | Journal events committed after this run's last event (other runs). A non-zero value means the global horizon moved past the run |
| `phases` | Ten fixed rows (plan, approval, preparation, attempt, transport, verification, cancellation, cleanup, terminal, other) with count, first and last sequence |
| `approval` | `granted`, `not_granted`, `pending` or `unknown`, from state transitions only. A run cancelled while awaiting approval is `not_granted`; the store does not record who refused |
| `cancellation` | The logical request of the current approval cycle and the cancelled state. A `retry.awaiting_fresh_approval` ends the cycle: an earlier request still marks its own events but no longer appears here. It does not prove that a process stopped |
| `cleanup` | The recorded cleanup scope and the sequence of any cleanup marker. `physical_cleanup` stays `unknown` |
| `verification` | A state derived from the run state and the sequence of the last verification record. The objective stays `unknown` |
| `attempts` | Per attempt: requested model and effort from validated evidence, `applied` always `unknown`, usage, recorded cleanup, whether the finished marker follows the cancel request of its own approval cycle, and that request's sequence (`cancel_request_sequence`) |

Usage shows evidence token and cost values as `recorded_unattested` when both exist, `partial` when one exists and `unknown` otherwise. An absent value is never zero and native usage is not attested.

`unknown` fixes acceptance of the exact revision, notification delivery, lost notifications, event timestamps, physical cleanup and the objective, because no producer writes them. A late `attempt.finished` or evidence after a cancel request is shown as such. The view cannot tell a lost notification from none: an `output_incomplete` marker is the only capture-gap evidence.

The `attempt` on an entry is positional (the latest attempt start at or before the event), not a journal binding. A refusal (corrupt chain, missing store, exceeded limit) has no timeline. The view never repairs the store, retries, approves, opens artifacts or touches the network; tests assert the journal, run summary and control rows are unchanged after repeated reads, including with an owner open.

A synthetic example: a run cancelled after its attempt began, whose result was still recorded afterwards. The run identifier, model and digests are fixtures, not benchmark output.

```text
Run timeline (schema 4)
Snapshot: e7d97472fb2b63ecc39dc3913cefb0e4ed1ab2fae447120dd6658c96553a6389
Run key: 603d00cb0b1ed988498039940e5a0937c5360273f9d56b33df8acb4558250e72
State: cancelled | Attempts: 1
Journal: complete through global sequence 10 | Later events from other runs: 0 | Details: redacted
Ordering: journal_sequence_ascending (no timestamps)
Showing 10 of 10 events (limit 64)
Events (sequence: phase kind [recorded state]):
  1: plan planned [planned]
  2: approval transition [awaiting_approval]
  3: approval transition [queued]
  4: preparation transition [preparing]
  5: attempt attempt_started [preparing] attempt 1
  6: attempt transition [running] attempt 1
  7: cancellation cancel_requested [running]
  8: transport attempt_finished [running] attempt 1 after cancel request
  9: cancellation transition [cancelled] after cancel request
  10: attempt attempt_evidence [cancelled] attempt 1 after cancel request
Phases:
  plan 1 (first 1, last 1)
  approval 2 (first 2, last 3)
  preparation 1 (first 4, last 4)
  attempt 3 (first 5, last 10)
  transport 1 (first 8, last 8)
  verification 0 (first none, last none)
  cancellation 2 (first 7, last 9)
  cleanup 0 (first none, last none)
  terminal 0 (first none, last none)
  other 0 (first none, last none)
Approval: granted (awaiting 2, granted 3; state transitions only)
Cancellation: requested 7 | before start none | terminal cancelled: yes | events after request: 3 (logical only)
Cleanup: recorded unknown | confirmed marker none | unknown marker none | physical shutdown unknown
Verification: not_reached (record none) | Objective: unknown
Attempt 1: requested vendor/model-v4:2026_10.28 effort high | applied unknown/unknown | usage partial (tokens 5, cost unknown, unattested) | recorded cleanup unknown | result after cancel request yes | local supervisor capture unknown (missing_record)
Unknown: exact revision acceptance, notification delivery, lost notifications, event timestamps, physical cleanup, objective, applied model and effort
```

Evidence limits for this sample: ordering is the journal sequence, not time; no duration is known (capture duration needs a v3 timing record); "cancelled" says a cancellation was recorded, not that the process exited; the model is the requested one; tokens come from evidence and are not provider-attested; nothing says the work was useful or accepted.

### Read the v5 findings

V5 retains every v4 field (`schema_version: 5`, digest domain `run-observation-v5`) and adds `findings`, derived by a pure function from the v4 report of the same read transaction ([ADR 0049](adr/0049-run-findings.md)). Findings are advisory text and references. Nothing reroutes, approves, retries, releases a reservation, changes a permission or calls a provider. `automatic_action` is always `none`.

Each rule produces either one finding or one named suppression, so a missing finding is never silent. `rules_version` is 1.

| Rule | Fires when | Severity | Suppressed with |
| --- | --- | --- | --- |
| `repeated_failed_attempts` | Two or more attempts with validated evidence ended with a non-zero recorded exit code, a timeout, a kill or a spawn failure. A cancelled attempt and an unrecorded exit code do not count | warning | `below_threshold`, or `attempt_evidence_incomplete` when an attempt has no evidence |
| `cleanup_unconfirmed` | The run is terminal, at least one attempt started, the recorded cleanup is `unknown` and no cleanup-confirmed marker is newer than the last cleanup-unknown marker | warning | `run_not_terminal`, `no_attempt_started`, `cleanup_marker_confirmed`, `cleanup_recorded` |
| `late_result_after_cancel` | An attempt's finished marker follows the cancel request of its own approval cycle and the supervisor did not report a `cancelled` termination. An ordinary honoured cancellation never fires it | info | `no_cancel_request`, `ordinary_cancellation`, `no_gap` |
| `attempt_evidence_missing` | An attempt started without bound durable evidence and the run is no longer preparing or running | info | `run_in_progress`, `no_gap` |
| `attempt_duration` | Never. Duration rules need qualified timing (#117); the rule is listed so the dependency is visible | n/a | `timing_not_qualified` |

A finding carries `id`, `source_snapshot_sha256` (the v4 digest it was derived from), `basis` (`recorded_journal` is the only source), a fixed `fact`, `hypothesis`, the requirements to `preserve`, a discriminating `check`, `affected_attempts` and `evidence` (event sequence and hash from the same snapshot). The `id` digests the rule, the run key and the supporting event hashes. It does not depend on the journal horizon, so an unrelated run's append keeps it, while a new supporting event changes it. A finding from an older `source_snapshot_sha256` is not evidence about a later snapshot. To check that digest, take the v5 JSON without `findings`, set `schema_version` to 4, empty `snapshot_sha256` and hash the compact JSON (fields in the order emitted) with SHA-256 after the prefix `run-observation-v4` and a NUL byte; the result must equal `findings.source_snapshot_sha256`. A test performs this recomputation on the typed report.

`attempt_evidence_missing` has no in-flight false positive on a committed journal: the finished marker, the transition, the attempt evidence and the timing record are written in one IMMEDIATE transaction (`crates/router-runtime/src/store/escalation.rs:21-67`), and a run still preparing or running is suppressed with `run_in_progress`.

Rules that were considered and not delivered: tool-call loop and hung-process detection (the journal has no tool-call events and a missing event is not a hang), anything about Advisor results or skill load (no producer in this store), allocation status, and every elapsed-time rule. A corrupt, oversized or unreadable journal refuses the whole observation, so no finding is produced from it. The cleanup finding states uncertainty only: it cannot assert that a process stopped or release a reservation.

The same fixture as the v4 sample, with two attempts that exited non-zero and timed out:

```text
Findings (rules v1, advisory only, no automatic action; source snapshot 460b59e55c62e36e5a49ba29619a227e7f576d481d42543b6a4ed03d50a2ba98):
[warning] repeated_failed_attempts id 0ab5b508110ede53fa02999fa4c0eaf588012af57a90c5d2dddf723b6ccbb35a
  Attempts 1, 2 | Basis: recorded_journal | Evidence: event 7, event 15
  Fact: Two or more attempts of this run ended without a recorded clean exit.
  Hypothesis: The same cause may repeat across retries; the journal does not show the cause.
  Preserve: the original task requirements; a fresh approval before any retry
  Check: Compare termination and exit code of the listed attempts, change one input, and observe the next approved attempt again.
[warning] cleanup_unconfirmed id 94c9d4788bb1f377dc48ddc0893270c4bbb2dab59debd4d97156fcff81fda1a7
  Attempts 1, 2 | Basis: recorded_journal | Evidence: event 17
  Fact: The run is in a terminal state but its process cleanup is not confirmed in the journal.
  Hypothesis: A supervised process or lease may remain; the absence of a marker proves neither outcome.
  Preserve: no reuse of the worktree or lease until cleanup is confirmed; no release of any reservation on this evidence alone
  Check: Inspect the store with the read-only `agent-router doctor --runtime-root` and confirm no supervised process remains.
Suppressed rules (rule: reason):
  late_result_after_cancel: no_cancel_request
  attempt_evidence_missing: no_gap
  attempt_duration: timing_not_qualified
```

Evidence limits: the sample comes from a synthetic store. "Failed" is the supervisor's recorded termination, not a judgement of the model or of the task; the hypothesis is not a diagnosis; the check is a suggestion for a person and is not executed.

## Refusal and bounded work

The global journal is capped at **100,000 rows**, **64 MiB** of text and **65,536 bytes per row**; a selected run is capped at **4,096 event references** and its identifier at **1,024 bytes**. Summary and cleanup dependency tables have additional row/byte ceilings in [ADR 0025](adr/0025-run-observation.md#bounds-and-refusal). SQLite preflight checks counts and byte lengths before decoding private JSON.

An unsupported schema, broken chain, sequence gap, substituted binding, summary/history mismatch, malformed private JSON or exceeded ceiling refuses the observation. Corruption or limits in another run can also refuse it because the global chain is checked. There is no truncated-success result: a refusal has no snapshot identity and must not be interpreted as an empty or complete report. Older readable schemas preserve unknown cleanup without migration.

V2 also checks the actual producer's semantics: start state `preparing` with empty detail, finish state `running` with detail `observed`, then detail `transport_observed` and an outcome permitted from `running`. Evidence must bind the run, definition, receipt, ordinal, observed-attempt row and ordered event references. Conflicting or duplicate evidence is refused. These checks do not change v1.

Only model identifiers of at most 256 bytes using ASCII letters, digits or `-_.:/` may be exposed in `requested.model`; other values refuse the v2 report. This lexical restriction is not anonymisation or secret detection. Accepted identifiers and hashes do not authorize export.

The byte/row checks bound this reader’s Rust allocation and decoding work. They do not establish an OS sandbox or protection against every malicious database layout. The existing `status` and `events` commands keep their contracts.

## Delivery boundary

The [v3 verification record](bench/verification-local-capture-timing-2026-10-11.json) binds this timing slice to the combined source after M4 and PR #163. All seven selected local gates pass on macOS, including the full serialized Rust workspace suite outside the tool sandbox. Independent static review and final documentation review found no unresolved defect. Historical Python-version and sandbox-listener failures remain recorded with their successful follow-up checks. This result does not qualify Linux timing, full cloud CI, the copied native composition or savings.

The [v2 verification record](bench/verification-observation-v2-2026-10-08.json) records all 24 selected local checks passed across the complete run and two metadata repairs. The ADR-index date mismatch and synthetic-hash secret-scan false positive remain recorded; the 30 ADR tests and both scans passed after correction, with no Rust changes. The run contains 1,685 successful workspace Rust results plus one core check, and 365 Python tests. Rust totals sum result lines, not unique test definitions; 42 ignored native tests are not successes. The five existing measurement-reader regression tests passed. Independent review and 21 adversarial cases are included; native usage, costs and measured savings remain unknown.

The historical F04a v1 delivery, [PR #139](https://github.com/FlorianBruniaux/agent-router/pull/139), was tested and independently reviewed locally. Twelve runtime and three CLI focused tests passed, and 19 independent probes found no confirmed blocker. The [source-bound verification record](bench/verification-observation-2026-10-07.json) records all 21 selected quality checks passed: 1,633 successful workspace Rust results plus one core check, and 290 Python tests. Rust counts sum result lines, including subprocess invocations, rather than unique test definitions; the focused tests are included in the workspace count. The 42 ignored opt-in native tests are not successes. These historical results do not verify v2, full CI equivalence, a live provider, useful patch, release or measured gain.

`agent-router schema run-observation` publishes all five shapes; `schema --list` and `version --json` advertise versions `[1, 2, 3, 4, 5]`. It rejects extra fields, unsupported known claims, malformed digests and values outside the documented bounds. Schema validation does not recompute hashes, verify event ordering or establish the authenticity of a supplied snapshot. F04b still owns qualified native fields and malformed/lost/truncated notification coverage. The F05a timeline is delivered by v4 over recorded state only; trusted timestamps, native notification coverage and the broader F05b/#117 timing scope remain open. See [CLI output](cli-output.md), [JSON contracts](reference/json-contracts.md), the [bench playbook](bench/playbook.md) and the [current finalization plan](bench/finalization-plan-2026-10-07.md).

## Correlate a supplied export

The [attempt-correlation CLI](attempt-correlation.md) projects a complete v2 or v3 export into the unchanged offline B1 calculator and joins explicitly claimed physical observations. V3 timing rows and their evidence links are validated, but durations are not aggregated or added to the B1 report: keep the original v3 export for those measurements. The command does not reopen the runtime store or authenticate the journal. Usage and native identity stay unknown; v1 output is unchanged.
