# Fresh finite entry, local scripted qualification

The new product command composes task normalization, observed capabilities,
phase routing, two explicit approvals and the existing finite Runtime from one
fresh task. This record concerns Linux ARM64 and an authored local parent.
No model or account is exercised. The full #132 acceptance remains open.

## Product boundary

Start `agent-router finite-entry --config PATH --json` once for a fresh task.
The parent sends one `submit` JSON line containing the task/risk facts, a model
and effort, one output name and hashed inline public files. It does not pass a
Runtime manifest, an old receipt or an approval from another session.
The [decision](../../adr/0045-fresh-finite-entry.md) defines the authority boundary.

Configuration fields are `schema_version: 1`, `enabled: true`,
`transport: "local_scripted"`, an absent absolute `root`, `programs` and
`allowed_models`. Programs contain exact `{path, sha256}` pins named `client`,
`native`, `helper`, `guard`, `namespace` and `profile`. Allowed models map each
explicit model ID to permitted effort strings. The root parent is canonical,
owned by the user and has no group/other write access. The native path requires
the already qualified Linux namespace composition with exactly 8 GiB and no
swap; a regular host shell is not equivalent evidence.

The [authored parent](controller.py) shows an exact submission built from the
[pinned public fixture inputs](input-pins.json). It is qualification tooling,
not a model adapter or an automatic global hook. The product requires explicit
Public sensitivity, explicit risk facts, workspace-write/network
mutations and tools/workspace-write/network capabilities. Higher inferred risk
cannot be overridden by a Low label. There are at most 16 flat public files,
32 KiB per file and 128 KiB total; AGENTS.md, statement.md and the existing
output are mandatory. Only the named output is writable through finite tools.

## Parent protocol

Events bind `schema_version`, a fresh `session_id`, exact input
`submission_sha256`, and increasing `sequence`. The normal order is
`entry.accepted`, diagnostic `entry.approval_required`,
`entry.diagnostic_complete`, `entry.route`, Runtime `entry.approval_required`,
`entry.dispatch`, bounded `entry.request` events, then `entry.result`.

An approval response contains exactly `schema_version: 1`, `kind: "approve"`,
`session_id`, `submission_sha256`, `phase` and `approval_digest` from its
challenge. A local scripted response contains exactly `schema_version: 1`,
`kind: "response"`, both identities, `request_id` and the authored SSE string
`sse`. Stale identities, duplicate keys, extra fields, repeated native requests
and EOF refuse. The parent must drain stdout and stderr. Submission and
approval waits are bounded to 15 seconds; HTTP/response/output waits to 5
seconds. The task budget is at most 60 seconds; the worker uses its execute
node allocation (30 seconds for a 60-second direct-safe task), eight tool calls
and nine response requests. Preparation is outside that worker budget.

Exit 2 is a pre-dispatch refusal. Exit 3 means transport completed at
`Verifying`; it does not mean the task was accepted. Exit 6 means outcome
UNKNOWN after attempting dispatch. Never replay a task automatically from
these exit codes. Existing private roots refuse instead of resuming. A
configuration/source change before approval refuses; active revocation after
dispatch is not implemented by this slice.

The product reads the captured patch through its retained opaque verifier and
reports it separately from the bounded display output. It performs no
verification recording or publication. Parent model activity remains UNKNOWN
to the product. The separately qualified parent below uses authored responses,
a network-disabled cell and no account or real campaign registry.

## Qualification and evidence

The machine-readable [evidence](evidence.json) records the completed local/native checks,
exact source and binary hashes, retained evidence digests, and limits. Raw
private Runtime directories are not published. `source-pins.json` fixes the
product source overlay before compilation; later documentation does not alter
those Rust sources. The earlier v1 build was intentionally stopped after the
post-execution source read was changed to the existing bounded reader. It was
never used for native qualification or counted as a successful build.

The first native v2 run passed four early refusal cases and completed real
sandbox discovery and routing. It then refused before creating any Runtime
run or worker attempt: candidate estimates and timeout exceeded the execute
node's share of the task budget. This failed qualification is retained. The
local regression reproduces that mismatch; the correction derives both values
from the existing node without increasing limits. The collection fallback for
an older Python version is separately recorded and does not change the result.

