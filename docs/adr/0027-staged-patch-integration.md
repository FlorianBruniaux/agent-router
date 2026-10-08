---
adr: "0027"
title: Stage verified patches as immutable Git results
status: accepted
created: 2026-10-08
last_modified: 2026-10-08
scope: "Bounded F02a staged application under issue 132. Live-worktree overwrite, native composition and real-provider admission remain open. Implementation and bounded local verification are recorded in the linked report."
amended_by: []
supersedes: []
superseded_by: []
related: ["0024", "0026"]
---

# ADR 0027: Stage verified patches as immutable Git results

## Decision

Add a separate staged integration surface to the existing runtime and CLI. Its
result is an immutable Git tree bound to the exact reviewed and verified patch.
Publishing that result does not overwrite source worktree files or change the
meaning of the existing integration command. This is a bounded delivery within
[F02a / #132](https://github.com/FlorianBruniaux/agent-router/issues/132), not
completion of its full application acceptance.

The existing path checks source state and paths, then invokes Git against the
live worktree. An editor or another process can change a file or symlink between
those operations. Neither a clean Git status nor a runtime-only lock proves
exclusive access to user files. A later successful test cannot repair that gap.

## Contract and ownership

`router-types` owns the versioned serialized preview and result. `router-runtime`
binds the source commit/tree, allowed scope, stored patch digest, verified record,
and each changed file's before/after content and Git mode. Absence is explicit.
Only regular-file modes `100644` and `100755` are supported initially; Git modes
do not describe full POSIX permissions or ACLs. Unsupported entries and input
limits cause refusal before publication.

The displayed patch comes from the same validated bytes as its review digest.
The approval for staged publication has a separate identity from live-worktree
application. Changing the candidate, scope, verification evidence or destination
invalidates that approval.

Preparing the preview applies the stored patch to a private index initialized from
the immutable source tree. This preparation may create unreachable Git objects;
it is not a read-only observation. It changes no source worktree files, source
index or reference. A complete resulting tree is published through a dedicated
Git reference using compare-and-swap. The private index is disposable preparation;
an incomplete index is not a published result. A source commit binds immutable
preimages, not the contents of concurrently edited working files. A changed
current HEAD or source reference does not change this immutable-base contract;
no simultaneous source-reference consistency is claimed.

Reuse the current runtime store and hash-chained journal. Claim the operation
before publication in a verified transaction. Concurrent or repeated attempts
cannot acquire a second claim. A lost response or interruption around publication
retains the claim and uncertainty instead of automatically repeating an effect.
No second runtime, generic transaction framework or store is introduced.

The dedicated reference namespace belongs to managed staged publishers. Refuse
an already present symbolic reference, including a dangling one, before Git
publication. Git 2.50 can otherwise treat its unresolved value as absent and
replace it. The symbolic-reference preflight is not atomic with publication;
unrelated same-user writers changing that namespace during publication remain
outside the qualified scope. Direct-reference compare-and-swap still arbitrates
managed publishers. Do not describe this as exclusion of every external Git writer.

## Verification

Use a constructed two-file correction with a simulated producer and an unchanged
independent verifier. Verify failure on the base and success on the exact result;
a supplied verification hash alone does not prove execution of that verifier.
This fixture does not establish a real defect in Agent Router or measured savings.

Exercise changed content, modes, scope, stored patch, verification binding and
reviewed diff; unsupported links; duplicate/reopened/concurrent publication;
interruption before publication and before the final journal record. Verify
independently that source user files are preserved and no partial result is
published. The private records retain failures and source hashes.

## Remaining boundaries

Safe direct overwrite of a live user worktree, recovery between its writes and
concurrent-editor exclusion remain unqualified. Preserving an edited source
worktree while publishing a separate immutable result is not proof of those
properties. The legacy application command receives no new safety claim.

The selected native client in [ADR 0026](0026-owned-client-simulated-entry.md)
still needs OS supervision and F02b composition. Physical-send admission, qualified
observations and the exact approved trial manifest remain prerequisites for
real-provider work. The staged result is not human acceptance; [ADR 0024](0024-accepted-revision-accounting.md)
retains its distinct verifier, reviewer and accounting requirements.

Discarding private preparation must not erase the durable claim or its evidence.
A published reference can be removed only against its recorded expected value;
a changed value must be preserved for reconciliation. No automatic rollback may
write user files or erase an ambiguous publication.

## Delivery evidence

The [staged patch guide](../staged-patch.md) describes the API and simulated example.
The [verification record](../bench/verification-staged-patch-2026-10-08.json) binds
source hashes to tests and independent review. Interruption tests use private
`cfg(test)` injection at the publication boundaries; they are not physical
process-kill or power-loss tests. The stage consumes the runtime-captured
canonical `--no-renames` patch, not an untrusted arbitrary patch import.
