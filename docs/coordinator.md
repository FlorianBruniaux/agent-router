# Durable workflow coordination

`router-coordinator` adds explicit workflow instances, per-step approvals and
artifact handoff above `router-runtime`. A model-produced plan remains untrusted
data. The parent or user must verify that proposal before an Execute dependency
can become ready. Model output never mutates the workflow definition, permissions,
skills, tool policy, source scope, or approvals.

The implementation runs explicitly approved waves. It does not autonomously
approve later steps, reinterpret natural-language plans as executable authority,
or infer functional success from a provider's text.

## Definition and authority

`seal_definition` validates and hashes `WorkflowDefinition` version 1: the
unchanged v1 plan, its separately sealed execution-instance contract, declared
outputs, explicit artifact edges, context-size ceiling and failure policy.
Each node output must be declared exactly once. Every non-task input must resolve
to a declared output of an actual dependency. A hash cannot make an invalid graph,
out-of-scope writer, missing output, or unsafe artifact edge valid.

`register` binds the definition hash and runtime storage to an instance ID.
`review_step` computes the current input grants, includes their canonical JSON
data section in the final prompt, and consumes the router's opaque validated
receipt. A fresh approval covers that final prompt, selected candidate, artifact
metadata, workflow instance and generation. `PendingStep::final_request()` and
`artifact_contract()` let an explicit caller retain the exact material required
for reconstruction; the coordinator never persists full prompts automatically.

Each step's runtime ID is a deterministic digest of definition, node and
generation. Repeating a review with the same material is idempotent. Different
material, another instance, or a changed source is rejected. Candidate cost,
token and elapsed reservations must fit the node's budget. The coordinator also
accounts for cumulative declared cost/token reservations across generations.
These reservations do not prove provider billing or actual token consumption.
An unused reservation remains conservative rather than being silently refunded.

The runtime repeats the v3 asset gate on the actual adapter stdin at dispatch.
The original approved workspace/source/scope remains the authority; an isolated
write worktree or read-only verification view is a runtime-derived location.
Dependency data is appended before approval and before that gate, never after it.

## Results and handoff

`NativeFinalText` and `Patch` are declared output sources. They are independent
of optional stdout/stderr diagnostic retention. Native text is extracted only
from a complete, successful native stream: Claude `result` success text or Codex
completed agent messages followed by a completed turn. Missing, invalid, empty,
oversized or incomplete text cannot become a declared result. A valid result is
still untrusted content, not verification evidence.

Declared results use private files (directory 0700, files 0600), bounded bytes,
SHA-256, recorded inode identity and explicit retention. Schema 5 adds their
metadata to the runtime database. No raw result or prompt is copied into event
logs, run summaries, wave JSON, or the coordinator database. Publication records
Pending ownership before payload writes and changes to Retained only after fsync.
The explicit purge API accepts an artifact ID, never an arbitrary deletion path;
symlinks, replaced inodes and unowned files fail closed. Expired results cannot
be used as inputs. Callers explicitly schedule purge; expiry is not secure erasure.

A grant binds instance, producer node/run/generation, output label, content hash,
byte count and exposure. `PromptData` is bounded, canonically quoted data in the
request. `ReadOnlyWorkspacePatch` is available only to an independent, read-only
Verify node. After approval, the runtime creates a separate detached worktree
from the approved base, applies the stored hash-checked patch there, and runs the
verifier under the read-only OS sandbox. It never gives the verifier the mutable
producer worktree. The original source remains unchanged until explicit apply.

The verifier can run while the producer is `Verifying`. Explicit verification
records remain necessary for both nodes. Requested model/agent/owner independence
is checked against the producer; observed model identity remains UNKNOWN unless
native evidence actually provides it.

## Ownership, control and recovery

One coordinator owns its storage through a file lock. One runtime owns its
supervision database. SQLite WAL readers and durable control requests work while
that owner is executing children. Coordinator migrations, controls and events are
transactional; events are append-only and chained by hash. Control IDs hash their
full record, so repeating a request is idempotent.

