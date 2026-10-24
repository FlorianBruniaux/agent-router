---
adr: "0046"
title: Explicit controller acceptance and private candidate application
status: proposed
created: 2026-10-23
last_modified: 2026-10-23
scope: "One existing private output after finite namespace cleanup; no external repository or automatic resume"
amended_by: []
supersedes: []
superseded_by: []
related: ["0043", "0045"]
---

# ADR 0046: Explicit controller acceptance and private candidate application

## Context

The fresh t02 entry records technical verification against an exact source
revision and complete Git trees. Its verifier cannot apply a patch and its
worker remains restricted to private artifact production. Connecting acceptance
to application must preserve those boundaries described in ADR 0043 and ADR 0045.

## Decision

`FiniteClientApplication` is a separate, nonserializable controller handle
retained from an approved finite run before dispatch. It retains the prepared
source identity and original output binding without exposing a generic
`PendingRun`. It supports only an existing 0600 file in a private source directory
beside the owning Runtime store. The CLI uses only the freshly created public
packet repository, never a supplied user checkout. Worker permissions do not
include `Integrate`; verifier authority remains unchanged.

After namespace cleanup, a Passed technical attestation with UNKNOWN objective
permits a review. Reconstruction uses the captured patch and a private Git
index. The review binds the source revision, complete source/candidate trees,
single output, preimage/candidate hashes, patch, complete technical record,
run definition, original execution approval and source directory identity.
Changed paths and file modes must remain within the private replacement scope.

The opt-in qualification mode `fresh_t02_apply_v1` emits a candidate review and
a third approval challenge, `accept_apply`. It binds the current session,
submission, configuration and review digest. The broker returns its input only
after its reader thread stops; no concurrent reader handles the approval.
The controller checks the source and configuration again after the response.
Ordinary builds continue to exclude the qualification mode and oracle.

Explicit approval accepts the candidate, with provenance
`explicit_local_controller`, and attempts its application. The technical record
is compared transactionally before recording the acceptance attestation. Earlier
attestations remain in history. `Verified` means candidate acceptance, not
completed file application, human acceptance or global task success.

Application consumes a durable one-shot intent before writing through an open
descriptor relative to the retained directory. Source identity, preimage, owner,
single-link status and mode are checked before mutation; bytes and identity are
checked again afterward. The operation preserves mode 0600, preserves HEAD
and Git index entries, and creates no publication ref. Only a completed checked write
records `integration.applied` and reports `application.status: applied`.

## Consequences

The private source has one exclusive controller writer after the worker namespace
terminates. This does not protect against arbitrary processes of the same user
editing the same inode concurrently. File identity and preimage refusals detect
the tested substitutions; they do not establish a universal filesystem lock.

In-place writes are not crash-atomic. An interruption can leave the source
unchanged, changed or partially written after candidate acceptance. The intent
stays consumed, an absent applied event remains inconclusive, and no automatic
reapplication or rollback occurs. The entry refuses existing roots and has no
serialized handle import. A retry cannot erase later user edits.

Human acceptance remains absent in scripted qualification. Applied-byte checks,
real native execution and interrupted-owner recovery are separate evidence
claims. A simulated callback interruption does not qualify native process loss.
Linux qualification retains 8 GiB, zero swap, the existing private-access rules
and a network-disabled outer cell. No model or real registry is required.

## Rejected options

Adding integration rights to the worker or exporting its generic pending run
would widen execution authority. Promoting a technical receipt automatically
would bypass the new acceptance decision. Replaying an incomplete intent would
risk overwriting source changes made after interruption.
