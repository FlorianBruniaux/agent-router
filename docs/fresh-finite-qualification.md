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

The [2026-10-23 Linux qualification](bench/fresh-runtime-verification-2026-10-23/README.md)
proves one fresh fixed-fixture run at source `313bb1fa`: 18 candidate cases pass,
the original source fails, and the owning Runtime retains the matching receipt,
verification and full attestation event. Both isolated cells are stopped. The
ordinary build refuses the qualification mode and excludes its checker assets.

This supplements the earlier external t02 and #203 parent evidence. It does not
qualify arbitrary tasks, human acceptance, global automatic integration or the
remaining recovery/lifecycle criteria. The run remains Verifying with objective
UNKNOWN and no source publication. #132 stays open.

See [ADR 0045](adr/0045-fresh-finite-entry.md) for the trust boundary.
