# JSON contracts

The first slice uses deterministic, versioned JSON. It only computes a route.
It does not execute a model, call a host API, create a worktree, or install
anything. A catalog or host configuration proves routing intent and declared
capabilities, not actual model execution.

## Catalog

Catalog files use a numeric `schema_version` and exact model identifiers. The
following is a complete valid catalog with one model and one skill.

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

## BM25 candidate document

The retrieval adapter accepts a separate versioned candidate document. Its
skill IDs must be unique, scores are integers from 0 through 1,000,000,
and the router attaches only catalog-compatible skills.

```json
{
  "schema_version": "1",
  "source": "example-bm25-v1",
  "candidates": [
    {
      "skill": "example-skill",
      "score_micros": 910000
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

## Deterministic ordering

For identical request and catalog bytes, routing emits byte-identical JSON.
Exclusions sort by stage, model ID, code, then evidence. Plans sort by utility
descending, total cost ascending, then the lexical sequence of stage model IDs.
At most two alternatives follow the selected plan. Skills are de-duplicated by
skill ID and preserve the highest candidate score before one binding is emitted
per compatible stage.

## CLI exit codes

| Code | Meaning |
| --- | --- |
| `0` | A compatible route was selected. |
| `2` | Command syntax, input JSON, or catalog validation failed. |
| `3` | The request was valid but produced `no_route`. |
| `4` | An internal serialization or invariant failure occurred. |
