# Approved execution runtime

`router-runtime` executes a selected v2 phase through an adapter, an OS sandbox
and a separate resource-limit helper. The v1 dry-run router is unchanged.
No test in this crate invokes a model or a provider service.

## Approval and execution

`Runtime::review` accepts a `ValidatedPhaseContext`, recomputes its receipt using
`PhaseRouter`, validates the selected candidate and checks the workflow DAG.
The execution request must match the selected model, effort, agent, skills,
tools, capabilities, sandbox and node scope. A JSON receipt is insufficient.
The definition digest covers the complete request, including the prompt and
skill bodies, the complete receipt, the plan and the node identity.

Review requires a canonical Git repository root with a clean index and working
tree, including untracked files. Git hooks, external diffs, text conversion and
fsmonitor are disabled. Any local or included `filter.*` configuration is
rejected before status, checkout, add or diff. Repository configuration values
are neither logged nor returned.

The caller approves the exact digest returned by `PendingRun`. The resulting
`ApprovedRun` has private fields and cannot be deserialized. Execution rechecks
the source commit, clean state, snapshot age, permissions and adapter output.
Writing runs create a detached private worktree only after approval. A selected
skill is instruction text; executable skills remain unsupported.

`PendingRun` can be cloned for subsequent verification. `Runtime::reopen`
reconstructs only an existing, identical definition. It supports historical
review after snapshot expiry or after an approved patch makes the source dirty.
It never registers a new run. Execution and integration still enforce their
current-state checks.

## Durable state and control

The private runtime directory has mode `0700`; its database, lock and artifacts
have mode `0600`. SQLite uses WAL, foreign keys, full synchronization and
monotone migrations. Writes and their events share a transaction. The core
tables are `runs`, `run_attempts`, `events`, `writer_locks`, `artifacts`,
`verifications` and `control_requests`. Catalog and learning storage belongs to
the corresponding crates.

Events form a SHA-256 chain and SQL triggers reject updates and deletions. An
invalid state transition fails and records an integrity event. Hashes detect
changes; they do not authenticate a writer who controls the private storage.
Opening the store and `verify_integrity` replay the whole chain. Afterwards the
lock owner verifies only the events after the last one it verified, and
rehashes that one. Observation and cancellation connections replay the whole
chain. See [ADR 0018](adr/0018-incremental-event-chain-verification.md).

One supervisor holds an exclusive advisory file lock. A second supervisor
cannot bypass its concurrency limits. `observe`, `observe_events` and
`request_cancel` use separate WAL connections without taking that lock. A
cancellation immediately moves planned, awaiting-approval or queued work to
`cancelled` in one transaction. During preparation or execution, the request is
consumed by the live owner, which signals its own process group. Control clients
never signal a stored PID.

After restart, preparing or running work becomes `failed` with an
`interrupted` classification. Process liveness is recorded as unknown. The
runtime does not replay verified steps or signal old PIDs. A transport that
finished before restart still requires verification. Retries are explicit,
bounded, restricted to read-only work and require a fresh approval digest.

## Processes and scheduling

Execution uses `tokio::process::Command` with argv. The runtime does not insert
a shell. Test fixtures explicitly use a shell to exercise process behavior.
Each run starts a Unix process group. Cancellation, timeout and output overflow
trigger TERM followed by bounded KILL. The resource helper supervises the worker
outside the worker's resource limits and watches a private parent-lifetime pipe.
Dropping supervision or killing the parent closes that pipe. The helper keeps the
leader unreaped until its final group signals, then checks group disappearance;
it never sends a signal using an already reaped leader's PID.
stdout and stderr share a byte budget. Their hashes and counts survive;
their full payloads and prompts are not retained by default.

