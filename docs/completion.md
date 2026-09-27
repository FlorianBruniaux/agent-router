# Completion ledger

Scope authorized: implement the open roadmap and applicable functional/technical
specifications, verify each acceptance criterion, and merge passing pull requests.
Closing an issue is a result of evidence, not a substitute for delivery.

## Delivery sequence

| Work | Scope | Status | Evidence gate |
| --- | --- | --- | --- |
| #12 | Dual-host development harness | Complete | PR #13 merged at `2d2ebd4f12e34bbf4f75ade23b033951e23edd51`; native canaries and six CI jobs passed |
| #10 | Passive HostProbe and explicit snapshots | Implemented, PR validation | Passive cache/stream fixtures, hash and freshness checks; unknown fails closed |
| #9 | Provenance-bearing catalogs and aliases | Implemented, PR validation | Content/provenance hashes, alias/deprecation/conflict fixtures |
| #3 | Local BM25 metadata retrieval | Implemented, PR validation | Positive/negative/ambiguous/empty/security fixtures; bounded 10k benchmark |
| #4 | Immutable phase routing and receipts | Implemented, PR validation | Full-content identity, constraints before score, bounded pool; preference and verifier-order regressions passing |
| #5 | Replay, golden corpus, shadow evaluations | Software implemented; human gate open | 100 complete candidate replays, 200 traces, 60/20/20 split; all human labels false |
| #2 | Bounded optional semantic backends | Implemented in shadow only | Loopback timeout/schema/pool/confidence/fallback tests; no semantic production promotion |
| #7 | Thin Codex and Claude adapters | Conformance implemented | Native argv/stream tests; end-to-end supervised native execution still open |
| #1 | Durable isolated execution | Library implemented; CLI integration in progress | 25 library tests and two native macOS sandbox canaries; Linux CI proof pending |
| #6 | Offline feedback and reversible policy | Software implemented; runtime integration open | 18 tests, signed exact-digest promotion/rollback, retention clock regression independently verified |
| #8 | Distribution, licenses, notices, SBOM | Packaging validated; final snapshot pending | Reproducible exports, strict license scan, Gitleaks, extracted offline tests |
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
