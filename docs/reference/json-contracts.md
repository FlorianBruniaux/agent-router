# JSON contracts

The first slice uses deterministic, versioned JSON. It only computes a route.
It does not execute a model, call a host API, create a worktree, or install
anything. A catalog or host configuration proves routing intent and declared
capabilities, not actual model execution.

## Unknown keys

These serialized contracts reject unknown keys, at every level they define:

- the unsigned catalog document (`schema_version`, `version`, `models`, `skills`);
- `ModelSpec` and `SkillSpec`;
- `RouteRequest`, `TaskContext`, `HostSnapshot` and `SkillCandidate`;
- `EffectiveRestrictions`;
- the BM25 candidate document and its candidates (see below).

A document that carries an extra key is an error, never silently ignored. This is
a breaking change for unsigned catalogs and route requests that carried extra
keys; fix the producer, the rule is not configurable.

## Additional versioned contracts

The v1 examples below remain supported. New commands use separate contracts:

| Surface | Contract and validation |
| --- | --- |
| Host observation | v1, full content digest, source provenance, observation time and tri-state capabilities; non-unknown claims require authenticated proofs |
| Generated catalog | v2 envelope, source snapshot/prior/registry hashes, validated aliases and deprecation |
| Phase route | v2 request; receipt v5 when authenticated capability proofs are consumed, otherwise the existing receipt version; full context/catalog/policy/pool/registry hashes and fail-closed permission checks |
| Retrieval | v1 unbound metadata or v2 envelope bound to exact catalog content; v2 operations require the matching digest |
| Workflow | v1 normalized task and hashed acyclic plan, scope and downstream independent verification checks |
| Preparation | v1 manifest and redacted preview; optional private request bundle, no automatic approval |
| Execution | v1 request, private persisted definition and approval; observed transport stays separate from objective verification |
| Patch publication | Separate v1 review, receipt and status contracts reject unknown keys; the approval binds the actual diff, regular-file blobs/Git modes, definition, repository identity, verifier record and deterministic result revision. Publication creates one dedicated ref; unknown status never grants replay. See [ADR 0027](../adr/0027-verified-patch-publication.md) |
| Evaluation | replay inputs, independently audited traces and explicit human-label status; no gain inferred from proposed labels |
| Feedback | v1 private consent/events (optional `run_key_sha256`, `started_at_unix_seconds`, `measurements.usage` and `measurements.usage_scope`), schema 2 store with late facts, a read-only `report`, signed policy changes with externally trusted keys; no online exploration |
| Feedback summary | v2 (`agent-router feedback summary`): one global total built from the same classification as `report`, with `attribution` always `not_established`; see `docs/feedback.md` |
| Runtime-reconciled feedback | `feedback report --runtime-store` emits report v4; `summary --runtime-store` emits summary v3. Adds bounded `task_measurement` from the runtime journal and optional feedback, including missing-feedback attempts. Default report v3/summary v2 and older published schemas remain accepted; see `docs/feedback.md` |
| Qualified report pricing | Pricing table v2 gives report v5/summary v4 and `pricing.contract_version: 2`, with optional runtime reconciliation. Explicit cache-write scope and reasoning mode preserve unknown totals and known subtotals. V1 shapes/hashes remain unchanged; detailed replay with pricing v2 is refused |
| Passive run observation | v1 remains default and byte-compatible; explicit `observe --schema-version 2` adds bounded durable attempt identities, requested selections, supervisor outcomes and source hashes from the same passive snapshot; `--schema-version 3` adds qualified local capture intervals and `--schema-version 4` a bounded, redacted timeline of at most 64 journal-ordered entries. `schema run-observation` supports `[1, 2, 3, 4]`. Native model/effort, usage, trusted timestamps, protocol, objective and acceptance of the exact revision remain unknown; see [run observation](../run-observation.md), [ADR 0025](../adr/0025-run-observation.md), [ADR 0030](../adr/0030-qualified-local-capture-timing.md) and [ADR 0048](../adr/0048-run-timeline-projection.md). |
| Runtime doctor report | v2 (`agent-router doctor --runtime-root`): `database`, `storage_schema_version`, `counts` and `leases` nullable, `partial_reason` added, `schema_layout` may be `not_inspected`; a partial report has `overall` `unknown` and exit 3. See `docs/doctor.md` |
| Modality input and JSON output | v1 `InputContract` (image, audio, file; canonical media-type allow-list, size bound, expected sha256) and `JsonOutputContract` (schema digest, size bound); unknown and duplicate keys rejected. Pure byte checks: the media type is declared, not read from content, digests come from the caller, output is not schema-validated, and nothing is transported. Execution stays refused while `ExecutionRequest` carries text only |