`request_approval` and `request_verification` accept the exact run approval digest.
`service_controls` processes them only when the owner has reconstructed the
matching confidential `PendingStep`. A queued control with missing material waits
for reconstruction. Verification can therefore be recorded while another node is
running. `request_cancel` immediately cancels unstarted runs and sends durable
requests to active owners without signalling stored PIDs.

`run_ready` runs at most the plan's width, retaining runtime global/provider/agent
limits. `WaveCompletion` includes every submitted run's completion or typed error,
including queued runs cancelled by fail-fast. Continue-independent preserves
unrelated work; failed dependencies never become ready. Fail-fast stops the wave
and cancels its registered unstarted/active steps. It does not grant a retry or
resume a cancelled step. A new workflow instance is required for a new attempt
at cancelled work.

After restart, `reopen_step` requires the exact final request, artifact contract,
router context and receipt. Verified nodes remain verified and cannot be replayed.
Queued runs resume only with their existing approval digest. Interrupted execution
is classified by the runtime and requires the explicit bounded retry/generation
path; no automatic inference of successful transport or verification is made.

The runtime watchdog and schema 6 cleanup leases govern interrupted processes.
Unknown cleanup retains writer authority and blocks new execution even after the
coordinator has reconstructed its state. `GroupTerminated` on macOS is limited
to the process group; `NamespaceTerminated` on Linux is a separate containment
claim. Ordinary future cancellation and clean restart tests are not proof of
SIGKILL recovery; keep that evidence separate from coordinator reconstruction.

## Escalation of a failed step

A registered read-only step may supply durable attempts to the explicit
[escalation CLI](escalation.md#cli-coordinator-source-bundles). Its final prompt,
artifact grants, workflow instance and node remain fixed. The selected model or
effort changes only through the reviewed runtime proof and a new per-step
approval. The target is the next generation, with at most one attempt and no
implicit target retry. Generated and already escalated origins are ineligible.

`reopen_recorded_step` reconstructs historical confidential material against the
recorded coordinator/runtime mapping without opening a second owner. It grants
no approval. `review_step_decision` serializes admissions, checks the cumulative
reservation budget before runtime review, and requires the source's exact
artifact-contract hash. Runtime repeats that hash check before its durable claim.

The coordinator gates the final serialized request, including dependency text,
through the same conservative v3 preparation bound as the standalone pipeline.
Both serialized request size and actual host stdin remain bounded. Preparation
exports the base request, so review appends one dependency section rather than
duplicating a previously finalized prompt.

The CLI requires homogeneous attempt ceilings within a wave. An ordinary step
keeps the ceiling of two; an explicitly approved escalation wave can use three
through five. Cumulative declared coordinator reservations and parent-attested
actual-attempt ceilings are separate conservative checks. Neither measures a
provider's bill or tokens.

Registration spans two durable stores. A crash after the runtime claim but before
the coordinator row is recoverable by reviewing identical material. That operation
is idempotent and cannot reserve another target. Cancellation after reservation
keeps it consumed and blocks execution. There is no cross-database atomic commit
or automatic refund.

## Validation evidence

The tests use temporary Git repositories and fake native host processes. They
exercise proposal approval, isolated writes, immutable patch views, private result
storage, cross-instance rejection, tampering, symlink-safe purge, duplicate owner
refusal, partial-wave accounting, simultaneous workers and verification controls
while another worker remains active. A restart test reconstructs exact material,
processes a pending verification, and proves the verified node does not execute
again. An expired v3 asset-contract test rejects dispatch before any child starts.

The explicit native canary runs this plan/execute/verify handoff through the real
resource helper and OS sandbox. It passed locally with macOS Seatbelt. The Linux
workflow requires the same test under bubblewrap. Neither a fake host nor a native
sandbox canary proves that a real model produced a correct plan or patch.

```sh
cargo test -p router-coordinator -p router-runtime --locked
cargo build -p router-sandbox-helper --locked
cargo test -p router-coordinator --test workflow native_sandbox_plan_execute_verify_handoff --locked -- --ignored --exact --nocapture
```
