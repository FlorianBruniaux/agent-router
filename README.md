# agent-router

Local-first universal router for agentic coding harnesses.

The project will select and explain a complete execution plan across models, reasoning effort, agents, skills, prompts, tools, sandbox policies, verification, and host adapters.

## Status

Design and implementation planning. No runtime code or global integration is installed yet.

## Initial scope

The first vertical slice is deterministic and read-only:

- import a small built-in catalog;
- accept a normalized task request;
- retrieve candidate skills through an adapter contract;
- enforce hard compatibility constraints;
- select a `plan-execute-verify` plan;
- emit stable JSON for `route` and `explain`.

Codex and Claude integration, durable execution, worktrees, embeddings, feedback learning, and policy sidecars remain outside the first slice.

## Documentation

- `docs/superpowers/specs/2026-08-28-agent-router-core-design.md`
- `docs/specs/functional-specification.md`
- `docs/specs/technical-specification.md`
- `docs/research/repository-analysis.md`

## Safety boundary

This repository does not modify `~/.config/ai-agents`, `~/.codex`, `~/.claude`, hooks, plugins, or global skills without a separately approved installation workflow.
