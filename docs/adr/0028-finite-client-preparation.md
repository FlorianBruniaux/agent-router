---
adr: "0028"
title: "Bind finite-client preparation to an owned broker"
status: implemented
created: 2026-10-09
last_modified: 2026-10-09
scope: "Private review and fresh preparation checks only; no native launch or OS admission"
amended_by: []
supersedes: []
superseded_by: []
related: ["0021", "0022", "0026"]
---

# ADR 0028: Bind finite-client preparation to an owned broker

## Decision

Keep the finite worker selected in ADR 0026. Introduce a private typed
`FiniteClientManifest` in `router-types` and a preparation guard in
`router-runtime`. No helper protocol, runtime journal, capability schema or
sandbox authority changes. This is the first contract slice of F12a, not a
qualified native path.

`OwnedBroker` consumes a real IPv4 loopback TCP listener and assigns a random
instance digest. The endpoint and instance are both part of review. Neither a
port string nor a deserialized manifest can reconstruct this handle. The
approved preparation handle owns the listener; a validation guard borrows it.
Trusted controller code may accept connections on that borrowed listener.
No listener or descriptor is transferred to a worker by this API.

Review binds the manifest to the complete `PreparedRun`, with a separate digest
of stdin because serialized previews omit it. This covers receipt, source,
authorization digest, snapshot, model, effort, arguments, scope and bounds.
The manifest additionally binds task and host-snapshot identity, client/native/helper/wrapper
files, sandbox profile, native configuration, private-state directory and broker
instance. Files are canonical regular files with bounded reads and safe modes.
Executable identities and bytes, directory identities and modes are captured
and included in the approval digest. Open file and directory descriptors remain
owned by the handle, preventing reuse of an unlinked inode during its lifetime.
Assets and private state are outside the task workspace. Files cannot alias
through substituted symlinks or an identical replacement inode. Nonblocking
opens prevent a substituted FIFO from waiting indefinitely for a writer.

An exact digest approves this private preparation review. It does **not**
replace the durable runtime's `PendingRun` / `ApprovedRun` lifecycle. There is
no CLI loader, command builder, process spawn or provider-send method.

## Fresh proof boundary

`ApprovedFiniteClient::validate` rechecks the exact prepared bytes, current
reviewed host-snapshot identity and current model/effort support, owned listener, every file, workspace and private
state. It accepts only a live `StoredCapabilityProofVerifier`; historical
sequence views are refused. The verifier reopens the existing authenticated
store on every call, so a later failed attempt or removed store cannot be hidden
by a cached success.

Reuse the existing Network / LoopbackFetch evidence contract. Its expected
binding uses the client binary pin, native version, helper/wrapper/profile pins,
relative write scope and OS. The argument-template digest is domain-separated
`finite-client-review-v1` and covers the entire reviewed contract, including
the native binary, configuration, private state and broker instance. A proof
for another task or listener therefore cannot validate this preparation.
Unknown, unauthenticated, substituted, future, expired and revoked evidence is
refused. Expiry equal to the validation clock is refused conservatively.

A successful network canary still proves only its declared loopback effect.
This library does not issue production proofs and a MAC does not establish an
OS boundary. Synthetic proof issuance in tests is explicitly a fixture.

## Scope and consequences

F12a remains **NO_GO**. `ControllerBrokerOnly` is still unsupported by Seatbelt
and Bubblewrap. F02b is not attempted; no native model turn, useful native patch,
provider admission, accepted result or saving is established.

The guard does not bind a future command's concrete resource-limit arguments,
create isolated native state, render Codex configuration, or copy executable
bytes into an immutable execution view. Those producers and their actual
supervisor inputs must be tied to this reviewed contract before native wiring.
Memory enforcement, task-scoped containment, Mach/DNS, physical-send admission
and correlation remain separately qualified requirements.

Path checks and any future path-based exec are separate operations. This
change detects earlier substitutions; it does not exclude another writer
between validation and exec or while hashing a file. Trusted controller code
can explicitly duplicate a socket; this API does not claim isolation from that
trusted principal. The validation clock is supplied by trusted controller code, not parsed from a task.
Callers must retain the guard through their preparation
boundary, obtain a new guard after a relevant change or delay, and still use
the existing runtime approval and verified OS sandbox before any launch.

## Validation and next step

The contract tests use private synthetic files, a real loopback listener and
the existing authenticated private proof store. They cover exact preparation,
listener retention and release, stdin substitution despite equal previews,
wrong approval, endpoint/instance substitution, all six asset changes, inode
and symlink replacement, unsafe private state, task/model/effort/host changes,
stale/future/historical proofs, later authenticated failure, forged references,
missing stores, malformed or oversized input and a proof for another contract.
The existing product sandbox refusal is preserved. No live native/provider
fixture is used by these tests.

Next derive the actual finite-client command, private configuration and resource
limits from reviewed inputs; retain the broker through that composition;
qualify the selected OS boundary, then run the useful fake-provider task with
F06a correlation. Existing diagnostics remain dated evidence, not automatically
fresh proof for the new composition.

[Source-bound local verification](../bench/verification-finite-client-contract-2026-10-09.json)
records the frozen quality gate and its separate evidence-only follow-up.
