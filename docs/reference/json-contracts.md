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
| Host observation | v1, full content digest, source provenance, observation time and tri-state capabilities |
| Generated catalog | v2 envelope, source snapshot/prior/registry hashes, validated aliases and deprecation |
| Phase route | v2 request and receipt, full context/catalog/policy/pool/registry hashes; missing permission fails closed |
| Retrieval | v1 unbound metadata or v2 envelope bound to exact catalog content; v2 operations require the matching digest |
| Workflow | v1 normalized task and hashed acyclic plan, scope and downstream independent verification checks |
| Preparation | v1 manifest and redacted preview; optional private request bundle, no automatic approval |
| Execution | v1 request, private persisted definition and approval; observed transport stays separate from objective verification |
| Evaluation | replay inputs, independently audited traces and explicit human-label status; no gain inferred from proposed labels |
| Feedback | v1 private consent/events (optional `run_key_sha256`, `started_at_unix_seconds` and `measurements.usage`), schema 2 store with late facts, a read-only `report`, signed policy changes with externally trusted keys; no online exploration |
| Runtime doctor report | v2 (`agent-router doctor --runtime-root`): `database`, `storage_schema_version`, `counts` and `leases` nullable, `partial_reason` added, `schema_layout` may be `not_inspected`; a partial report has `overall` `unknown` and exit 3. See `docs/doctor.md` |
| Modality input and JSON output | v1 `InputContract` (image, audio, file; canonical media-type allow-list, size bound, expected sha256) and `JsonOutputContract` (schema digest, size bound); unknown and duplicate keys rejected. Pure byte checks: the media type is declared, not read from content, digests come from the caller, output is not schema-validated, and nothing is transported. Execution stays refused while `ExecutionRequest` carries text only |

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
`collection_disabled`, `inconclusive`, `invalid_fact` or `consent_changed_or_storage_error`.
`agent-router run` and `coordinate run-wave` keep the same `feedback` shape for
the automated event, with `event_hash` in place of `fact_hash`.

`FeedbackMeasurements.usage` (optional, omitted when absent so earlier events keep
their bytes and `event_hash`) holds a `HostUsage`: `input_tokens`,
`cached_input_tokens`, `cache_write_input_tokens`, `output_tokens` and
`reasoning_output_tokens`, each `{"tokens": n, "provenance": "host_reported"}` or
`{"tokens": null, "provenance": "unknown"}`. Recording refuses a count above
`MAX_VALUE`, a provenance that disagrees with the presence of the count, cached
input or cache writes above input, and reasoning above output. The sidecar
projection drops it.

`agent-router feedback --store <dir> --json report [--pricing <file>]` prints a
`FeedbackReport` (schema 1); the pricing file is a `PricingTable` (schema 1, unknown
keys refused, at most 64 models). It needs collection consent. Top-level keys:
`schema_version`, `generated_at_unix_seconds`, `event_set_hash`, `fact_set_hash`,
`automated_events`, `human_events`, `facts`, `unlinked`, `orphan_facts`, `pricing`
(`version`, `source`, `observed_at_unix_seconds`, or null), `groups`,
`catalog_estimate_ratio` (always `{"value":null,"reason":...}`), `advisory`,
`interpretation` and `content_hash` (domain `feedback-report-v1`). Each group
sorted by `(model, effort, task_kind)` has `runs`, `accepted`, `rejected`,
`unknown`, `unlinked`, `human`, `accepted_rate` (counts, `point_micros`,
`lower_micros`, `upper_micros`, `level_micros` 950000, `prior` `jeffreys`, or null),
`integration`, the `tokens`, `latency_ms`, `attempts` and
`time_to_acceptance_seconds` percentiles (`samples`, `missing`, `p50`, `p90`) and
`cost` (`priced_runs`, `cost_unknown`, `total_microusd`, `per_accepted_microusd`,
`reason`). No float appears in the output. `advisory` is
`{"advisory":null,"reason":"learning_disabled"}` or `{"advisory":true,"suggestion":null,"reason":...}`.
See [feedback](../feedback.md#feedback-report) for the join rules.

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
