---
adr: "0027"
title: Publish verified patches through a dedicated Git revision
status: implemented
created: 2026-10-08
last_modified: 2026-10-22
scope: "F02a simulated runtime patch publication; no working-copy rewrite or native admission."
amended_by: []
supersedes: []
superseded_by: []
related: ["0026"]
---

# ADR 0027: Publish verified patches through a dedicated Git revision

## Context

[#132](https://github.com/FlorianBruniaux/agent-router/issues/132) requires a
useful simulated patch with sealed preimages, content, modes, scope, verification
and application authority. The existing `Runtime::apply` runs Git against the
user working copy. Restoring that copy to the original source previously allowed
reusing the same approval. Sequential working-copy writes also cannot provide
all-or-nothing visibility or protect an unrelated editor through an advisory lock.

## Decision

Add a publication path to the existing runtime and CLI. Reconstruct the captured
patch on a private Git index starting from the exact source revision. Inspect
regular-file preimages and postimages, including Git modes `100644` and `100755`.
Reconstruct the diff from the resulting tree and require byte equality with the
captured, verified artifact. Generate a deterministic commit with that source as
its parent. The new approval binds the definition, local repository identity,
artifact, files, modes, tree, commit, verifier record and exact displayed diff.

Publication requires the separate digest, Integrate permission, allowed Integrate
mutation, and current capability evidence. CLI mutation reconstructs live proof pins from
its explicit `--config` sandbox configuration; historical reopen alone cannot
satisfy this gate. Publication rebuilds executable and configuration bindings at
each verification, including the final pre-effect check. These reads do not lock
those files against an unrelated concurrent writer after the check. Existing object/ref/log directory symlinks are refused before
Git mutation. Consume one durable `integration.intent`
in the existing hash journal before publication. Use Git's create-only ref
transaction with `--no-deref` for `refs/agent-router/integrations/<approval>`.
A pre-existing symbolic destination, including a dangling one, is explicitly
refused before that transaction. `--no-deref` alone does not preserve a dangling
symbolic ref on Git 2.50.1. Competing managed direct-ref publications use the Git
transaction; unrelated writers racing to change this namespace are excluded.
Git serializes competing creation even when requests use separate runtime stores.
No new store, schema migration, provider, global hook or dependency is introduced.

The visibility point is that one ref creation. A reader sees the complete result
revision or no result ref. User working files, index, HEAD and current branch are
not rewritten. Candidate generation can write several private files; interruption
after the first does not expose them as a published result. Objects created while
reviewing remain ordinary unreferenced Git objects until publication and can later
be collected by Git. Temporary indexes use the existing owner-private runtime root.

If execution stops after the intent or after ref creation but before terminal
`integration.published` evidence, observation returns `unknown`. It never clears
an intent, retries, resets a branch or rolls back user files. A ref matching the
candidate cannot repair missing journal evidence. Changed or missing refs also
make a previously complete observation unknown. Status is a read-time observation,
not a perpetual integrity guarantee.

Legacy `apply` retains its working-copy semantics and now consumes the same
one-shot intent before mutation. Its sequential working-copy application is not
qualified by this publication decision. Publication does not emit a feedback
`integration: applied` fact: creating a revision is not a human acceptance verdict.

## Evidence and limits

The runtime fixture repairs a nonnegative clamp function. Its sealed task names
the file and three acceptance cases. Independent fixture checks fail on the source,
pass on the candidate and pass again on code extracted from the published commit.
Other fixtures cover stale sources, candidate mode changes, substituted source and
Git-directory and ref-directory links, replaced review payloads, unsupported versions/keys, no-op,
permission refusal, expired evidence, duplicates and concurrent publication.
Fault injection covers both sides of ref creation, restart and concurrent user edits.
A timed-out worker demonstrates interruption after one private candidate write.
These are simulated filesystem/runtime tests, not OS sandbox or native-host proofs.

Support is bounded to a 16 MiB captured patch, UTF-8 review diff, 1,024 changed
regular files and 64 KiB of total path text. Git preserves only its executable-bit
modes, not arbitrary POSIX permission bits. The review-file CLI reader is bounded
to 128 MiB to permit JSON escaping of the patch. Every Git capture retains the
existing elapsed/output ceilings and repository-filter refusal. Publication also
refuses promisor/partial-clone configuration and disables implicit Git fetching,
hooks, fsmonitor and maintenance. Binary patches and rename/copy extended headers
are unsupported. Before cached application, the source-tree inventory is limited
to 100,000 entries and the sum of changed source blob sizes plus patch bytes to
16 MiB. Changed result blobs have a separate aggregate 16 MiB ceiling before
commit creation. These checks do not impose an OS memory or disk quota.

Local repository identities detect substitutions before publication. These checks
do not certify adversarial replacement of Git metadata during a Git subprocess,
power-loss persistence on every filesystem, or the selected native client/OS
composition. Those remain qualification work. F02b, F12a, financial admission and
real-provider trials stay open. No model saving or accepted real task is inferred.

## Process-crash fixture, 2026-10-22

An additional Unix subprocess fixture exercises the publication tail with a
prepared intent and receipt. It synchronizes through the existing capture
observer, then the parent SIGKILLs only its owned child before ref creation or
after Git succeeds but before `integration.published` is recorded. The parent
waits for actual termination before reopening the Runtime. Both interrupted
cases remain UNKNOWN with the expected absent or present result ref. Observation
and a refused repeated intent leave the journal unchanged. Staged, unstaged and
untracked user edits, index bytes, HEAD and the source revision stay intact.
A completed-publication child is the positive control: reopening observes
Published and the complete result revision, without replaying the intent.

The marker, bounded waits and owned-child cleanup are fixture code only. The Git
helper used by these fixtures ignores personal/system configuration and hooks. No production
failpoint, API, permission or resource limit changes. This checks process-crash
recovery of the publication tail, not the full authorization/worker pipeline,
machine power loss, native sandbox composition, human acceptance or provider
accounting. The earlier timed-out candidate-write test keeps its own scope.

## References

[Git update-ref](https://git-scm.com/docs/git-update-ref) defines create-if-absent
transactions and `--no-deref`. [Git apply](https://git-scm.com/docs/git-apply)
defines cached application without working-tree writes. The repository tests
exercise the selected operations rather than treating the documentation as runtime
evidence. See [ADR 0026](0026-owned-client-simulated-entry.md) for the entry boundary.
