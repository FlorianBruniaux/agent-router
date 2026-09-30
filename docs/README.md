# Documentation index

English | [Français](README.fr.md)

Every guide, specification and reference page under `docs/`, grouped by what
you want to do. Pages marked (FR) are
written in French.

## Get started

| Page | What it covers |
| --- | --- |
| [Installation](install.md) | Prerequisites, build features, local release, uninstall |
| [CLI output](cli-output.md) | Human and `--json` output, exit codes and error codes |
| [Configuration formats](configuration-formats.md) | Strict JSON, TOML and YAML configuration layers |
| [Guided task intake](task-guide.md) | Collect missing task facts without implied permissions |

## Route and plan

| Page | What it covers |
| --- | --- |
| [Retrieval](retrieval.md) | Catalog search, contextual signals, rank fusion, local vectors |
| [Risk classification](risk-classification.md) | Deterministic risk floor |
| [Plan generation](plan-generation.md) | Bounded global plan assignments under budgets |
| [Plan proposals](plan-proposals.md) | Reviewing a plan proposed by a model |
| [Review separation](review-separation.md) | Producer and reviewer on distinct providers |
| [Optional decision backends](backends.md) | Semantic proposals kept inside the admissible pool |
| [Standard sidecar client](standard-sidecar.md) | Local ranking sidecar protocol and consent |
| [Local route-only API](local-api.md) | `agent-router serve` over a private Unix socket |

## Catalogs and trust

| Page | What it covers |
| --- | --- |
| [Signed catalogs](signed-catalogs.md) | Signed common catalogs and provenance rules |
| [Current catalog authority](catalog-store.md) | Persistent current catalog selection and revocation |
| [Signed routing](signed-routing.md) | Signed catalog constraints before scoring |
| [Signed preparation](signed-preparation.md) | Catalog and grant revalidation around assembly |
| [Publisher policy guards](catalog-active-cli.md) | Library guards; there is no `catalog-active` CLI |
| [Asset contracts](asset-contracts.md) | Bound asset contracts and gates |
| [Inert importer](importer.md) | Markdown agents and skills to an unsigned candidate |

## Execute and verify

| Page | What it covers |
| --- | --- |
| [Preparation](preparation.md) | Preparing a workflow node |
| [CLI execution](cli-execution.md) | Approve, run, verify, retain and purge outputs |
| [Runtime](runtime.md) | Sandboxed execution, crash recovery, platform limits |
| [Coordinator](coordinator.md) | Durable workflow waves and artifact handoff |
| [Escalation](escalation.md) | Escalating from durable attempts |
| [Shared admission](admission.md) | Concurrency reserved across runtime owners |
| [Session budgets](session-budgets.md) | Conservative cost and token allocations |
| [Structured validation](structured-validation.md) | Typed validation contracts and reports |
| [Other hosts](other-hosts.md) | OpenCode and Copilot supervised transports |
| [Provider transport](provider-transport.md) | Loopback provider transport, single-user hosts only |

## Backends, evaluation and feedback

| Page | What it covers |
| --- | --- |
| [Backend promotion](backend-promotion.md) | Signed promotion evidence and gates |
| [Backend activation store](backend-store.md) | Durable activation, rollback and revocation |
| [Backend execution](backend-execution.md) | Executing a signed backend decision |
| [Evaluation](evaluation.md) | Replay, cohorts, trade-offs and independent review |
| [Router value protocol](bench/router-value-protocol.md) | Pre-registrable bench of the router against fixed arms |
| [Feedback](feedback.md) | Local consented feedback and offline policies |

## Diagnostics, distribution and security

| Page | What it covers |
| --- | --- |
| [Doctor](doctor.md) | Read-only diagnostics and redacted audit |
| [Distribution](distribution.md) | Source distributions and license evidence |
| [Binary distribution](binary-distribution.md) | Local native binary export |
| [Local release](release-local.md) | Verifiable local release recipe |
| [Dependency audit](dependency-audit.md) | RustSec audit of the locked dependencies |
| [Fuzzing](fuzzing.md) | Bounded parser fuzzing |
| [SQLite WAL fix](sqlite-wal-fix.md) | Bundled SQLite WAL-reset correction |
| [Performance](performance.md) | Local performance observations |
| [Startup investigation](startup-investigation.md) | First-launch time analysis |

## Reference and project status

| Page | What it covers |
| --- | --- |
| [JSON contracts](reference/json-contracts.md) | Versioned input and output contracts, trust rules |
| [Architecture decisions](adr/README.md) | ADR 0001 to 0019, with frontmatter and index |
| [Completion ledger](completion.md) | What is delivered, with evidence |
| [Remaining acceptance](remaining-acceptance.md) (FR) | Open requirements and proof gates |
| [Functional specification](specs/functional-specification.md) (FR) | Product requirements |
| [Technical specification](specs/technical-specification.md) (FR) | Technical design |
| [Omnigent comparison](omnigent-comparison.md) (FR) | Comparison and derived roadmap items |
| [Checkpoint 2026-09-27](checkpoints/2026-09-27-paused-work.md) | Paused work snapshot |

## Contributing

| Page | What it covers |
| --- | --- |
| [Development harness](harness.md) | Project agents, skills and hooks for Claude Code and Codex |
| [Harness evidence](harness-evidence.md) | Validation evidence for the harness |
| [Review grid](review-grid.md) | Checklist used for code review |
