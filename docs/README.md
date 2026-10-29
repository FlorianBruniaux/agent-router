# Documentation index

[One-cell Linux campaign launcher](bench/finite-campaign-launcher-2026-10-18/README.md), 2026-10-18: sealed assignment, exact parent approval and bounded private output; local qualification is separate from real model trials.

[Finite-client namespace ownership](bench/linux-namespace-owner-2026-10-15/README.md), 2026-10-15: transfer startup, copies and broker into the existing locked namespace lifecycle. Selected Linux composition, 8 GiB unchanged; F12a NO_GO.

[Host-state writer exclusion](bench/host-state-writer-exclusion-2026-10-15.md), 2026-10-15: require the exact locked stop proof before disposal, preserve exposed state on Drop and quarantine incomplete filesystem cleanup across restart. 8 GiB unchanged; F12a NO_GO.

[Private host-state cleanup](bench/host-state-cleanup-2026-10-13.md), 2026-10-13: preserve substituted home/store paths, refuse ambiguous absence and prevent Drop retries. Local macOS tests; 8 GiB unchanged, F12a NO_GO.

[Producer-owned Linux startup](bench/linux-owned-startup-2026-10-11/README.md), 2026-10-11: bind installation/SQLite startup before review and qualify the same producer under unchanged 8 GiB. Diagnostic only, F12a NO_GO.

[Copied client in Linux isolation](bench/linux-copied-client-2026-10-11/README.md), 2026-10-11: pinned native client, broker port and protected-copy canaries at unchanged 8 GiB; detached activity checked across external namespace teardown. Diagnostic only, F12a NO_GO.

[Real helper in Linux isolation](bench/linux-helper-memory-2026-10-11/README.md), 2026-10-11: unchanged helper installs 8 GiB; direct/fork/exec allocation refusal and four cleanup cases pass. Diagnostic component only, F12a NO_GO.

[Strict 8 GiB startup follow-up](bench/macos-strict-memory-startup-2026-10-11/README.md), 2026-10-11: four loader controls still refuse the unchanged limit; exact 384 GiB geometry matches pinned XNU GPU reservation constants. Source-to-host attribution remains UNKNOWN, F12a NO_GO.

[Physical-memory experiment](bench/macos-memory-protection-2026-10-11/README.md), 2026-10-11: five direct-child observation/termination cases, one physical-limit API refusal, measured overshoot and preserved NO_GO. [Proposed monitoring contract](adr/0031-macos-physical-memory-observation.md).

[F12a owned resource preflight](bench/finite-client-qualification-2026-10-11/README.md), 2026-10-11: exact copied-helper refusal at 8 GiB, source-bound local checks and remaining native gates. NO_GO. [Independent macOS components](bench/macos-component-qualification-2026-10-11/README.md): 47 results and instrumented refusal evidence, without complete native qualification.

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
| [Offline evaluation verification](bench/offline-evaluation-evidence-2026-10-15.md) | Local contract tests, independent review and remaining measurement evidence |
| [Evaluator qualification](evaluator-qualification.md) | Required-case comparisons, supplied qualification witnesses and explicit inconclusive outcomes |
| [t02 oracle v2](bench/t02-oracle-v2-2026-10-22/README.md) | Versioned multi-implementation qualification and bounded authored-fixture attestation |
| [Frozen t02 oracle probe](bench/t02-oracle-qualification-2026-10-21/README.md) | Offline Linux evidence of a missed overflow defect, independent public labels and unchanged 8 GiB limits |
| [Supplied usage pricing](supplied-usage-pricing.md) | Offline per-physical usage bridge, estimated prices and an allowlisted portable projection |
| [Retained M4 acceptance evidence](bench/accepted-evidence-2026-10-15/README.md) | Three historical attempts, one accepted revision, unknown monetary totals; no new trial |
| [Evaluation](evaluation.md) | Replay, cohorts, trade-offs and independent review |
| [claude-router regression matrix](claude-router-regression-matrix.md) | 2026-10-28: T01 to T14 dispositions and offline tests; parent routing, cache, context fit and hysteresis stay deferred |
| [Router value protocol](bench/router-value-protocol.md) | Pre-registrable bench of the router against fixed arms |
| [Bench playbook](bench/playbook.md) | Rerunning and checking a bench campaign, campaign log and pilot results |
| [Agent Router self-dogfood](bench/self-dogfood-b1b-2026-10-10/README.md) | 2026-10-10: routed B1b implementation trial; see report status, no comparative savings claim |
| [M4 stop report](bench/measurement-m4-result-2026-10-10/README.md) (FR) | 279,563 host tokens, 0 worker assignments, coherent counters and late interruption |
| [M4 v2 measured result](bench/measurement-m4-v2-result-2026-10-11/README.md) (FR) | 156,434 host tokens; one unique change subsequently accepted and integrated via [#164 follow-up](bench/m4-fix-integration-2026-10-11.md); no savings or native qualification |
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
| [Passive run observation](run-observation.md) | 2026-10-28: opt-in v4 bounded timeline over recorded state (no timestamps); 2026-10-11: opt-in v3 local supervisor capture intervals, v2 durable attempt facts and v1 default preserved; native/provider durations and full F04b/F05a remain open |
| [Observation verification](bench/verification-observation-2026-10-07.json) | 21 selected checks passed: 1,633 workspace Rust results + 1 core, 290 Python; result-line counts, 42 native opt-in tests ignored; not full CI equivalence |
| [Observation v2 verification](bench/verification-observation-v2-2026-10-08.json) | 24 local checks across the full run and ADR-index/fixture-hash repairs, independent review and measurement compatibility; native facts remain unknown |
| [Local capture timing verification](bench/verification-local-capture-timing-2026-10-11.json) | Opt-in v3: seven local gates passed on combined source, explicit legacy unknowns and independent review; native/provider and end-to-end durations remain unqualified |
| [Finite-client assets review](bench/verification-finite-client-assets-review-2026-10-11.json) | PR #163 merged after independent post-fix review and local macOS/Linux checks; preparation only, F12a remains NO_GO |
| [Startup-outage cancellation](bench/physical-send-feasibility-2026-10-07/outage-cancellation/README.md) | Broker absent before launch, correlated interrupt/terminal and physical cleanup, zero provider; startup-outage evidence; see the later dedicated-client qualification; global admission remains open |
| [Physical-send feasibility](bench/physical-send-feasibility-2026-10-07/README.md) | 2026-10-07: six native cases pass, broker absence times out; overall NO-GO, no real provider admission |
| [Dedicated Codex client qualification](bench/owned-entry-qualification-2026-10-08/README.md) | 2026-10-08: bounded direct-client choice for simulated patch work; three local native scenarios, no real-provider admission |
| [Owned-client verification](bench/verification-owned-client-2026-10-08.json) | Source-bound local gates, independent reviews and native diagnostic limits |
| [Finite context profile v4](bench/rtk-context-native-qualification/README.md) | 2026-10-21: policy-bound preparation, schema-2 terminal and authored canary; native Linux ARM64 qualification NOT RUN |
| [Finite context offline corpus](bench/rtk-context-offline/README.md) | 2026-10-21: authored fidelity corpus, signed byte comparison and complete raw rereads; costs and model success unknown, native qualification NOT RUN |
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
