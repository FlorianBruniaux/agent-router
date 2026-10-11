# Documentation index

English | [Français](README.fr.md)

Every guide, specification and reference page under `docs/`, grouped by what
you want to do. Pages marked (FR) are
written in French.

## Get started

[Current finalization plan](bench/finalization-plan-2026-10-07.md) (FR), 2026-10-07: delivery order, pass conditions, real-trial and publication boundaries. Planned work, not activation evidence.

[Verified publication checks](bench/verification-publication-hardening-2026-10-08.json), 2026-10-08: 24 local checks, retained negative regressions and independent review. Native composition and real-provider admission remain open.

| Page | What it covers |
| --- | --- |
| [Experimental onboarding](experimental-release.md) | 2026-10-07: source candidate, offline example and host evidence limits; publication not implied |
| [Installation](install.md), [Français](install.fr.md) | Pinned Cargo and local Homebrew channels, two binaries, prerequisites and scoped uninstall |
| [Distribution qualification](bench/distribution-qualification-2026-10-07.md) | 2026-10-07: current v2 artifacts and local channel evidence; no publication or native-provider claim |
| [Bounded build investigation](bench/reproducibility-investigation-2026-10-07.md) | Four matching historical-source builds; original mismatch and cause remain unresolved |
| [CLI output](cli-output.md) | Human and `--json` output, exit codes and error codes |
| [Configuration formats](configuration-formats.md) | Strict JSON, TOML and YAML configuration layers |
| [Guided task intake](task-guide.md) | Collect missing task facts without implied permissions |

## Route and plan

| Page | What it covers |
| --- | --- |
| [How skills are suggested](skill-selection.md) (FR) | Actors, metadata search, compatibility checks, body loading and offline example |
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
| [Accepted revision costs](accepted-changes.md) | 2026-10-07: offline exact-revision accounting, revocation and missing-cost coverage; supplied evidence only |
| [Attempt correlation](attempt-correlation.md) | 2026-10-10: offline B1 CLI and complete observe-v2 projection; supplied evidence, no native usage collection |
| [Evaluation](evaluation.md) | Replay, cohorts, trade-offs and independent review |
| [Router value protocol](bench/router-value-protocol.md) | Pre-registrable bench of the router against fixed arms |
| [Bench playbook](bench/playbook.md) | Rerunning and checking a bench campaign, campaign log and pilot results |
| [Agent Router self-dogfood](bench/self-dogfood-b1b-2026-10-10/README.md) | 2026-10-10: routed B1b implementation trial; see report status, no comparative savings claim |
| [M4 stop report](bench/measurement-m4-result-2026-10-10/README.md) (FR) | 279,563 host tokens, 0 worker assignments, coherent counters and late interruption |
| [M4 v2 measured result](bench/measurement-m4-v2-result-2026-10-11/README.md) (FR) | 156,434 host tokens, two candidates pass 21 local checks each, one exact patch rejected; human acceptance pending |
| [M4 v2 sealed protocol](bench/measurement-m4-v2/README.md) (FR) | Historical approval proposal, subsequently approved and executed; original seal preserved |
| [Original M4 protocol](bench/measurement-m4/README.md) (FR) | Historical sealed v1, preserved unchanged after the approved attempt stopped |
| [Measurement preparation M1–M3](bench/measurement-m123/README.md) (FR) | Campaign catalog, trace accounting and offline comparison rehearsal; no savings claim |
| [Calibration v2 candidate](bench/calibration-v2/README.md) (FR) | 2026-10-05: collection protocol and incomplete preparation template; no run authorization |
| [Owned-entry decision](bench/owned-entry-2026-10-07/README.md) | 2026-10-07: direct app-server and ACP stopped before activation; per-send control and behavioral parity remain unqualified |
| [Native Codex integration diagnostic](bench/native-integration-u02/README.md) | 2026-10-05: local app-server parameter and output evidence; automatic CLI integration not admitted |
| [Feedback](feedback.md) | Local consented feedback and offline policies |

