# agent-router

Local-first universal router for agentic coding harnesses.

The project will select and explain a complete execution plan across models, reasoning effort, agents, skills, prompts, tools, sandbox policies, verification, and host adapters.

## Status

Dry-run routing only. The binary deterministically selects or rejects a route;
it does not execute a host, call a model provider, create a worktree, or install
anything globally.

## Initial scope

The first vertical slice is deterministic and read-only:

- import a small built-in catalog;
- accept a normalized task request;
- retrieve candidate skills through an adapter contract;
- enforce hard compatibility constraints;
- select a `plan-execute-verify` plan;
- emit stable JSON for `route` and `explain`.

Codex and Claude integration, durable execution, worktrees, embeddings, feedback learning, and policy sidecars remain outside the first slice.

## Quickstart

Build the workspace:

```bash
cargo build --workspace
```

Run the full test suite:

```bash
cargo test --workspace
```

Route a normalized request against an explicit catalog. The command only emits
JSON to standard output:

```bash
cargo run -p agent-router-cli -- route \
  --request tests/fixtures/requests/codex-implementation.json \
  --catalog assets/catalogs/codex-core-v1.json \
  --json
```

Save that decision and explain it without re-routing or executing anything:

```bash
cargo run -p agent-router-cli -- route \
  --request tests/fixtures/requests/codex-implementation.json \
  --catalog assets/catalogs/codex-core-v1.json \
  --json > /tmp/agent-router-decision.json

cargo run -p agent-router-cli -- explain \
  --decision /tmp/agent-router-decision.json
```

`route` exits `0` for a selected route, `3` for a valid `no_route` decision,
`2` for invalid command input or catalog data, and `4` for an internal error.

## Resource and trust boundaries

Catalogs contain at most 64 models and 256 skills. Routing evaluates the
bounded 64^3 stage combination space without materializing it, retains only the
best three routes, and deduplicates identical exclusions. Before scoring, it
also enforces `Router::MAX_PLAN_WORK_UNITS = 1,000,000` over the combined model
and attached-skill payload. Inputs within each individual limit can therefore
still be rejected when their combined routing work is too large. The CLI reads
at most 1 MiB for requests, 4 MiB for catalogs, and 8 MiB for decisions before
rejecting an oversized file.

Route requests contain at most 256 skill candidates with non-empty sources and
scores from 0 through 1,000,000. Only built-in, trusted-local, and verified
third-party skills can attach or improve utility. Untrusted and blocked skills
have no routing effect.

## Documentation

- [Development harness](docs/harness.md): shared Codex and Claude Code project
  instructions, agents, skills, hooks and validation, separate from the product.
- `docs/superpowers/specs/2026-08-28-agent-router-core-design.md`
- `docs/specs/functional-specification.md`
- `docs/specs/technical-specification.md`
- `docs/research/repository-analysis.md`
- `docs/reference/json-contracts.md`

## Safety boundary

This repository has no global installation path. It does not modify
`~/.config/ai-agents`, `~/.codex`, `~/.claude`, hooks, plugins, or global
skills without a separately approved installation workflow.