The parent independently checks the actual emitted route, SQLite summary and
attempts, admission release, namespace cleanup, unchanged source and captured
patch. It reapplies that patch to the initial source and evaluates the resulting
bytes using 18 already-public t02 v2 cases. These are consumed fixture checks,
not a new held-out oracle or a model benchmark. Objective remains UNKNOWN,
human acceptance is absent and no source publication is performed.

To inspect/test the parent without a native run, stage the `inputs/` directory
from each repository-relative source in `input-pins.json`, verifying each SHA256,
and run `python3 -B -m unittest -q test_controller` in this directory. Native
execution additionally requires the independently reviewed isolation driver,
its pinned offline image/vendor/assets, and fresh explicit approvals. Presence
of these scripts or a passing unit test grants none of that authority.

The later v3 product run completed at `Verifying` with one attempt, one actual
permission refusal and released admission. Its parent then failed while
importing the external evaluator from a path shallower than the evaluator's
repository-layout contract. That parent result remains FAILED. The loader is
corrected by staging the unchanged evaluator at its repository-relative path
and checking its hash. The artifact is evaluated separately from the retained
native result; this does not reopen Runtime or run another worker.

For the parent's external acceptance function, stage `verify.py` and `cases.rs`
also beneath `evaluation-repo/crates/router-runtime/examples/campaign/t02-v2/`,
using the same pinned bytes as `inputs/`. The portable parent tests check that
layout without invoking a native client or compiler. This record distinguishes
a successful product transport from its failed historical parent orchestration
and the separate artifact evaluation.

The completed qualification combines nine native refusal controls, one native
transport at `Verifying`, and separate acceptance of that exact captured patch:
all 18 public cases pass for the candidate while the original source fails. Two
modified artifact bindings refuse before evaluation. The
[artifact-only checker](evaluate_cell.py) opens no Runtime and starts no worker.
For that historical v3/v4 attempt, the corrected full parent flow was not
replayed; that record is composed evidence,
not a passing historical parent run. Both isolated cells are stopped.

Local checks report 1,957 workspace passes (including nested helper reports),
zero failures and 43 ignored tests. The 80 CLI checks, eight portable parent
tests, formatting, warnings-denied Clippy, harness parity, ADR checks and secret
scan pass. The isolated Linux build and nine CLI integration tests also pass.
CI is not claimed. The 8 GiB limit, zero swap and private access boundaries stay
unchanged. Observed cost/tokens and human acceptance remain unavailable.

## Follow-up: one complete fresh parent flow

A subsequent qualification runs the corrected parent in one new session through
both approvals, the native transport and the external public checker. The
[separate full-parent receipt](full-parent-evidence.json) records a passing
parent, one native attempt, six authored local requests and one actual
permission refusal. The candidate passes all 18 public cases; the initial
source fails as expected. The report, SQLite summary, captured patch and tested
candidate match. Admission is released and the namespace is terminated.

The product commit is `9c8ee4fa06eade652b1fdae3bbbd20a900baa29f`. All 469
inventoried files (466 under `crates/`, plus root `Cargo.toml`, `Cargo.lock` and
`rust-toolchain.toml`) and the 12 core source pins match the previously
qualified Linux build. The four
binaries and corrected parent are reused byte-for-byte. Ten portable parent
and evidence-collection tests pass. Earlier preparation-order and local sandbox
process-cleanup test failures are retained with their successful rerun; they
are not product or native failures. No Rust change or full workspace rerun was
needed for this two-file evidence follow-up.

All 25 retained evidence files were hash- and size-checked independently. The
cell is stopped, with 8 GiB, zero swap, no OOM event, no network or host mounts
and a read-only root. There is no model, provider account or real registry.
The original source is unchanged. The earlier failed parent and its separate
artifact evaluation remain in the unchanged `evidence.json`; the nine refusal
controls were not replayed. This new session is not another benchmark trial.

This closes the missing complete positive parent qualification only. Runtime
still reports `Verifying`, its objective remains UNKNOWN, and external tests
do not record a Runtime verdict, human acceptance or source publication.
Automatic host integration without a per-task command and remaining lifecycle
and recovery cases still belong to #132. #120/#133 retain their separate gates
for any future real-provider trial. The blocked remote CI is not a passing check.