A stream reader that has not finished two seconds after termination (for
example a descendant still holding the pipe) is aborted. For a run, the observed
termination and the bytes already captured are kept and an `output.incomplete`
event is recorded; the failure is not reclassified as a spawn error.
A nonzero exit whose adapter classification fails with an error other than a
protocol mismatch records `failure.classification_error` and the non-retryable
`RunFailure::Unclassified` (serialized `unclassified`, an additive contract
value; feedback treats it as `Unknown`, never as a model fault). A run whose
process exited but whose output is incomplete also ends `failed` with
`unclassified`, never `succeeded_transport`. Such a prefix is never decoded:
`runtime_evidence` is `None` and no `protocol.observed` event is written. The
`RunCompletion` digests and byte counts describe the captured bytes only (as
they already do on output overflow; the fields are not optional, so omitting
them would be a contract change), and retained output artifacts are marked
`truncated: true`. Every other capture caller (git,
probes, version checks) fails closed with `runtime.process.failed` when a
stream is incomplete.
If retaining output fails after a successful transport, the run moves through
`verifying` to `verification_failed` instead of staying `succeeded_transport`.

Schema 6 adds per-attempt cleanup leases and persistent cleanup quarantine.
Before spawn, the runtime creates a private, never-overwritten lease, binds its
inode to the attempt transaction, and passes its path only to the trusted helper.
The helper holds an exclusive lock from before spawn through cleanup. Recovery
must acquire that lock and validate the original inode and exact status. A
pending lease is changed to recovered while still locked, preventing a delayed
helper from spawning after recovery. Missing, active, corrupt or replaced leases
preserve writer locks and block new execution and retries. Observation remains
available. Historical interrupted runs without leases are not assumed safe.

Schema 8 additionally journals each preparation cycle before Git or a native
version probe can run. The cycle binds the run definition, workflow generation
and next model attempt, but has its own sequence and never consumes an attempt.
Every Git capture, including its configuration query, opens an `untracked`
window before spawn. Only a completed capture closes it, even when Git then
reports a business error such as a changed source. A crash or capture error in
that window remains unknown. These Git-return observations are separate from
native group/namespace cleanup evidence.

Version-probe leases are registered against the cycle before capture and resolved
under their original inode lock through the journal commit. Recovery requires
both safe preparation and safe model-attempt cleanup. Restoring the original
lease after a temporary absence permits reconciliation only after its cleanup
status and lock can both be verified. Terminal cancellation or rejection retains
quarantine while either proof is unknown. No timeout or PID observation clears
it. Historical interrupted `Preparing` and `Running` runs without a cycle remain
quarantined, including runs with a clean model lease. Migration does not backfill
missing evidence; completed history remains readable.

Cycle projections are anchored in append-only events, with at most 1,024 cycles
per run, 1,024 captures and eight probes per cycle, and 16 KiB per cycle event.
The events contain identities and evidence kinds, never probe output or prompts.
The local hash chain detects projection changes but is not authentication against
complete database rewriting by its owner. See [ADR 0007](adr/0007-durable-preparation-cleanup.md).

`Runtime::cleanup_evidence` distinguishes `Unknown`, `NotStarted`,
`GroupTerminated` and `NamespaceTerminated`. The macOS contract proves termination
of the process group, not hostile descendants that detached with `setsid`.
Linux namespace evidence additionally requires the probed bubblewrap PID
namespace. A transport exit code never substitutes for these cleanup claims.
The runtime records only the bounded evidence kind, not process payloads.
The native macOS SIGKILL regression runs a separate supervisor, kills it while a
TERM-resistant fake worker writes in its isolated worktree, reopens storage and
checks both the cleanup evidence and writer-lock release. It also checks that the
worker's heartbeat remains unchanged after recovery. Linux CI requires the same
runtime test and the helper's detached-descendant namespace regression.

FIFO semaphores bound global, host and logical-agent concurrency, plus the plan
width and effective restriction limit. Host identifiers currently serve as the
provider queue key. Queue reasons are persisted. A node starts only after its
dependencies are verified. The runtime never copies arbitrary predecessor
stdout into a successor prompt; artifact transfer requires an explicit caller
contract.

## Optional output retention

`RuntimeLimits.output_retention` defaults to `OutputRetentionPolicy::Disabled`.
An explicit `PrivateArtifacts { retention_seconds }` value, from 1 second to
365 days, binds the policy into the run definition and approval. Changing the
policy invalidates an existing approval. Disabled runs preserve the v1
definition hash; retained runs use the separate
`runtime-definition-v2-output-retention` domain. The policy is persisted in
`RunSummary` so historical callers can reopen the same definition.

