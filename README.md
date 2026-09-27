# agent-router

Local-first universal router for agentic coding harnesses.

Agent Router selects and explains bounded plans across models, reasoning effort,
agents, skills, tools, sandbox policies and verification requirements.

## Status

The CLI supports deterministic routing, phase routing, passive host inspection,
versioned catalogs, BM25 retrieval, eight workflow presets, replay evaluation,
diagnostics and redacted audit export. Routing and planning commands are dry
runs. Optional loopback backends propose shadow decisions; they do not replace
the active deterministic receipt.

The development workspace also contains explicit preparation, durable execution,
private feedback and signed offline policy activation. Execution requires a
reviewed digest and live sandbox enforcement. Their presence is not a claim
that all roadmap acceptance criteria have passed.
See [the completion ledger](docs/completion.md) for outstanding gates, including
human validation of the evaluation corpus. No global installation is performed.

## Initial scope

The first vertical slice is deterministic and read-only:

- import a small built-in catalog;
- accept a normalized task request;
- retrieve candidate skills through an adapter contract;
- enforce hard compatibility constraints;
- select a `plan-execute-verify` plan;
- emit stable JSON for `route` and `explain`.

Version 1 route receipts remain compatible. New contracts add provenance and
explicit authority without changing that original command's default behavior.

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

Route one phase using the documented synthetic fixtures:

```sh
cargo run -p agent-router-cli -- route-phase \
  --context fixtures/examples/phase-request.json \
  --host-snapshot fixtures/examples/phase-snapshot.json \
  --catalog fixtures/examples/phase-catalog.json --now 100 --json
```

`--now 100` is for these fixtures only. Real host evidence expires and must be
refreshed. `inspect --host codex --json` reads the Codex model cache without
launching a model. Claude inspection takes an explicit native stream file and
its observation time; unknown effort support remains unknown.

Use `agent-router --help` and each subcommand's `--help` for the implemented
file-based interfaces. Commands require `--json` for stable machine output.
Missing task authority produces clarification fields and exit code 3.

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
- [Routing boundary decisions](docs/adr/0002-bounded-routing-pipeline.md)
- [Optional local backends](docs/backends.md)
- [Prepare an execution request](docs/preparation.md)
- [CLI execution and privacy controls](docs/cli-execution.md)
- [Durable runtime and sandbox evidence](docs/runtime.md)
- [Metadata and catalog binding](docs/retrieval.md)
- [Private feedback and offline policy](docs/feedback.md)
- [Replay and evaluation](crates/router-eval/README.md)
- [Source distribution](docs/distribution.md)
- [Completion and outstanding acceptance](docs/completion.md)
- `docs/superpowers/specs/2026-08-28-agent-router-core-design.md`
- `docs/specs/functional-specification.md`
- `docs/specs/technical-specification.md`
- `docs/research/repository-analysis.md`
- `docs/reference/json-contracts.md`

## Safety boundary

This repository has no global installation path. It does not modify
`~/.config/ai-agents`, `~/.codex`, `~/.claude`, hooks, plugins, or global
skills without a separately approved installation workflow.
