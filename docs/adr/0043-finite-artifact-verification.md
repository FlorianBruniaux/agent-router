---
adr: "0043"
title: Explicit verification of a finite private artifact
status: accepted
created: 2026-10-21
last_modified: 2026-10-22
scope: "In-memory explicit verifier handle; no automatic evaluator, publication or provider authority"
amended_by: []
supersedes: []
superseded_by: []
related: ["0037"]
---

# ADR 0043: Explicit verification of a finite private artifact

## Context

The qualified native path captures a private patch and stops at `Verifying`.
An external fixture checker can evaluate the export, but a finite run cannot
reopen as a generic `PendingRun` to call `record_verification`. That restriction
also prevents accidental source publication and must remain intact.

## Decision

Before consuming `ApprovedFiniteClientRun`, the explicit caller can retain a
`FiniteClientVerifier` from the owning Runtime. It borrows that Runtime and
keeps the approved run, definition, approval digest and verifier-separation
policy private. Creation requires the exact queued definition. The handle is
not serialized, reopened from JSON or converted into an execution/publication
handle. It cannot survive dropping the Runtime; durable verifier resumption is
outside this slice.

Reading the patch requires `Verifying`, matching definition and approval, and
`NamespaceTerminated` evidence from the existing cleanup/admission owner. The
existing bounded artifact reader rejects changed bytes, symlinks, unsafe modes
and oversized input. Empty or absent artifacts are refused. The returned bytes
are the checked diff; an unrelated display file is not validated by this read.

Recording rechecks the captured artifact and uses the existing atomic
verification transaction. The evidence digest and exact artifact digest are
required, as is a verifier distinct from the worker for independent phases.
UNKNOWN remains inconclusive, explicit failure becomes `VerificationFailed`,
and two Passed outcomes become `Verified`. Concurrent or duplicate conclusive
attestations cannot replace the first record.

The caller remains responsible for the evidence's provenance, required cases
and objective evaluation. Parsing provider output or an external JSON receipt
is not authorization to call this API. No evaluator or automatic importer is
installed by the Runtime. `Verified` does not mean human acceptance or permission to publish.

## Consequences

The finite handle still cannot call generic approval, execution or publication.
The unchanged private-only scope rejects `Mutation::Integrate`. The 8 GiB,
zero-swap and private-access contracts are unchanged. The useful-score replay
in PR #191 retains its historical UNKNOWN Runtime and null human verdict.

Store tests exercise explicit pass/fail/UNKNOWN, identity and artifact refusal,
changed files between read and record, and concurrent attestation. A public API
test rejects a different Runtime, queued artifacts and cancellation. The Linux
fixture checks the actual captured bytes through this handle and records only
UNKNOWN when running its local lifecycle checks. None of these checks evaluates
a real model task or establishes a human verdict.

The [local verification record](../bench/finite-artifact-verification-2026-10-21.json)
binds the modified Rust sources, workspace gate and five native Linux cases.

The [fixed public-score example caller](../bench/finite-public-evaluation-2026-10-21/README.md)
opts in to executing the embedded reviewed checker after worker cleanup. It
records technical validation only; the frozen campaign objective stays UNKNOWN.
Its private receipt is produced by that invocation, not imported as authority.

## Amendment: retain accepted attestations, 2026-10-22

The mutable current verification alone discarded a superseded inconclusive
record, while its event retained only an evidence digest. The shared Runtime
transaction now appends the full bounded record as `verification.attested.v1`
before commit, preserving the current projection and existing digest event.
This applies to generic and finite explicit verifiers. Reuse the event chain
and its private storage instead of adding a history database or migration.

Before overwriting a projection different from the latest full record, retain it
as `verification.legacy_snapshot.v1`, explicitly an observation at update time.
Compare the full record, including when a legacy writer ran between two new
writers with the same evidence digest and different outcomes.
Earlier lost verdicts remain UNKNOWN; opening a store does not synthesize
history. See the [event contract](../reference/json-contracts.md#durable-verification-attestations).
Rollback, damaged-chain refusal, size limits and reopen tests cover the write.
No authority, sandbox or human-acceptance semantics change. The prior native
qualification remains tied to its recorded source, not this amended writer.
