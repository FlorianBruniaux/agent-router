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
| #8 | Distribution, licenses, notices, SBOM | Source license and advisory gates delivered; final release pending | PR #29 (`0fa3adf`) enforces real strict source exports and verification in CI; PR #32 (`e1e8fe0`) adds fresh RustSec audits. Local binary inventories are implemented in the integration candidate below; signed release artifacts and final release acceptance remain open |
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
- Deep diagnostics check bounded component schemas and catalog/body bindings.
  Declared database paths now receive metadata checks and an explicit snapshot
  requirement, without opening their SQLite files. PR #26 supplies bounded
  in-memory inspection from an already-owned connection, including committed
  WAL contents. The runtime observer uses SQLite read-only coordination, which
  can update WAL-index/read marks; zero filesystem writes are not promised.
  Host activation and authentication remain separate observations.
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

The coordinator extension shipped in PR #25 at `009b701` with registered bundle sources,
exact final prompts and artifact grants, cumulative reservations and next-generation
approval. Two native synthetic-host canaries cover a nonempty predecessor handoff,
two failed generations, secondary dispatch, historical reopen, interrupted
registration recovery, and budget/cancellation refusals before a new claim.
Its independent review and eight PR checks passed, as did all four post-merge
checks. The local suite passed 687 tests, with 27 explicitly ignored native
cases run separately where applicable. Those canaries do not establish provider
behavior or measured accounting. See
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
requirements. A separate [current catalog store](catalog-store.md) now implements
explicit activation/revocation approval, durable revisions, cross-process guards
and historical inspection. Its 15 local tests cover rollback, expiry, changed
grants and replacement of the directory or lock while a guard is held. Independent
regressions reproduced both stale-guard paths before correction and reject them
afterward. The store shipped in PR #23 at `4804b37`. PR #27 at `9c47a2b`
then added [signed routing](signed-routing.md): authenticated constraints are
applied before scoring and v4 receipts bind the current catalog and grants.
All eight PR checks and four post-merge checks passed. PR #31 (`d4eafa1`)
then delivered opaque [signed preparation](signed-preparation.md): current
catalog and publisher authority are checked before body reads and after
assembly, including the exact selected candidate and snapshot expiry. An
independent regression corrected rejection caused by an excluded candidate.
The integrated local suite passed 723 tests, with 27 explicitly ignored;
all eight PR checks passed. Runtime dispatch and the public signed CLI
lifecycle remain open. See [signed catalogs](signed-catalogs.md).

PR #26 at `4116045` delivered the owned SQLite snapshot after independent
review found and reproduced an inherited transaction-mode defect. Explicit
deferred reads now preserve caller defaults and work during a concurrent WAL
writer. The final local suite passed 697 tests with 27 ignored; all eight PR
checks and four post-merge checks passed. A snapshot is not evidence of host
activation, schema migration correctness or process liveness.

The optional [standard sidecar client](standard-sidecar.md) shipped in PR #28
at `2caf048`. The final local suite passed 731 tests, with 27 explicitly ignored;
all eight PR checks and four post-merge checks passed. Independent regressions
cover capacity negotiation, concurrent sockets and a deadline shared by
negotiation and ranking. It negotiates protocol v1, ranks only the supplied candidate pool
and sends separately authorized retained outcomes or human feedback. The
deterministic decision stays active; a transport acknowledgement proves neither
learning nor native execution. This client does not implement the local daemon.

PR #29 (`0fa3adf`) requires a real core source export with complete license
evidence and independent archive verification in CI. Its 26 packaging tests
include incomplete evidence and tampering refusals; all eight PR and four
post-merge checks passed. PR #32 (`e1e8fe0`) adds a bounded, read-only RustSec
audit with an isolated pinned toolchain and no advisory exceptions. All ten
PR checks passed, including two audits against fresh database state. The
recorded lockfile/database pair is documented in the PR; future advisories
remain subject to the recurring check. Neither gate supplies legal approval,
a compiled-binary SBOM or authorization for public release.

The [Omnigent comparison](omnigent-comparison.md) extends existing roadmap lots
with six documented ideas. The first implementation is a strict, offline routing
translation with ten tests and independent review. It only recommends a pair
that Omnigent offered and the local router validates. It starts no session and
does not enforce Omnigent's fallback behavior; an HTTP endpoint and native
integration evidence remain open. See [the bridge contract](../integrations/omnigent.md).

## Combined local candidate, September 28

The integration branch combines PRs #34 through #42. Its code at `1acec1d`
passed 905 workspace tests with all features, zero failures and 34 explicitly
ignored native tests. Six relevant native macOS canaries passed separately:
shared admission, queued cancellation, source changes, session allocations,
SIGKILL recovery and coordinator handoff/escalation. Providers in these canaries
are synthetic. Formatting, strict workspace/core Clippy, the harness, 38
packaging tests, workflow syntax and the source secret scan passed.

| Lot | Implemented in this candidate | Remaining boundary |
| --- | --- | --- |
| R12 / O3, PRs #35 and #40 | Shared capacity, fairness, atomic conservative cost/token reservation, durable pre-dispatch commitment and state-bound limit review | Allocations are not measured billing; unknown cleanup retains capacity and allocation |
| R16, PR #38 | Guided task intake and bounded, explicit export | Export is atomic per file, not as a directory transaction |
| R17, PR #39 | Passive runtime storage observation, integrated with schema 9 | Process liveness, journal authenticity, cleanup authority and separate admission accounting stay UNKNOWN |
| R18, PR #36 | Cohort-bound success under budget, cost/time per success, Brier and calibration metrics | Human evidence, observed gain, frontier/regret metrics and durable cache remain open |
| R06, PR #37 | Library guards for publisher policy and reconstruction of approval reviews | The signed catalog CLI and runtime dispatch remain unimplemented |
| R20, PRs #34, #41 and #42 | Three bounded parser fuzz targets, SQLite 3.51.3 and local binary export with license texts and build inventories | New combined fuzz runs, platform evidence and release/security gates remain separately tracked |

Integration reproduced a POSIX-lock defect independently of the upstream
SQLite WAL-reset issue: opening and closing an auxiliary descriptor could
release another connection's process-wide locks. Runtime and coordinator now
inspect existing database metadata without that auxiliary open/close. Doctor
path inspection requires an owned SQLite snapshot instead of reading database
bytes. Native child-process regressions observed the failures before the fixes
and preservation afterward; they do not establish that corruption occurred.
Correct handling requires database artifacts to be declared as `Database`.

The binary recipe produced two identical native binaries and normalized archives
from source `1cf033e`, with all 276 referenced license texts. Independent
extraction and fixture routing passed. That source predates this combined
candidate and its additional lock fixes; its binary is local packaging evidence,
not a release artifact for `1acec1d`.

The pull requests remain drafts. GitHub Actions on PR #42 head `668a001` did
not execute any step because of the account payment/spending restriction.
[The recorded job](https://github.com/FlorianBruniaux/agent-router/actions/runs/36465499224/job/109074428992)
is distinct from local checks. Linux execution and remote merge validation for
this candidate remain unverified. Exported-workspace checks and fresh combined
fuzz evidence are recorded independently as they complete.

## Acceptance that automated implementation cannot manufacture

The specifications require 100 cases with human-validated expected decisions and
a recall@5 threshold defined from that human corpus. Generated fixtures are
regression candidates until human validation is supplied. Semantic promotion
requires an observed held-out improvement; it remains disabled when that gate
is unavailable or fails. Runtime capability, identity and permission claims
require host evidence and remain UNKNOWN otherwise. No public publication is
implied by local packaging or merge authority.
