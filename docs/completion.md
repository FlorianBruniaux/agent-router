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
| #2 | Bounded optional semantic backends | Complete within the issue scope | PR #19 merged at `2261511e57870fb69dff67a8b8d28df854155294`; 495 local tests, 17 native macOS canaries and eight PR checks passed. Human evidence gate remains closed for production |
| #7 | Thin Codex and Claude adapters | Complete within the issue scope | PR #15 merged at `8b36b07aff508410a764f8feeefc62eaa0512935`; native canaries use a fake host under real macOS/Linux isolation; actual model behavior remains UNKNOWN |
| #1 | Durable isolated execution | Complete within the issue scope | PR #16 merged at `dfe7d9346093bf3b6bec4d423706301d1b7f1825` after 411 local tests and eight PR checks; watchdog, durable leases, coordination and native macOS/Linux SIGKILL recovery covered. PR #17 stabilizes the historical-expiry test |
| #6 | Offline feedback and reversible policy | Complete within the issue scope | PR #15 merged with eight passing CI checks; consent-bound runtime recording, signed offline promotion and rollback tested; objective validation remains separate |
| #8 | Distribution, licenses, notices, SBOM | Packaging validated; final snapshot pending | Strict license inventory and Gitleaks pass on the PR #16 snapshot; extracted integrations: 41 tests; extracted core CLI without default adapters: 39 tests. Rebuild required for subsequent changes |
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
  Its library pipeline and public CLI lifecycle passed native macOS/Linux tests
  with fake hosts in PR #16. Subsequent changes require their own delivery checks.
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

The user's full-project scope also includes the later phases: optional semantic
retrieval, daemon/API, a local graphical or terminal client, signed packs,
advanced learning and Windows runtime. These remain tracked in the exhaustive
[remaining acceptance inventory](remaining-acceptance.md); MVP completion alone
does not close the project.

## Signed execution and optional hosts

PR #19 adds durable signed backend activation, externally pinned
trust roots and current-state locking through dispatch. Preparation, standalone
runs, workflow coordination and feedback share the selected decision while
preserving the original deterministic receipt. Historical verification remains
possible after rollback; new dispatch fails. Changing the store locator or its
pinned authority requires new approval, even for a byte-identical store copy.

OpenCode and Copilot are explicit optional build features. Their approved binary
receives a bounded, offline, read-only version probe before any worker/worktree.
Native canaries use synthetic executable protocols under real OS containment;
they establish neither vendor authentication nor observed model identity.

PR #18 (`a5b0709ebeae0c3c871e2bd6309c2dd1de6ccde2`) delivered the repository
presentation, feature table, author block and an offline routing GIF/MP4. Its
post-merge macOS/Linux workflows passed. Demo timing is edited, not a benchmark.

## Contextual retrieval, risk floors and terminal output

PR #20 (`5525b1205d2db2a36d5b8fa6312f66a6756dbc9d`) adds five-family contextual retrieval, catalog-bound metadata
v3, deterministic reciprocal rank fusion and incremental updates. Semantic
ranking is an explicit build feature and consumes a supplied ranking; it does
not install or contact an embedding provider. Human recall evidence remains open.

Normalization and planning now apply versioned risk floors. Sensitive operations
cannot be made low risk through a weaker declared value. Missing authority facts
still require clarification, and classification grants no permissions.

Product commands expose a bounded human rendering of the same versioned result
as `--json`. Approval digests and operation exit codes are preserved, including
when an oversized display is shortened. The September 28 continuation adds
strict JSON, TOML and YAML input for `config --layers`; 16 CLI tests exercise
valid files, equivalent semantics and adversarial syntax in both default-host
and core-only builds. Guided task input and other manifest formats remain
separate work. See [configuration formats](configuration-formats.md).

Native failure classification uses bounded typed host events, with unsupported,
conflicting or incomplete evidence falling back to the generic failure. Timeout,
cancellation and supervisor limits retain priority. Typed validation commands
and independently observed objective success remain separate from this change.

## Durable escalation

The runtime library now records terminal attempt facts atomically and can propose
a different eligible model or effort after two separately observed failures.
Unknown cost/token accounting blocks the proposal unless the parent explicitly
attests a bound for each attempt. Such attestations remain distinct from measured
spending. The source evidence can authorize one new attempt, with a new approval,
the same task/source/scope, and the remaining cumulative budget.

The reviewed version passed 60 runtime/decision tests and four real macOS sandbox
canaries using synthetic workers. The canaries include the v3 asset gate and a
regression refusing writing failures even after their worktrees are discarded.
The standalone CLI lifecycle shipped in PR #22, merged as `cc8b3e4`; all eight
checks passed on its exact head `8d11338`. It requires parent accounting approval,
an inert pinned proposal, selected-asset preparation and a fresh run approval.

The coordinator extension is implemented locally with registered bundle sources,
exact final prompts and artifact grants, cumulative reservations and next-generation
approval. Two native synthetic-host canaries cover a nonempty predecessor handoff,
two failed generations, secondary dispatch, historical reopen, interrupted
registration recovery, and budget/cancellation refusals before a new claim.
Its independent review and remote CI must accompany delivery; those local canaries
do not establish provider behavior or measured accounting. See
[coordinator escalation](escalation.md#cli-coordinator-source-bundles).

## Resumed planning, catalogs and Omnigent work

The September 28 continuation validates the paused checkpoint's bounded global
plan generator and connects its retained proposal to CLI preparation and runtime
decisions. The initial proposal is pinned transactionally to the workflow event
history. Eight runtime regression tests cover migration, restart, altered or
missing pins, source/scope changes and expiry. The record is a consistency check,
not a signature protecting a database whose entire history can be rewritten.
See [plan generation](plan-generation.md) for the native execution evidence.

Signed catalog validation now covers common provenance, author, license and
validity fields, hosts and presets, and exact prompt references. The 42 catalog
tests include a correction for payload hosts outside a prompt's signed
requirements. Active catalog state and dispatch-time invalidation remain a
separate integration. See [signed catalogs](signed-catalogs.md).

The [Omnigent comparison](omnigent-comparison.md) extends existing roadmap lots
with six documented ideas. The first implementation is a strict, offline routing
translation with ten tests and independent review. It only recommends a pair
that Omnigent offered and the local router validates. It starts no session and
does not enforce Omnigent's fallback behavior; an HTTP endpoint and native
integration evidence remain open. See [the bridge contract](../integrations/omnigent.md).

## Acceptance that automated implementation cannot manufacture

The specifications require 100 cases with human-validated expected decisions and
a recall@5 threshold defined from that human corpus. Generated fixtures are
regression candidates until human validation is supplied. Semantic promotion
requires an observed held-out improvement; it remains disabled when that gate
is unavailable or fails. Runtime capability, identity and permission claims
require host evidence and remain UNKNOWN otherwise. No public publication is
implied by local packaging or merge authority.