## Diagnostics, distribution and security

| Page | What it covers |
| --- | --- |
| [Doctor](doctor.md) | Read-only diagnostics and redacted audit |
| [Native supervision diagnostic](bench/owned-supervision-feasibility-2026-10-08/README.md) | 2026-10-08: explicit no-provider diagnostic; current composition remains NO_GO, with broker identity, memory and cleanup limits |
| [Passive run observation](run-observation.md) | 2026-10-08: opt-in v2 durable attempt identities, requested selections and supervisor outcomes; v1 default preserved; native facts, timing and full F04b/F05a remain open |
| [Observation verification](bench/verification-observation-2026-10-07.json) | 21 selected checks passed: 1,633 workspace Rust results + 1 core, 290 Python; result-line counts, 42 native opt-in tests ignored; not full CI equivalence |
| [Observation v2 verification](bench/verification-observation-v2-2026-10-08.json) | 24 local checks across the full run and ADR-index/fixture-hash repairs, independent review and measurement compatibility; native facts remain unknown |
| [Startup-outage cancellation](bench/physical-send-feasibility-2026-10-07/outage-cancellation/README.md) | Broker absent before launch, correlated interrupt/terminal and physical cleanup, zero provider; startup-outage evidence; see the later dedicated-client qualification; global admission remains open |
| [Physical-send feasibility](bench/physical-send-feasibility-2026-10-07/README.md) | 2026-10-07: six native cases pass, broker absence times out; overall NO-GO, no real provider admission |
| [Dedicated Codex client qualification](bench/owned-entry-qualification-2026-10-08/README.md) | 2026-10-08: bounded direct-client choice for simulated patch work; three local native scenarios, no real-provider admission |
| [Owned-client verification](bench/verification-owned-client-2026-10-08.json) | Source-bound local gates, independent reviews and native diagnostic limits |
| [Distribution](distribution.md) | Source distributions and license evidence |
| [Binary distribution](binary-distribution.md) | Local native binary export |
| [Local release](release-local.md) | Verifiable local release recipe |
| [Release qualification, 2026-10-07](bench/release-qualification-2026-10-07.md) | Exact artifacts, local install/removal, release fix, security inventory and unresolved publication gates |
| [Dependency audit](dependency-audit.md) | RustSec audit of the locked dependencies |
| [Fuzzing](fuzzing.md) | Bounded parser fuzzing |
| [SQLite WAL fix](sqlite-wal-fix.md) | Bundled SQLite WAL-reset correction |
| [Performance](performance.md) | Local performance observations |
| [Startup investigation](startup-investigation.md) | First-launch time analysis |

## Reference and project status

| Page | What it covers |
| --- | --- |
| [JSON contracts](reference/json-contracts.md) | Versioned input and output contracts, trust rules |
| [Architecture decisions](adr/README.md) | ADR decisions, with frontmatter and index |
| [Completion ledger](completion.md) | What is delivered, with evidence |
| [Remaining acceptance](remaining-acceptance.md) (FR) | Open requirements and proof gates |
| [Functional specification](specs/functional-specification.md) (FR) | Product requirements |
| [Technical specification](specs/technical-specification.md) (FR) | Technical design |
| [Comparison with similar projects](comparison.md) | Sourced comparison with other routers and coding-agent tools |
| [Omnigent comparison](omnigent-comparison.md) (FR) | Comparison and derived roadmap items |
| [Checkpoint 2026-09-27](checkpoints/2026-09-27-paused-work.md) | Paused work snapshot |

## Contributing

| Page | What it covers |
| --- | --- |
| [Development harness](harness.md) | Project agents, skills and hooks for Claude Code and Codex |
| [Local verification](local-ci.md) | Repeatable local checks, retained evidence and gaps when Actions is unavailable |
| [Harness evidence](harness-evidence.md) | Validation evidence for the harness |
| [Review grid](review-grid.md) | Checklist used for code review |