The runtime writes stdout and stderr only after capture completes, below its
private `outputs` directory. Directories are mode 0700 and files mode 0600.
The combined retained size cannot exceed the request's capture limit, with an
absolute ceiling of 16 MiB per attempt. Overflow artifacts contain the captured
prefix and report `truncated: true`. A crash before capture completes produces
no complete output artifact. The prompt is never retained separately.

`RunCompletion.output_artifacts` and `Runtime::output_artifacts(run_id)` expose
paths, counts, SHA-256 digests, timestamps and retention state, never payload
bytes. The SQLite journal contains metadata digests only. Retained bytes remain
sensitive material even though they are excluded from JSON and event logs.
No output directory is created when retention is disabled.

SQLite schema 3 records pending output metadata before writing. Each created
file's identity is committed before any payload bytes are written. Interrupted
writes remain `pending` and can be explicitly purged; a crash between file
creation and identity registration may leave an empty file, which the purge
will not claim. Completed artifacts are `retained`. Purge records `purging`
before deletion and `purged` afterward, so an interrupted purge is retryable.

Deletion is caller-controlled through
`purge_output_artifacts(run_id, attempt)` or
`purge_expired_output_artifacts(now_unix_seconds, limit)`. The latter accepts at
most 100 attempts per call. Expiration is an eligibility timestamp, not a
background scheduler: an operator must invoke the purge. APIs accept no paths
to delete. Directory handles, no-follow opens, exact generated filenames and
recorded file identities prevent traversal, symlink following and deletion of
an unowned file at a colliding name. Purging verifies retained content first;
replaced files or directories require operator review. Unrelated files survive.
Deletion removes directory entries; it does not claim secure media erasure.

## Native sandbox evidence

`SeatbeltSandbox::probe` runs local canaries for allowed and denied reads,
allowed and denied writes, and a denied connection to a loopback listener.
The positive controls must also succeed. A failed or unavailable probe cannot
produce a supervisor. Programs, extra readable roots and environment values
are explicit trusted integration inputs. No home directory or environment is
inherited implicitly. Codex and Claude execute a per-attempt private copy of
the approved entry file. The runtime hashes that copy after closing it and
refuses a mismatch; the child cannot write its 0500 file or 0700 directory,
which stay outside the store and writable host state. Replacing the original
entry after command preparation does not replace the executed bytes.
Interpreters, loaded modules, other adapters, helper and sandbox-wrapper paths
retain their existing hash-to-exec race boundary. Another unsandboxed process
running as the same user can still modify private files.
Install approved binaries and the helper where the workload cannot write.
Some macOS system executables fail signature validation when copied despite
identical bytes. The router does not re-sign them or transform approved bytes.
Hosts that load resources relative to their entry path may also fail from a
copy; successful copying alone is not native-host readiness.
Probe scratch files use fixed names below the private directory; stale entries
from a killed probe are removed if owned and not symlinks, and every path is
removed on every exit.

The macOS profile denies access by default. It permits required system reads,
the repository and approved readable roots. The loader also requires reading
the `/` directory itself; this is a literal rule, not a subtree permission.
`mach-lookup` is allowed globally (the loader and system libraries need
several services). `network: false` therefore proves only that socket
networking is denied, not the absence of DNS or LaunchServices channels.
`SeatbeltSandbox::dns_resolution()` is `Unknown` unless the caller invokes
`probe_dns_resolution()`, which the normal probe never does. That call performs
one real outbound DNS query (`example.com`) outside the sandbox as a control
and one under the `network: false` profile, each bounded to 2 s, through
`/usr/bin/dscacheutil` (a system binary outside the approved-binary hash
table). The result is `Resolved`, `NotResolvedUnderProfile` (one failed lookup,
not proof of absence) or `Unknown` (control failed). It is evidence only and
never fails a probe or a run. A mach service allowlist is deferred to a native
canary.

Both native sandboxes share one guard: the working directory, readable roots
and writable scope paths must already be canonical (no symlink or alias), and
`/` is never readable. Seatbelt tolerates a write path that does not exist yet
if its deepest existing ancestor is canonical; bubblewrap requires it to exist.

