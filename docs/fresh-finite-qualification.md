# Fresh finite verification qualification

This development path advances #132. It requires PR #202 and the verification
history from #198. It does not close #132 or qualify a real model.

Build the CLI with `qualification-fresh-t02` and enable
`"qualification": "fresh_t02_v1"` in the reviewed finite-entry configuration.
Use only the fixed public t02 source and reviewed corrected candidate.
The ordinary build rejects this configuration. No arbitrary oracle command,
external receipt, account or campaign registry is accepted.

The checker inherits the time budget of the existing verify node, including
the 30-second allocation for the 60-second public task. It cannot borrow the
worker allocation or fall back to a fixed larger timeout.

The diagnostic and runtime approval frames include `qualification` with the
checker identities. The parent must inspect those fields before echoing each
fresh digest. The normal scripted transport and existing private access rules
remain in force. A successful checker retains `fresh-t02-receipt.json` inside
the new private root before adding its attestation. The final entry report
contains `qualification.verification`, `history_retained: true`, refreshed
summary and events. The nested `result` remains the worker-completion snapshot:
its `result.validation` is still `unknown`, while the later recorded verdict is
`qualification.verification.validation`. Read that qualified field for technical
validation after checking. Exit 3 still means Verifying, not task acceptance.

## Required local evidence

- Ordinary build rejects the mode; no checker asset is embedded there.
- Fixture substitutions, stale session/report bindings, missing cases and
  forged outcomes refuse; changed or linked checker files never execute.
- Both approval challenges bind the displayed checking identities.
- Linux runner proves real success, failed exit, bounded timeout and signal
  refusal without an unbounded wait. The outer cell tears down descendants.
- A fresh Linux parent run proves the actual checker, 18 passing candidate
  cases, original-source failures, retained receipt, one matching history
  event and matching durable projection through the owning Runtime.
- Checker failure or receipt-storage refusal yields UNKNOWN and no successful
  validation attestation. Publication remains absent and objective UNKNOWN.

The Linux pilot must use the existing reviewed local image with no network,
no host mounts, unchanged private accesses, 8 GiB and zero swap. Never infer
these properties from the checker receipt alone. Keep its frozen source and
parent digest, actual cell configuration, peak memory, teardown, report and
receipt hashes. No model, provider or real registry is involved.

## Current evidence

The [complete-tree native qualification](bench/native-proof-link-2026-10-23/README.md)
proves the new binding at product source `f6d4b596`, with task source revision
`d674cbfdc683bc1a4e190934f594f925bf8ef143`. All 18 candidate criteria pass, against
six on the source; receipt, projection and attestation agree. Both cells are
stopped, with unchanged 8 GiB, zero swap and private accesses. Its source and
driver received independent review; the final build and run evidence were
checked by the primary session, with independent readback unavailable.

The [earlier 2026-10-23 Linux qualification](bench/fresh-runtime-verification-2026-10-23/README.md)
proved one fresh fixed-fixture run at source `313bb1fa`: 18 candidate cases pass,
the original source fails, and the owning Runtime retains the matching receipt,
verification and full attestation event. Both isolated cells are stopped. The
ordinary build refuses the qualification mode and excludes its checker assets.

This supplements the earlier external t02 and #203 parent evidence. It does not
qualify arbitrary tasks, human acceptance, global automatic integration or the
remaining recovery/lifecycle criteria. The run remains Verifying with objective
UNKNOWN and no source publication. #132 stays open.

See [ADR 0045](adr/0045-fresh-finite-entry.md) for the trust boundary.

## Complete public-tree binding

New receipts declare `revision_binding: complete_public_tree_v1`. Both approval
challenges expose this binding version and the expected complete source and
candidate Git trees. Receipts predating these required fields remain historical;
the new checker does not upgrade or accept them as complete-tree evidence.

The owning entry places its internally computed `source_state` in the report as
`source_revision`, using the same revision as the approved execution request.
Within the existing verify-node deadline, it checks that the source remains clean
at that exact commit and compares the commit's entire Git tree to the fixed public
source tree. It repeats this check before recording the attestation. An empty
commit with identical files cannot substitute for the originally bound revision.

The checker reconstructs every supplied public file in a disposable repository,
applies the captured patch to its private index, and checks the complete candidate
tree and canonical patch. All source files are private `0600` files, represented
as Git mode `100644`; no executable or symlink mode is admitted. The full trees
include the public context files as well as `lib.rs`. The candidate tree is a
reconstruction, not a published commit or an applied user-workspace correction.

The exact receipt binds source revision, both complete trees, patch, candidate,
session and checking identities. The durable attestation retains its digest.
Objective remains UNKNOWN and human acceptance absent. The verifier handle gains
no execution or application authority. The separate application/recovery fixtures
in #205 remain separate evidence until an accepted native candidate can enter an
explicitly approved integration path.
