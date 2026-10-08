# Staged verified patches

Staged integration publishes a verified patch as an immutable Git
tree under a dedicated reference. It preserves the source worktree and its index.
The result is inspectable independently of later edits to the worker's worktree.
This is a partial delivery of [#132](https://github.com/FlorianBruniaux/agent-router/issues/132),
under [ADR 0027](adr/0027-staged-patch-integration.md). The
[source-bound verification](bench/verification-staged-patch-2026-10-08.json) separates
simulated runtime tests, native sandbox CLI coverage and remaining admission work.

## Try the simulated correction

From the repository root, with the project's Rust toolchain and locked dependencies:

```sh
cargo test -p router-runtime --locked --test staged_integration two_file_patch_passes_unchanged_acceptance_on_exact_staged_tree_only -- --exact
```

This constructed fixture corrects validation of a zero attempt limit and its error
text in two files. The producer is simulated. An unchanged independent acceptance
check fails on the base and passes on the exact staged tree. It is a test of the
patch path, not a claim that Agent Router itself had this fixture defect, that a
real provider produced the patch, or that the router saved money.

## Existing verified runs

The new artifact commands prepare the staged preview and publish its approved
result. They require an existing verified run and its original definition. A
route decision alone cannot supply those inputs. The coordinator exposes the
same operation for an existing reviewed step. Example arguments for your existing
verified definition and store:

```sh
agent-router artifacts --definition run-definition.json --store runtime staged-preview --json
agent-router artifacts --definition run-definition.json --store runtime stage --approve REVIEWED_DIGEST --json
```

`REVIEWED_DIGEST` is the exact digest of the inspected staged preview. These
commands do not create or authorize a native run. The dedicated Codex client
composition from ADR 0026 is a separate milestone.

The preview contains the exact stored patch, immutable source and result trees,
allowed scope, changed files with before/after hashes and Git modes, verification
record and review digest. Preparing it may create unreachable Git objects and a
private temporary index. It is not a passive observation command.

Staging requires the exact preview digest and the existing integration permissions.
Publishing a tree creates no accepted-change verdict or feedback fact saying that
source files were integrated. The original worktree remains available to the user.
A later edit to the worker or source worktree does not replace the captured patch.

## Failure and recovery

The runtime claims the operation durably before publishing its reference. A
second call, a concurrent managed publisher or a reopened runtime cannot silently
repeat it. A failed or interrupted publication retains its claim. A nonzero exit
does not prove that no reference was published: interruption can occur after Git
publication and before the completion event.

Preserve the runtime journal and inspect the expected staged reference before
manual reconciliation. Do not delete a claim to force a retry. This slice supplies
no automatic recovery or rollback command. Any manual removal must compare the
current reference to its recorded expected value and preserve a changed reference.
Removing a worker worktree is not permission to erase a staged claim or result.

## Limits

The supported result is an immutable tree, not a new source commit, a checked-out
user correction or human acceptance. Only regular Git file modes are supported;
these modes describe the executable bit, not full POSIX permissions or ACLs.
The staged path consumes only the runtime-captured canonical patch (`--no-renames`),
not an arbitrary patch import. Limits are 256 changed files, 100,000 source-tree
entries and 16 MiB for the patch and each aggregate source/result blob budget.
Binary patches are refused before application. These input bounds do not prove
a hard OS memory or disk cap. Partial-clone/promisor repositories and configured
content filters are refused.
There is no provider call or automatic credential discovery in this operation.

An existing symbolic destination, including a dangling one, is refused. Concurrent
managed publishers use direct-reference compare-and-swap. An unrelated same-user
process replacing dedicated staging references between checks is outside this
qualified boundary; the symbolic-reference preflight is not an atomic lock.

The legacy live-worktree application command is unchanged. Safe overwrite of a
worktree with concurrent writers and recovery between its file writes remain
open in #132. Native useful-task composition, OS qualification, physical-send
admission and real-trial measurement also remain separate milestones.
