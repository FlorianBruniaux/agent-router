# Completion ledger

Scope authorized: implement the open roadmap and applicable functional/technical
specifications, verify each acceptance criterion, and merge passing pull requests.
Closing an issue is a result of evidence, not a substitute for delivery.

## Delivery sequence

| Work | Scope | Status | Evidence gate |
| --- | --- | --- | --- |
| #12 | Dual-host development harness | Complete | PR #13 merged at `2d2ebd4f12e34bbf4f75ade23b033951e23edd51`; native canaries and six CI jobs passed |
| #10 | Passive HostProbe and explicit snapshots | Complete | PR #14 merged at `363857d0010640ccd9fa8471a01123f0752bf8b3`; 249 local tests and six CI jobs passed |
| #9 | Provenance-bearing catalogs and aliases | Complete | PR #14 merged at `363857d0010640ccd9fa8471a01123f0752bf8b3`; 249 local tests and six CI jobs passed |
| #3 | Local BM25 metadata retrieval | Complete | PR #14 merged at `363857d0010640ccd9fa8471a01123f0752bf8b3`; 249 local tests and six CI jobs passed |
| #4 | Immutable phase routing and receipts | Complete | PR #14 merged at `363857d0010640ccd9fa8471a01123f0752bf8b3`; 249 local tests and six CI jobs passed |
| #5 | Replay, golden corpus, shadow evaluations | Software implemented; human gate open | 100 complete candidate replays, 200 traces, 60/20/20 split; all human labels false |
| #2 | Bounded optional semantic backends | Implemented in shadow only | Loopback timeout/schema/pool/confidence/fallback tests; no semantic production promotion |
| #7 | Thin Codex and Claude adapters | Complete within the issue scope | PR #15 merged at `8b36b07aff508410a764f8feeefc62eaa0512935`; native canaries use a fake host under real macOS/Linux isolation; actual model behavior remains UNKNOWN |
| #1 | Durable isolated execution | PR #15 merged; follow-up coordination and crash fix under validation | A real supervisor SIGKILL exposed missing process lifetime proof; issue reopened. Watchdog, durable leases and explicit group/namespace evidence now have targeted tests, including native macOS SIGKILL |
| #6 | Offline feedback and reversible policy | Complete within the issue scope | PR #15 merged with eight passing CI checks; consent-bound runtime recording, signed offline promotion and rollback tested; objective validation remains separate |
| #8 | Distribution, licenses, notices, SBOM | Packaging validated; final snapshot pending | Strict license inventory and Gitleaks pass; extracted integrations: 41 tests; extracted core CLI without default adapters: 30 tests |
| #11 | Roadmap acceptance | Pending | All dependencies and the gaps below have evidence/disposition |

## Specification coverage beyond issue titles

- FR-002: normalized objective, deliverables, scope, risk, sensitivity, budget,
  dependencies, allowed mutations and acceptance; visible assumptions and
  clarification on ambiguous authority.
- FR-003: bounded acyclic workflow planner with owner, scope, inputs/outputs,
  budget, stop rule and verification per node; eight documented presets.
- FR-008/009/018: full agent/skill/prompt/tool metadata, typed prompt variables,
  context budgets, separate executable permissions, provenance and asset checks.
- FR-014: explicit precedence for defaults/presets/offline policy/project/org/
  overrides, with monotone restrictions that cannot regrant prohibited actions.
- FR-017: catalog/index invalidation without overwriting user edits.
- FR-019/020: doctor and redacted audit export, including paths, permission,
  schema, drift, processes and evidence boundaries.
- FR-015/016: opt-out, retention, purge/export, versioned offline statistics and
  quarantine; correlations remain distinct from verified success.
- Additional surfaces: Copilot/OpenCode and provider bridge have separate
  conformance and capability status; a transport is not a workspace harness.
- CLI: catalog, plan, run, status, cancel, workflow, artifacts, feedback, replay,
  policy and doctor need observable behavior and versioned JSON.
- NFR: measure decision p95, retrieval on 10,000 assets and cold startup;
  verify private storage modes, bounded resources and decision trace IDs.

## Open implementation and observation gates

- Workflow instances isolate dependency satisfaction. The follow-up coordinator
  implements explicit waves, private declared outputs, immutable patch views for
  independent verification, generation budgets, durable controls and restart.
  Its library pipeline and public CLI lifecycle passed native macOS tests with
  fake hosts. Full snapshot and native Linux verification remain delivery gates.
- V3 model/provider/auth/context/format contracts and skill review manifests are
  implemented. CLI approval now requires this authority; the runtime rechecks the
  actual prepared bytes. Historical v1/v2 inputs remain readable. Evidence links
  and synthetic tests do not authenticate a provider or prove live model limits.
- The watchdog distinguishes process-group termination from Linux namespace
  termination. Detached descendants are outside the macOS group-only proof.
  Missing, corrupt or unresolved leases block recovery and retain writer locks.
- Deep diagnostics now check bounded component schemas, catalog/body bindings
  and SQLite integrity without writes. WAL requires a coherent snapshot; unsafe
  descriptor access remains unsupported. Host activation and authentication
  remain distinct observations, never inferred from file presence.
- The measured NFR-003 first-launch startup target is not met: 524.315 ms and
  620.105 ms were observed against a 150 ms requirement. Decision p95 up to
  0.476 ms and hot-index retrieval p95 up to 10.789 ms meet their measured
  boundaries. Index construction and uncontrolled OS caches are separate;
  see `docs/performance.md`. The follow-up startup investigation isolates most
  latency before main, including minimal Rust/C controls, without changing OS
  security settings or claiming that a compiler profile solves the target.
- Native macOS address-space enforcement is unsupported by the current helper.
  An explicit null limit is reported as not requested, never as enforced.
- Copilot/OpenCode/provider bridge contracts do not prove installed-host or HTTP
  execution. A real native model's identity, effort and objective outcome still
  require observed evidence.

Optional later vision: embeddings, daemon/API, graphical/TUI client, marketplace,
complex learned model and Windows runtime. Their absence does not block the
specified MVP, but is not evidence that every possible future feature exists.

## Acceptance that automated implementation cannot manufacture

The specifications require 100 cases with human-validated expected decisions and
a recall@5 threshold defined from that human corpus. Generated fixtures are
regression candidates until human validation is supplied. Semantic promotion
requires an observed held-out improvement; it remains disabled when that gate
is unavailable or fails. Runtime capability, identity and permission claims
require host evidence and remain UNKNOWN otherwise. No public publication is
implied by local packaging or merge authority.