`agent-router schema --list` names the contracts that have a published JSON
Schema (`route-decision`, `feedback-report`, `feedback-summary`, `error`,
`run-observation` and `version`) with the `schema_version` values supported for each, and
`agent-router schema <name>` prints the document. `agent-router version --json`
lists the same versions per output. The schemas describe output shapes and
reject unknown keys; the sections of this file and the guides they link remain
the reference for meaning. See [CLI output](../cli-output.md#commands-for-programmatic-callers).

`PhaseCandidate.tool_policy` defaults to `restricted` and is omitted when
serialized at that default, preserving existing v2 identities. `native_host`
requires the explicit `native_host_tools` permission and observed Tools support,
and cannot be combined with a per-tool allowlist. It never removes OS sandbox
or native permission enforcement.

Commands taking an active policy store bind the selected policy content.
Generated execution bundles preserve the store and artifact identity so a later
quarantine, rollback or activation is detected before fresh execution. Explicit
policy files override store selection when a command accepts both; an execution
definition naming both is refused with `policy.composition.denied`. A change of
authority source also changes the CLI approval digest.

## Authenticated capability evidence

A host observation's digest checks its bytes; `kind: host_runtime` alone is not
execution evidence. Every non-`unknown` capability requires an explicit
`CapabilityProofVerifier` at the shared `ValidatedHostObservation` boundary.
Without one, a handwritten `supported` or `unsupported` claim fails with
`capability.evidence.unauthenticated`. Model discovery and capability validation
remain separate. A model cache or native stream never proves tools or writes.

Codex model discovery compares the cache's whole `major.minor.patch` version
with the selected CLI version. A valid prerelease or build suffix can match
that whole version for discovery; a different release cannot. Supplying
`inspect --observed-version` also retains the exact caller-provided version in
an evidence source of kind `codex_cli_version`. That source is provenance only:
it cannot establish a signed native host, native asset availability or a
capability. Proof loading compares this full version exactly, and native
dispatch still checks the full version and executable digest independently.
Omitting the flag retains the existing cache-only observation shape. Reading
a shared cache does not prove which process, account or client surface wrote it.

For the October model pool, use `assets/catalogs/codex-core-v2.json` as the
explicit policy prior; its schema remains 1. See
[catalog policy and unrated entries](../../assets/catalogs/README.md).

For Claude Code, `assets/catalogs/claude-core-v1.json` is an operator-declared,
fully unrated policy prior (schema 1). It is not refreshable from a Claude
snapshot because that snapshot carries no efforts.

The v1 authenticated proof contract lives in `router-adapter-api`. It contains a
random store identity, increasing sequence, proof identity, capability, tier,
attempt outcome, model identity, nonce/effect digests and start/end/lifetime
fields. Its binding names the exact host snapshot and harness, binary digest,
observed CLI version, adapter/template, sandbox helper/wrapper/profile, OS,
relative write roots and network setting. Network evidence additionally names
`loopback_fetch`; it does not prove external egress, DNS or TLS. An `attested`
record cannot enter the capability map or set `supported` in this batch.

`capability-proofs.sqlite3` is a separate schema-1 database inside the private
runtime root. Its MAC key is derived from `approval.key` with HKDF-SHA256, using
the persistent random store identity as salt and
`agent-router/capability-proof/v1` as the derivation context. The approval key is
never used directly as the proof MAC key. The store accepts at most 4096 records
of at most 32768 bytes each; a snapshot still accepts at most 64 evidence sources.
Records append in sequence order. For one binding, capability and tier, the
latest completed attempt decides; a newer failure revokes an earlier success.
Future timestamps, expired records, changed pins and cross-store reuse fail
closed. The same-user attacker remains outside this protection: that identity
can read the key and alter its own store.

`inspect --store ROOT --execution-config FILE` reads existing proofs and hashes
the configured executables and compiled templates/profile. It does not execute
a host or probe a sandbox. It folds only unexpired `proven` successes from one
coherent binding. Without both arguments, inspection grants no capabilities.
`route-phase` accepts the same pair and checks the actual requested scope and
network setting. `prepare` manifests use `capability_store` and
`execution_config`; `run` uses its existing `--store` and `--sandbox` arguments.
`catalog` subcommands, `assets seal`, `index`, `search` and `refresh` accept
that explicit pair for authenticated snapshot or generated-catalog inputs.
`assets review` also checks the phase's actual scope. Legacy `route` has no
execution scope and continues to refuse unauthenticated claims, as does the
local API. Catalog and metadata operations verify the scope already bound to
the proof; they grant no execution authority and emit no key or execution
configuration. Historical replay does not validate the currently installed
executables and never authorizes a new run.

Each `generate-plan` node can name `capability_store` and `execution_config`;
the pair is required together and the configuration bytes enter the captured
source digest. Historical reconstruction authenticates the exact referenced
runtime proofs and their sequence horizon, then verifies the original proposal
and source hashes. Additional cache provenance is preserved but never becomes
runtime authority. The coordinator keeps proofs in `--store/runtime`, validates
each wave step against its own configuration, and uses run checkpoints only
for historical reconstruction. All executing steps retain the live dispatch gate.

These proof readers never create a proof database or an approval key. No production
canary issuer is supplied by this batch, so real host capabilities remain UNKNOWN.

A selected phase consuming proofs emits receipt schema 5. Its
`capability_evidence` map records each required capability's proof digest, tier,
observation time and expiry. Receipts with no consumed proofs retain their
existing version and omit the empty map. The runtime records those compact
values together with the decision clock and the highest sequence among all
validated snapshot proofs, including capabilities unused by the phase.
Historical reopen uses that recorded clock and sequence horizon; a later failed
attempt in the same second cannot rewrite the old decision. A new run uses the
live log and current clock. Immediately before spawning the main process, the
runtime checks the live proof again, after the native version probe and other
asynchronous preparation.

`replay --store ROOT` and `eval compare --store ROOT` can authenticate store
records. Without the store, replay reports `capability_evidence: unverified`,
downgrades capability claims to UNKNOWN for evaluation and records a hard
violation. This result is never execution authority. Historical proof validity
also never authorizes a new dispatch.

## Feedback late facts

`FeedbackInput` (schema 1) gains two optional keys, both omitted when absent so
an earlier event keeps its bytes and `event_hash`:

- `run_key_sha256`: 64 lowercase hex characters, the digest of `[run_id,
  definition_hash, approval_digest]` of the run summary. It is not in
  `FeedbackBinding`, which human and automated verdicts must share.
- `started_at_unix_seconds`: when the native process started; at most
  `observed_at_unix_seconds`.

A late fact (`FeedbackFact`, schema 1) has `run_key_sha256`,
`observed_at_unix_seconds` and `kind`, a tagged object with `type` of either:

- `verification`: `validation`, `objective` (`passed`, `failed` or `unknown`,
  not both `unknown`, `objective` `passed` only with `validation` `passed`) and
  `evidence_sha256`, the digest of the stored verification record. It is
  recorded only once the run is `verified` or `verification_failed`;
- `integration`: `outcome`, `applied` or `discarded`.

The stored `FeedbackFactEvent` adds `sequence`, `consent_revision`,
`recorded_at_unix_seconds` and `fact_hash` (domain `feedback-fact-v1`), with no
free text. There is one fact per run key and kind. `FeedbackExport` (schema 1)
lists them in `facts`, omitted when empty so an export without facts keeps its
`content_hash`. The store `user_version` is 2; a newer version is refused.

`verify`, `artifacts apply`, `artifacts discard` and `coordinate artifacts`
accept an optional `--config <sandbox.json>`, of which only `feedback_store` is
read. Their JSON output gains `feedback`: `{"recorded":true,"fact_hash":"..."}`
or `{"recorded":false,"reason":"..."}` with `not_requested`,
`collection_disabled`, `inconclusive`, `invalid_fact`, `already_recorded`,
`verification_record_unavailable` (only from `coordinate service`) or
`consent_changed_or_storage_error`. `already_recorded` means a fact with the
same content is stored, possibly under another `observed_at`; a different
content for the same run key and kind stays `consent_changed_or_storage_error`.
`coordinate service` adds a `feedback` array to its output: one
`{"run_id":...,"feedback":{...}}` per run its pass concluded, from the
`feedback_store` of that run's bundle sandbox configuration.
`agent-router run` and `coordinate run-wave` keep the same `feedback` shape for
the automated event, with `event_hash` in place of `fact_hash`.

`FeedbackMeasurements.usage` (optional, omitted when absent so earlier events keep
their bytes and `event_hash`) holds a `HostUsage`: `input_tokens`,
`cached_input_tokens`, `cache_write_input_tokens`, `output_tokens` and
`reasoning_output_tokens`, each `{"tokens": n, "provenance": "host_reported"}` or
`{"tokens": null, "provenance": "unknown"}`. Recording refuses a count above
`MAX_VALUE`, a provenance that disagrees with the presence of the count, cached
input or cache writes above input, cached input plus cache writes above input,
reasoning above output, and a `tokens` total that differs from the split's
total (input plus output). The sidecar
projection drops it.

`agent-router feedback --store <dir> --json report [--pricing <file>]` prints a
`FeedbackReport` (schema 3, with strict schema 1/2 output validation retained); the pricing file is a `PricingTable` (schema 1, unknown
keys refused, at most 64 models). It needs collection consent. The following
describes the default v1 pricing report; the qualified v2 pricing extension
and runtime reconciliation versions are listed above. Top-level keys:
`schema_version`, `generated_at_unix_seconds`, `event_set_hash`, `fact_set_hash`,
`automated_events`, `human_events`, `facts`, `unlinked`, `orphan_facts`, `superseded_events`, `pricing`
(`version`, `source`, `observed_at_unix_seconds`, or null), `groups`, `runs`,
`catalog_estimate_ratio` (always `{"value":null,"reason":...}`), `advisory`,
`interpretation` and `content_hash` (domain `feedback-report-v3`; historical schema 1/2 use
`feedback-report-v1` / `feedback-report-v2`, and schema 1 has no `runs`). Each group
sorted by `(model, effort, task_kind)` has `runs`, `accepted`, `rejected`,
`unknown`, `unlinked`, `human`, `accepted_rate` (counts, `point_micros`,
`lower_micros`, `upper_micros`, `level_micros` 950000, `prior` `jeffreys`, or null),
`integration`, the `tokens`, `latency_ms`, `attempts` and
`time_to_acceptance_seconds` percentiles (`samples`, `missing`, `p50`, `p90`) and
`cost` (`priced_runs`, `cost_unknown`, `runs_with_cache_writes_priced_as_input`, `total_microusd`, `per_accepted_microusd`,
`reason`, `known_subtotal_microusd`, `expected_attempts`, `observed_attempts`,
`priced_attempts`, `missing_attempts`, `conflicting_attempts`, `scope_unknown_runs`).
No float appears in the output. `advisory` is
`{"advisory":null,"reason":"learning_disabled"}` or `{"advisory":true,"suggestion":null,"reason":...}`.
`runs` contains at most 10,000 retained automated rows, in event sequence order.
Each row carries the original `binding`, optional `run_key_sha256`, observation
time, model/effort/task kind, `verdict` (`accepted`, `rejected`, `unknown`),
`transport`, nullable `failure`, nullable `usage`, `cost` and `evidence`. Usage preserves
the five `HostUsage` fields and their provenance. Evidence names `feedback_store`
as its `source`, the `feedback_event_hash`, and nullable `verification_fact_hash`
and `integration_fact_hash`. `runtime_evidence` and `native_external_evidence`
are always `unknown`: the report verifies neither runtime execution nor native
external sessions. Bindings are source claims, not freshly authenticated
receipts. Multiple rows may share a task hash; they are never merged by that hash.
Summary schema 2 (hash domain `feedback-summary-v2`) cites the schema 3 report hash.
Historical summary schema 1 retains its strict shape and `feedback-summary-v1` domain.

`measurements.usage_scope` optionally equals `per_attempt`; absence is omitted
from serialization, preserving v1 event bytes and hashes. It binds usage to the
one-based ordinal in `measurements.attempts`. Report pricing deduplicates marked
snapshots by `(run_key_sha256, ordinal)`. Usage or model conflicts for one ordinal
are UNKNOWN. Expected coverage is the maximum observed ordinal, using bounded
maps of stored events rather than allocating by that ordinal. The known subtotal
includes failed attempts; the complete total is null for missing or unpriced
attempts. Unmarked single attempts retain latest-snapshot pricing; unmarked
retries cannot claim a complete total, even alongside marked snapshots. A legacy
ordinal one contributes known usage but conflicts with differing marked usage.
Group attribution uses the latest run
snapshot, while each attempt is priced using its own model. Coverage does not
prove that every dispatch produced feedback or join separate escalated runs.

See [feedback](../feedback.md#feedback-report) for the join rules.

`feedback replay-export` prints a report (`schema_version` 1) and, without
`--output`, the sealed `ReplayRecord` under `record`. It introduces no new
record contract: the `ReplayInput` and `CaseLabel` schemas are unchanged. Report
keys: `input_hash`, `policy_id`, `executed_candidate_id`, `label`
(`basis`: `integration`, `human` or `human_and_integration`; `human_validated`;
`independent`, always false), `observations` (`usage` is `host_reported` or `unknown`; `elapsed_ms`
may be null), `replay_context` (`complete` or `without_asset_authority`),
`request_free_text_fields` and, with `--output`, `output`. Refusal codes:
`feedback.replay.unlabeled`, `feedback.replay.rejected`,
`feedback.replay.ambiguous`, `feedback.replay.candidate_outside_request`,
`feedback.replay.invalid`, `feedback.replay.output_exists_or_unavailable` and
`feedback.replay.output_write`.

## Catalog

Catalog files use a numeric `schema_version` and exact model identifiers. The
following is a complete valid catalog with one model and one skill.

The MVP accepts from 1 through 64 model rows (`MAX_CATALOG_MODELS`) and from 0
through 256 skill rows (`MAX_CATALOG_SKILLS`). An empty `models` list is
rejected with `catalog.models.empty`; over-limit model and skill lists are
rejected with `catalog.models.too_many` and `catalog.skills.too_many`.

```json
{
  "schema_version": 1,
  "version": "example-catalog-v1",
  "models": [
    {
      "id": "provider-exact-model-v1",
      "stages": ["plan", "execute", "verify"],
      "efforts": ["medium", "high"],
      "capabilities": ["tools"],
      "quality_micros": 800000,
      "cost_micros": 400000,
      "latency_micros": 300000,
      "source": "example"
    }
  ],
  "skills": [
    {
      "id": "example-skill",
      "description": "A trusted example instruction.",
      "task_kinds": ["implement"],
      "required_capabilities": ["tools"],
      "trust": "builtin"
    }
  ]
}
```

Aliases such as `sol` are rejected at the catalog boundary. Use the exact host
model identifier, such as `gpt-5.6-sol` in the built-in catalog.

## Route request

A route request contains the normalized task, a host snapshot, retrieved skill
candidates, and optional tri-state capability evidence. Omitted capability
evidence falls back to the snapshot: a listed capability is `supported` and an
unlisted capability is `unsupported`.

```json
{
  "task": {
    "objective": "Implement a bounded parser change",
    "kind": "implement",
    "risk": "medium",
    "required_capabilities": ["tools"],
    "acceptance": ["workspace tests pass"]
  },
  "host": {
    "id": "local-example",
    "harness": "codex",
    "supported_models": ["provider-exact-model-v1"],
    "supported_efforts": {
      "provider-exact-model-v1": ["medium", "high"]
    },
    "capabilities": ["tools"]
  },
  "skill_candidates": [
    {
      "skill": "example-skill",
      "source": "example-bm25-v1",
      "score_micros": 910000
    }
  ],
  "capability_knowledge": {
    "tools": "supported"
  }
}
```

`capability_knowledge` accepts `supported`, `unsupported`, and `unknown`.
Unknown required capability evidence is recorded in `missing_evidence`; it
excludes high-risk and critical candidates with
`host.capability.unknown_high_risk`.

Embedded `skill_candidates` are validated again at the authoritative router
boundary, regardless of their adapter. A request accepts at most 256 rows.
Every source must be non-empty and every integer score must be from 0 through
1,000,000. Duplicate valid skill IDs are allowed and contribute only their
highest score once.

## BM25 candidate document

The retrieval adapter accepts a separate versioned candidate document. Its
skill IDs must be unique, scores are integers from 0 through 1,000,000,
and the router attaches only catalog-compatible skills.

Limits: 1 MiB per document (`retrieval.document.too_large`), 512 candidates
(`retrieval.candidates.too_many`) and a source of at most 256 bytes
(`retrieval.source.too_long`). Unknown keys are refused (`retrieval.json.invalid`).
The one exception is a per-candidate `path`, which is accepted and never read:
skill bodies are not loaded here.

Only `builtin`, `trusted_local`, and `verified_third_party` catalog skills may
attach to a plan or improve utility. `untrusted` and `blocked` skills are
ignored by routing, even when retrieval returns them.

```json
{
  "schema_version": "1",
  "source": "example-bm25-v1",
  "candidates": [
    {
      "skill": "example-skill",
      "score_micros": 910000,
      "path": "/not-loaded/skills/example-skill/SKILL.md"
    }
  ]
}
```

## Selected decision

`route --json` returns a decision envelope. This is a complete valid selected
decision. The decision ID is deterministic for the serialized request, catalog
version, schema version, and policy version.

```json
{
  "schema_version": "1",
  "decision_id": "9ec65c5ab0f698d70e368d127e8c95af77dc6ab455bb9d2d9e01f4424e795ab4",
  "catalog_version": "example-catalog-v1",
  "policy_version": "stable-v1",
  "decision": {
    "selected": {
      "plan": {
        "preset": "plan-execute-verify",
        "stages": [
          {
            "kind": "plan",
            "model": "provider-exact-model-v1",
            "effort": "high",
            "skills": ["example-skill"]
          },
          {
            "kind": "execute",
            "model": "provider-exact-model-v1",
            "effort": "medium",
            "skills": ["example-skill"]
          },
          {
            "kind": "verify",
            "model": "provider-exact-model-v1",
            "effort": "high",
            "skills": ["example-skill"],
            "verification_independence": "fresh_instance"
          }
        ],
        "utility_micros": 145500
      },
      "alternatives": [],
      "exclusions": [],
      "missing_evidence": []
    }
  }
}
```

## No-route decision

An incompatible request returns JSON as well. It has no partial plan and is
distinguished by the `no_route` variant.

```json
{
  "schema_version": "1",
  "decision_id": "3e8f8314a5f4a565ae45587a5b1d2d0186d44046f10135e2bbc6b0767ccaff8e",
  "catalog_version": "example-catalog-v1",
  "policy_version": "stable-v1",
  "decision": {
    "no_route": {
      "exclusions": [
        {
          "stage": "plan",
          "model": "provider-exact-model-v1",
          "code": "host.capability.unknown_high_risk",
          "evidence": "host 'local-example' has unknown support for capability 'tools'"
        }
      ],
      "missing_evidence": [
        "host 'local-example' has unknown support for capability 'tools'"
      ]
    }
  }
}
```

## Advisory decision cache entry

`router-advisory-cache` writes one private file per key, named
`<64 hex>.json`. Fields: `schema_version` (1), `key`, `created_unix`,
`expires_unix`, `proposal` (a `BackendProposal`), `mac` (hex HMAC-SHA256 over the
length-prefixed key, both timestamps and the serialized proposal). Unknown
fields are rejected. The entry is advisory: it names an alias that the caller
must resolve through a fresh `BoundedDecisionContext`, and it never contains
task text, prompts, receipts or grants. Any invalid entry is a cache miss. See
`docs/adr/0011-advisory-decision-cache.md`.

## Deterministic ordering

For identical request and catalog bytes, routing emits byte-identical JSON.
Exclusions sort by stage, model ID, code, then evidence. Plans sort by utility
descending, total cost ascending, then the lexical sequence of stage model IDs.
At most two alternatives follow the selected plan. Skills are de-duplicated by
skill ID and preserve the highest candidate score before one binding is emitted
per compatible stage.

The validated catalog limit bounds the complete candidate product at 64^3, or
262,144, plan-execute-verify combinations. The router scores that complete
bounded product through a streaming accumulator that holds at most four routes
temporarily and retains exactly the best three: one selected plan and at most
two alternatives. It never materializes the full Cartesian product. Identical
exclusions are deduplicated after sorting by stage, model ID, code, and
evidence.

Before entering the three combination loops, the router applies the public
`Router::MAX_PLAN_WORK_UNITS` ceiling of 1,000,000. It computes the estimate
with checked integer arithmetic:

```text
combinations = plan_candidates * execute_candidates * verify_candidates
payload_per_combination = 1
  + max_plan_candidate_skills
  + max_execute_candidate_skills
  + max_verify_candidate_skills
work_units = combinations * payload_per_combination
```

An overflow or a value above the ceiling fails immediately with
`route.resource.work_limit_exceeded`, before any plan is scored or cloned. The
error records `actual` when the checked result is representable and always
records `max`. It is invalid input, so the CLI emits the opaque
`request.invalid` line and exits `2`. The model, skill, candidate, and file
limits remain independently enforced, but satisfying each individual limit
does not guarantee acceptance when their combined work exceeds this budget.

The CLI reads inputs through bounded streams before parsing: route requests are
limited to 1 MiB, catalogs to 4 MiB, and stored decisions to 8 MiB. Oversized
files fail with `request.too_large`, `catalog.too_large`, or
`decision.too_large`.

## CLI exit codes

These codes apply to both human and JSON output. Product commands use `0` for
a successful operation and `3` for a valid request requiring clarification or
abstaining. A display limit never changes a completed operation's exit status.
See [CLI output](../cli-output.md) for bounded human views, schema 2 task
normalization/plan envelopes and historical compatibility, and
[retrieval](../retrieval.md) for the contextual metadata schema 3.

| Code | Meaning |
| --- | --- |
| `0` | A compatible route was selected. |
| `2` | Command syntax, file limits, request semantics, input JSON, catalog validation or a refusal failed, including input that drifted after approval (`*.changed`). |
| `3` | The request was valid but produced `no_route`. |
| `4` | An internal serialization, storage or invariant failure occurred, including a failed write of the result to stdout (for example a closed pipe), reported as `route.internal`. |
| `5` | Transient contention (`*.busy`, `*.locked`); the same command may be relaunched. |
| `6` | The outcome is unknown (`backend_store.published_durability_unknown`, `runtime.approval_key.published_durability_unknown`, `runtime.cleanup.unknown`); inspect before retrying. |
| `7` | An integrity check failed: a stored record, a hash chain or a signature no longer matches (`*.integrity`, `*.storage_corrupt` and a short explicit list). |

The status is derived from the stable error code alone; the complete table and
its matching rules are in
[exit codes and error codes](../cli-output.md#exit-codes-and-error-codes).
`explain` refuses a decision whose `schema_version` is not `"1"` with
`decision.schema.unsupported` (exit `2`) instead of rendering it.

Every command, not only `route`, exits `4` with `error: route.internal` (with
`--json`: `{"schema_version":1,"error":"route.internal"}` on stderr) when
writing its result to stdout fails (for example a closed pipe). With `--json`,
every error is that single JSON line on stderr, with the same exit status; the
local API uses the same body and field order. The write
failure is reported instead of a panic or a success status. For commands that
persist state, such as `run`, the persisted result is unaffected: the failed
write only loses the printed copy, so read it back with `status` or `events`.

## Operator attestations (ADR0021, batch 9)

A local declaration authenticates its stored bytes, not the operator's identity
or a native effect. `capability attest --store ROOT --request FILE --json`
reads a strict request with `schema_version: 1`, `execution_config`,
`host_snapshot`, `capability`, relative `scope`, `network`, `declared_version`,
`operator_label`, `reason`, and `max_age_seconds` (1 through 86400). Paths in
that request resolve against the request file. The issuer validates the
snapshot and hashes the actual executable, adapter template and sandbox
configuration without spawning a program. The identity is a declared label;
the reason is bounded to 2048 bytes. The version is explicitly declared and
must still match the bounded native version observation before dispatch.

The explicit issuing operation may create a private store and approval key or
atomically migrate capability database schema 1 to schema 2. Read-only consumers
never create or migrate either. The original canary rows remain byte-identical.
A schema 2 declaration payload omits model, nonce and effect fields, and uses a
separate HKDF context, `agent-router/capability-attestation/v1`. Canary and
attestation records share the authenticated store identity and append sequence.
Old schema 1 records labelled `attested` are not accepted as declarations.

`capability revoke --store ROOT --record-id ID --operator-label LABEL --reason
TEXT --json` appends a revocation referencing an existing authenticated grant.
It reads neither the old executable nor its execution configuration. Revocation
has no expiry. For the same capability and execution binding, the latest
declaration is chosen before checking its time and action; a future record,
expired grant or revocation cannot resurrect an older grant. Historical reads
use the recorded sequence horizon and decision clock. A valid proven canary
wins over an attestation. An attestation must be newer than every failed canary
for the same binding, even when a later successful canary has since expired.

Both commands return `schema_version: 1`, `tier: "attested"`, `record_id`,
`sequence`, `content_sha256`, `capability`, `action` (`grant` or `revoke`),
`created_at_unix_seconds`, nullable `expires_at_unix_seconds`, and
`executed: false`. They return no snapshot. `inspect` with explicit `--store`
and `--execution-config` folds valid declaration references into a snapshot 2
field `attestations: {"schema_version": 1, "entries": [...]}`. Each entry has
`record_id`, `content_sha256`, `capability` and `created_at_unix_seconds`.
`host.capabilities` excludes attested capabilities and their support remains
`unknown`.

Phase policy schema 3 adds the explicit `accepted_attestations` capability
list, empty by default. Schema 2 cannot opt in and keeps its serialized bytes.
Shared policy and execution gates require both a valid reference and opt-in;
legacy execution APIs remain proven-only. The policy hash and retained evidence
enter the approval binding. Changed selected evidence during the native version
probe is refused by the live guard immediately before main-process dispatch.
The guard freezes the evidence actually selected, not every intervening log row.
A later successful canary that has already expired does not displace the same
still-valid grant if no failure, revocation or newer declaration intervened.
A still-valid proven canary changes the selected tier and requires fresh approval.
Receipts consuming an attestation use schema 6 with tier `attested`; schemas 2
through 5 retain their domains and meaning. Replay without its authenticated
store reports unverified evidence and cannot authorize execution.

Network attestations are explicitly refused by the issuer and shared verifier,
and cannot pass policy or execution acceptance even with a valid MAC and
approval. This batch provides no provider-egress broker or admission authority.
No native capability or provider admission is established by these declarations.

## Accepted changes input and report

`eval accepted-changes --input FILE --json` has a separate input and output
schema version 1, owned and validated by `router-feedback`. Existing feedback
report/summary schemas and stores are unchanged. The standalone formats are
not registered in `agent-router schema`. See [ADR 0024](../adr/0024-accepted-revision-accounting.md)
for the strict fields, bounded parsing, content-hash domains, physical identity,
revision bindings, frontiers and explicit completeness rules, and the
[usage guide](../accepted-changes.md) for the synthetic example.

`evidence_trust: supplied_unverified` is mandatory in the output. Validation
checks the supplied calculation and joins; it is not independent verification
of a provider, reviewer, invoice or patch. Cost categories and null totals
remain distinct from approved allocations, elapsed time and measured benefit.

## Offline evaluator and supplied pricing evidence

`eval evaluator-qualification --input FILE --json` accepts feature-owned schemas
1 and 2 in `router-eval`. Schema 2 requires explicit tuning case identity sets;
legacy tuning output hashes without case identities yield inconclusive
comparisons. The report echoes the input schema version. Supplied qualification
witnesses, required-case comparisons and retained verdicts do not authenticate an oracle
or authorize adoption. See [the qualification guide](../evaluator-qualification.md).

`eval supplied-usage-pricing --input FILE --json` uses a separate schema-1
contract owned by `router-feedback`. It applies qualified rates to supplied
per-physical-call usage and invokes the existing accepted-change calculator.
The full report can contain caller identifiers. Only its `--portable` projection
is an allowlisted export; it preserves unknown totals and marks prices as
estimates, not invoices. See [the pricing guide](../supplied-usage-pricing.md).

Both parse bounded strict JSON and retain supplied, unverified evidence.
They do not modify observation v2/v3, stores, replay formats or the public
`schema` registry. [ADR 0034](../adr/0034-offline-evaluation-evidence.md) records
the separation from native collection and execution authority.

## Private finite-client preparation

`FiniteClientManifest` is a library-only private schema-1 contract with strict
unknown-field rejection. It binds six file paths and SHA-256 values, task and host-snapshot IDs,
native version, private state and a retained broker endpoint/instance. It is not
a public observation, execution receipt, permission grant or CLI manifest.
`FiniteClientReview` separately binds the complete prepared request and its
omitted stdin bytes and validated concrete resource-helper arguments under
`finite-client-review-v2`. Generated credential-free Codex state is consumed by
the review owner. A private, serialization-only macOS launch preview refreshes
the pins and derives helper/wrapper/client and native arguments; it has no
process conversion or public CLI loader. Existing resource defaults remain
unchanged. The approved handle and validation guard are neither
serializable nor reconstructable from this JSON. Existing run approval and OS
sandbox checks remain required. See [ADR 0028](../adr/0028-finite-client-preparation.md)
for bounded inputs, authenticated proof binding and outstanding native limits.


Producer-owned finite-client preparation additionally retains copies of client,
native, helper, wrapper and profile. It binds their paths and inode identities
under `finite-client-copy-review-v2` and uses proof adapter
`finite-client-contract-v4`; manifest-only preparation stays v2.
The review also binds producer-owned startup state: a fresh installation slot
under private tmp, its retained regular-file and symlink identities and the
exact relative alias. The derived five-key environment includes
`CODEX_SQLITE_HOME` under that tmp. Changed startup slots and prior v2/v3
owned proofs refuse. See [ADR 0032](../adr/0032-finite-client-owned-startup.md).
The private launch preview's `read_only_assets` array lists client, native, helper, wrapper,
profile and the generated configuration, each as a path/hash pin. These entries
are requirements for a future sandbox, not evidence of write exclusion.
The preview derives invocation paths from the retained copies and rechecks them
before rendering. Native launch and OS admission remain unavailable; see
[ADR 0029](../adr/0029-finite-client-owned-assets.md).

The dedicated Linux entrance is separate from that legacy preview. It retains
its namespace executable, uses proof adapter `finite-client-contract-v5` and
requires a combined runtime approval, durable admission and live OS canaries.
Producer startup v2 additionally pins a credential-free operator catalogue for
the coded local transport. An opaque preapproved workspace may bind one existing
file for a private replacement and patch artifact. Finite runs cannot reopen as
generic publication handles, and finite writes reject integration authority.
No serialized type, database schema or ordinary execution digest changes.
See [ADR 0037](../adr/0037-qualified-local-finite-dispatch.md) for the exact
platform, evidence and remaining provider/benchmark/publication boundaries.

## Explicit finite artifact verification

The in-memory `FiniteClientVerifier` borrows the owning Runtime and is retained
from an approved finite run before dispatch. It reads the exact captured patch
only while Verifying with matching definition/approval and confirmed namespace
cleanup. Its explicit `VerificationRecord` uses the existing transaction and
independent-verifier policy; UNKNOWN remains UNKNOWN. It supplies no generic
execution/publication handle, JSON import, human verdict or resumption after
Runtime disposal. See [ADR 0043](../adr/0043-finite-artifact-verification.md).
The opt-in [public-score example](../bench/finite-public-evaluation-2026-10-21/README.md)
executes its embedded checker and records validation Passed with objective
UNKNOWN. Its local evidence files are not a new product JSON-import contract.


## Durable verification attestations

Each accepted Runtime verification now appends `verification.attested.v1` to
its existing private event chain. `detail` is a serialized `VerificationRecord`:
verifier, evidence digest, optional artifact digest, validation and objective.
The event envelope binds the run, sequence, state and preceding event hash. Its
2 KiB detail limit is unchanged. The current `verifications` projection,
attestation event, existing digest-only `verification.recorded` event and any
resulting state transition commit in one SQLite transaction. An append or
transition failure leaves no partial verdict or history.

A later permitted inconclusive-to-conclusive update retains every new full
attestation; refused updates append none. A valid chain tail is required before
the write. The existing full integrity check and reopening verify the retained
chain; incremental checks retain the trust limits of ADR 0018. Hash chaining
is local integrity evidence, not authenticated evaluator or human identity.

Older builds emitted only `verification.recorded` with an evidence digest.
When the current record differs from the latest retained full record (or none
is retained), a
`verification.legacy_snapshot.v1` event preserves the observed current
`VerificationRecord` before it is overwritten. That snapshot is recorded now;
it is not the original event or a reconstruction of lost historical verdicts.
No events are backfilled merely by reading or opening the store. Missing earlier
record contents remain UNKNOWN. Consumers must keep the snapshot distinct from
new attestations and must not infer a complete pre-upgrade history. This also
covers a legacy writer used between two new writers: the complete record is
compared, since the same evidence digest can accompany different outcomes.

Existing `Runtime::events` and run-event readers expose these additive events;
`verification_record` still returns the current projection. There is no new
store, schema migration, verifier-import path, human verdict or execution/
publication permission. The finite fixture's objective remains UNKNOWN until
an explicitly authorized verifier establishes it.

## Qualified local capture observation

`observe --schema-version 3` adds bounded local capture timing rows while v1 stays
its default and v2 remains unchanged. Known intervals require a qualified
`attempt.timing.v1` producer record joined to exact attempt evidence. Legacy
elapsed values do not qualify themselves. The contract, unknown reasons and
boundaries are in [ADR 0030](../adr/0030-qualified-local-capture-timing.md) and the
[observation guide](../run-observation.md#read-v3-local-capture-intervals).
No database schema, legacy evidence format or execution authority changes.

The nonserializable approved preparation additionally exposes a bounded
`probe_owned_resources` operation for trusted helper integration. It executes
only the retained helper's `--probe` with the reviewed resource arguments,
checks ownership and pins before/after, and returns no launch handle or proof.
This cannot be reconstructed from the launch-preview JSON. An applied-limit
probe is separate from OS enforcement, native admission and task containment.


## Finite context tools v4

An explicit `FiniteContextPolicy` has schema 1, `read_windows` and at most eight valid lower-case SHA-256 `terminal_argument_sha256` entries. Unknown fields are rejected. Its digest covers the complete validated policy. It grants no capability. The [v4 profile decision](../adr/0044-finite-context-profile.md) and [qualification status](../bench/rtk-context-native-qualification/README.md) distinguish preparation from observed Linux effects.

`ToolOwnerBoot` adds optional `context_policy`, `context_policy_sha256` and `catalog_sha256`. All are defaulted and omitted when absent. With policy, both digests must match the validated policy and the actual deterministic MCP tools array. The parent binds the whole bootstrap bytes, retained file identity, startup configuration and policy into the finite review. Live mismatches refuse dispatch. No worker field or environment variable can select this policy.

`LINUX_FINITE_TOOLS_CONTEXT_PROFILE_V4` is exactly `{"kind":"linux-isolated-finite-tools-context-v4","schema_version":1}` followed by LF. It consumes only schema-2 `ToolTerminal` with matching policy and catalogue digests. The existing `LINUX_FINITE_TOOLS_PROFILE_V3` bytes and schema-1 terminal remain exact without policy; no absent option is added to historical hash tuples. A v3 profile cannot claim v4 policy or terminal metadata.

`read_file` exposes paired integer `start_line`/`max_lines` only when enabled, each 1..32768. The optional `window` metadata binds the entire checked source; UTF-8, CRLF, whitespace and unterminated tails remain exact. Truncation removes window metadata and retains the existing budget stop.

A nonempty terminal allowlist advertises `read_output` and requires it in the Runtime request allowlist. Only listed complete `exec` results may receive `view` metadata for `terminal_sgr_v1`. Non-SGR ESC returns the exact result. The complete raw result remains session-scoped, at most eight entries and 32768 content bytes. Recall consumes an invocation and its full content bytes, without another process invocation or partial recovery. Content, JSON and escaped MCP-text bounds are checked before advertising a handle.

Optional journal `context` counters measure admitted raw, shown and recalled content plus full ToolResult JSON bytes, excluding the MCP envelope. They do not authenticate a process, a price or a model outcome. The Runtime still requires opaque producer state, matching approvals, fresh OS evidence and exact stopped-namespace ownership before observing or promoting output.

## Explicit private application resume

The versioned finite application resume review and context live with their
validating runtime library. [The command protocol](finite-application-resume.md)
requires the original admission binding and a new same-process approval on an
intact preimage. Context, original intent, resume intent and completion remain
separate append-only evidence; candidate bytes alone never imply completion.

### Finite application display envelopes

`FiniteApplicationPresentation` contains the existing `FiniteApplicationReview`
and an inline `diff`. `FiniteResumePresentation` contains the fresh resume
`review`, the persisted `original` review and that same artifact text. These are
display envelopes, not persisted intent formats. Their validation rereads the
captured artifact and checks the complete payload against current runtime state.
The existing artifact hash binds the exact displayed bytes into approval; no
second display digest or authority is introduced.

The finite CLI emits these fields before approval. JSON consumers hash decoded
UTF-8 bytes and render untrusted patch text as data. Limits and refusal behavior
are documented in [the resume protocol](finite-application-resume.md).
