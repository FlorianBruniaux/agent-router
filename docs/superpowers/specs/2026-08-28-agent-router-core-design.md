# Agent Router Core Design

Date: 2026-08-28

## Decision

Build a new Rust workspace named `agent-router`. The binary is the authoritative decision engine. Existing Claude and Codex skills, hooks, releases, and BM25 routing remain external inputs and host adapters.

## First vertical slice

The first slice proves one claim: given a normalized task, a host capability snapshot, a catalog, and skill candidates, the router can deterministically produce and explain a compatible `plan-execute-verify` plan.

It includes:

1. stable domain types for tasks, assets, constraints, candidates, plans, and decision traces;
2. a validated built-in catalog with exact model identifiers and compatibility metadata;
3. a skill retrieval port with an in-memory test adapter and a future adapter for the installed BM25 router;
4. hard constraints evaluated before scoring;
5. a deterministic plan builder and scorer;
6. CLI commands `route` and `explain` with versioned JSON;
7. fixtures covering compatible routes, exclusions, unknown capabilities, and overrides.

It excludes runtime execution, child processes, DAG persistence, worktrees, hooks installation, global configuration changes, embeddings, online learning, policy sidecars, and graphical interfaces.

## Architecture

```text
TaskRequest + HostSnapshot + Catalog + SkillCandidates
                       |
                       v
                  Normalizer
                       |
                       v
               Candidate Builder
                       |
                       v
               Hard Constraints
                       |
                       v
          Deterministic Plan Scoring
                       |
                       v
        RouteDecision + DecisionTrace JSON
```

The initial workspace contains four crates:

- `router-types`: serialization contracts and enums;
- `router-catalog`: catalog loading and validation;
- `router-policy`: constraints, candidate construction, scoring, and presets;
- `agent-router-cli`: command parsing and JSON rendering.

The dependency direction is CLI to policy and catalog, policy and catalog to types, and types to no internal crate.

## Existing BM25 boundary

The installed BM25 hook remains the current suggestion mechanism. The core defines a `SkillRetriever` port but does not rewrite the Node router. The first production adapter will consume its versioned output or catalog data. A future Tantivy implementation must pass parity fixtures before it can replace any existing path.

## Constraint semantics

Every constraint returns one of:

- `eligible`;
- `excluded` with a stable reason code and evidence;
- `unknown` with a stable reason code.

Unknown capability excludes a high-risk candidate. For low-risk work, policy may retain it only as an alternative requiring review. Scores never override exclusions.

## Deterministic preset

The built-in `plan-execute-verify` preset creates three logical stages:

1. plan with a high-capability planning model;
2. execute with a compatible implementation model;
3. verify with an independent reviewer model or instance.

The initial catalog uses exact IDs. Aliases such as `sol`, `terra`, and `luna` are rejected at the storage boundary and may be resolved only by an explicit alias table at input.

## Output contract

`route --json` returns:

- schema version;
- decision ID;
- task summary;
- selected plan and stages;
- estimated score components;
- exclusions and alternatives;
- policy and catalog versions;
- confidence and missing evidence.

`explain` renders the same stored or supplied decision in concise human-readable form. The first slice may explain a decision file before SQLite persistence is added.

## Error handling

Library crates expose typed errors. The CLI maps them to stable error codes and nonzero exit statuses. Invalid catalogs, unknown model aliases, missing required capabilities, malformed task input, and empty eligible sets are distinct errors.

No fallback selects the first catalog entry. The safe fallback is an explicit `no_route` decision with reasons.

## Testing

Implementation follows red-green-refactor. Tests cover:

- serialization round trips;
- catalog validation;
- exact ID and alias behavior;
- each hard constraint;
- the invariant that excluded candidates cannot win;
- deterministic results for identical snapshots;
- stable JSON golden fixtures;
- CLI success and failure exit codes.

Property tests and live host probes come after the first deterministic slice.

## Completion criteria

The slice is complete when:

1. all workspace tests, formatting, Clippy, and documentation checks pass;
2. at least twelve route fixtures cover Codex, Claude, unsupported, and unknown cases;
3. repeated routing of the same fixture produces byte-identical JSON except for explicitly injected IDs;
4. a high-scoring incompatible candidate is demonstrably excluded;
5. no files outside the repository are changed by tests or commands.
