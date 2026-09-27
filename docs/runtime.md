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
trigger TERM followed by bounded KILL; dropping supervision also kills the
group. stdout and stderr share a byte budget. Their hashes and counts survive;
their full payloads and prompts are not retained by default.

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
inherited implicitly. Native program hashes are rechecked before launch.

The macOS profile denies access by default. It permits required system reads,
the repository and approved readable roots. The loader also requires reading
the `/` directory itself; this is a literal rule, not a subtree permission.
Writes are limited to approved scope paths and `/dev/null`, with Git metadata
writes denied. Network access requires the explicit routed network capability.

`BubblewrapSandbox::probe` provides the Linux implementation. It creates mount,
PID and network namespaces, drops capabilities and uses read-only mounts plus
the selected writable scope. It requires an available bubblewrap binary and
working unprivileged namespaces. Linux write scopes must already exist in the
worktree so they can be mounted precisely. Its enforcement canary must run on
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

Dependencies are pinned to Tokio 1.45.1, rusqlite 0.32.1, nix 0.29.0 and rustix
1.1.4. Their upstream manifests declare Rust 1.70 for
[Tokio](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.45.1/tokio/Cargo.toml),
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
identity, successful verification, or access to the producer's patch. Artifact
handoff and a durable multi-node coordinator are still separate work; readiness
alone must not be reported as complete DAG execution.

An explicit new node generation can use a separately approved fallback only after
a retryable, read-only predecessor failure. Attempt limits are cumulative across
generations, obsolete generations cannot retry, and registering a consumer freezes
its producers' generations. Authentication, policy, cancellation, output-limit,
and verification failures do not authorize retries. No automatic retry or fallback
approval is inferred.