Writes are limited to approved scope paths and `/dev/null`, with Git metadata
writes denied for every case spelling of `.git`. Network access requires the explicit routed network capability.

`BubblewrapSandbox::probe` provides the Linux implementation. It creates mount,
PID and network namespaces, drops capabilities and uses read-only mounts plus
the selected writable scope. It requires an available bubblewrap binary and
working unprivileged namespaces. Linux write scopes must already exist in the
worktree so they can be mounted precisely. A `.git` entry (any case)
directly inside a writable scope root is remounted read-only; a `.git` symlink
there is refused. Nested `.git` entries, and any the child creates during the
run, are not remounted (a recursive walk fails on scopes holding `node_modules`
or `target`). Instead the host walks the produced worktree with
`symlink_metadata` (no symlink followed) before its first git command and
refuses any entry named `.git` in any case other than the worktree's own root
`.git`, with `runtime.workspace.nested_git`. The walk is bounded to 500,000
entries and depth 128 (`runtime.limit` beyond). The root `.git` must not be a
symlink. The name comparison is ASCII case-insensitive only: HFS and NTFS
variants of `.git` are not matched. Residual limit under Seatbelt: the walk
runs after the worker ends, but without a PID namespace a detached descendant
(`setsid`) is not proven terminated and could write a nested `.git` between the
walk and the host git commands. Bubblewrap `--unshare-all` (PID namespace)
kills every descendant. Scope entries containing `.git` in any case (`.GIT`,
`.Git`) are rejected on every platform. Its enforcement canary must run on
Linux; compiling it on macOS is not runtime proof.

`ResourceSupervisor::probe` wraps either native sandbox with the separately
installed `router-sandbox-helper`. The helper applies resource limits in its
own process and then replaces itself with the native sandbox command. It never
changes the router parent's limits. CPU time, address space, descriptor count,
process count and output file size have explicit bounds; core dumps are disabled.
`RLIMIT_AS` describes virtual address space, not RSS. `RLIMIT_NPROC` has per-user
semantics, not a job-wide count. The helper fails if a requested primitive is
unavailable. `ResourceLimits.address_space_bytes` is `Some(8 GiB)` by default.
On the tested macOS host, Darwin rejects `RLIMIT_AS`, including outside the
development sandbox. The strict default therefore blocks execution there.
An operator may explicitly choose `None`; the resulting receipt reports
`memory_enforcement: not_requested`. This choice must be included in the
approval's configuration digest. It provides no memory bound, RSS enforcement
or evidence that the memory requirement is complete. Linux has a separate
adversarial allocation test for `Some` limits.

Approval-bound callers use `probe_expected` on the native sandbox and resource
supervisor. The expected map contains canonical executable paths and SHA-256
digests, including the helper and wrapper (`/usr/bin/sandbox-exec` on macOS).
Missing or changed digests fail before the helper or wrapper is executed.
These checks preserve the approved baseline across probing and launch.
The compatibility `probe` APIs establish a new baseline and must not consume an
earlier executable approval. Native binaries and the helper must remain trusted
and immutable while launching: execution still resolves a path after hashing,
so the content check is not an atomic file-descriptor-to-exec binding. System
canary utilities and dynamic runtime dependencies belong to the trusted OS.

## Verification and integration

Exit status zero produces `succeeded_transport`, then `verifying`. Neither a
provider message nor `verified=true` in stdout can complete verification.
`record_verification` requires an explicit verifier and evidence digest bound
to the current artifact. Independent verification rejects the executing
logical-agent identity. The caller remains responsible for authenticating the
verifier and the evidence; a digest alone is not an independent test result.

The runtime captures a binary Git patch from the isolated worktree and rejects
changed paths outside scope or paths containing symlinks. `integration_preview`
requires verification and binds a separate approval digest to the source and
exact patch. `apply` requires that digest, explicit integration permission and
an unchanged source. It modifies only the reviewed patch and never commits,
resets or globally stages files. `discard` explicitly removes the run's own
worktree; keeping it is the default when no discard is requested.

Callers must prevent concurrent hostile mutation of repository ancestors and
Git metadata. Process groups, private directories and content checks are not a
defense against another process with the same user's authority changing those
inputs between checks. The custom sandbox implementation itself is trusted
code; only the concrete native implementations and their canaries provide OS
enforcement evidence.

