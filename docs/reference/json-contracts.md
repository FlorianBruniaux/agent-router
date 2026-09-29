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
| Feedback | v1 private consent/events, signed policy changes with externally trusted keys; no online exploration |
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
policy files override store selection. A change of authority source also changes
the CLI approval digest.

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
| `2` | Command syntax, file limits, request semantics, input JSON, or catalog validation failed. |
| `3` | The request was valid but produced `no_route`. |
| `4` | An internal serialization or invariant failure occurred, including a failed write of the result to stdout (for example a closed pipe), reported as `route.internal`. |

Every command, not only `route`, exits `4` with `error: route.internal` when
writing its result to stdout fails (for example a closed pipe). The write
failure is reported instead of a panic or a success status. For commands that
persist state, such as `run`, the persisted result is unaffected: the failed
write only loses the printed copy, so read it back with `status` or `events`.