## Reproducible checks

```sh
cargo test -p router-runtime -p router-sandbox-helper --locked
cargo clippy -p router-runtime -p router-sandbox-helper --all-targets --locked -- -D warnings
cargo test -p router-runtime --test runtime native_seatbelt_write_and_network_canary --locked -- --ignored --exact --nocapture
```

The regular suite uses temporary repositories and fake worker processes. It
covers invalid transitions, WAL integrity, recovery, fresh approvals, source
drift, timeout, cancellation, descendant termination, output bounds, writer
conflicts, hostile filters, isolated patches and second-process control.
Native enforcement probes are separate from these tests and from any live
model invocation.

`.github/workflows/runtime-linux.yml` installs bubblewrap and netcat on an
ephemeral Ubuntu runner and explicitly runs the native canary and the real
allocation refusal test. It fails when those controls are unavailable; it does
not skip an unavailable namespace or replace the probe with a profile check.
The local macOS run verified read/write/network isolation and a fake worker
through the resource helper plus Seatbelt. No Linux execution is inferred from
that macOS result.

Dependencies are pinned to Tokio 1.53.3, rusqlite 0.32.1, nix 0.29.0 and rustix
1.1.4. Their upstream manifests declare Rust 1.71 for
[Tokio](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/Cargo.toml),
1.69 for [nix](https://raw.githubusercontent.com/nix-rust/nix/v0.29.0/Cargo.toml)
and 1.63 for [rustix](https://raw.githubusercontent.com/bytecodealliance/rustix/v1.1.4/Cargo.toml).
[rusqlite 0.32.1](https://raw.githubusercontent.com/rusqlite/rusqlite/v0.32.1/Cargo.toml)
does not declare a Rust version; the workspace's Rust 1.85 build is the local
compatibility check. SQLite's [WAL documentation](https://sqlite.org/wal.html)
describes the separate reader and writer behavior used by the control API.

## Explicit workflow instances

The v1 `WorkflowPlan` JSON and its hash remain unchanged. An opt-in,
separately sealed `WorkflowExecutionContract` binds a caller-chosen instance ID
to that plan, canonical workspace, committed source state, and the declared
phase and dependency requirement of every node. Review and approval bind the
contract hash and node generation. Reusing an instance ID with different content
is rejected. Legacy standalone runs have no instance and cannot satisfy workflow
dependencies. Schema 4 migrates existing history without assigning instances.

`review_selected` and `reopen_selected` consume `ValidatedPhaseReceipt`, an
opaque proof recomputed by the phase router. It covers the exact selected or
fallback receipt, including declared unavailable candidates; it is not evidence
that a provider failed. Existing `review` and `reopen` wrap primary selection and
remain available for nodes without dependencies. Dependency-bearing nodes require
the explicit execution contract.

Dependencies resolve only within an instance. A waiting node stays `Queued`,
holds no capacity permits, and observes durable cancellation. After restart,
`reopen_selected` plus `resume_approved` reconstruct an already approved queue
entry. They do not grant a new approval. `dependencies_ready` supports explicit
coordinator polling. A dependency that becomes unready during capacity acquisition
returns `runtime.dependencies.pending`, leaving the node queued.

`Verified` requires explicit verification of the producer. The separate
`ArtifactReadyForVerification` requirement permits a read-only independent Verify
node after an Execute writer has produced a private, hash-checked patch and entered
`Verifying`. The verifier must use a different declared owner, requested model,
and, when present on the producer, agent. Its `previous_model` must match the
producer's persisted requested model. These checks do not prove an observed model
identity or successful verification. Opt-in declared artifact handoff and the
durable coordinator are described in [coordinator.md](coordinator.md); readiness
alone does not transmit a producer's patch.

An explicit new node generation can use a separately approved fallback only after
a retryable, read-only predecessor failure. Attempt limits are cumulative across
generations, obsolete generations cannot retry, and registering a consumer freezes
its producers' generations. Authentication, policy, cancellation, output-limit,
and verification failures do not authorize retries. No automatic retry or fallback
approval is inferred.
