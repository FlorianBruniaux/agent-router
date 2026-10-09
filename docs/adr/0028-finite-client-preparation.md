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
sandbox authority changes. This prepares the F12a composition; native OS qualification remains open.

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
no CLI loader, executable command conversion, process spawn or provider-send method.

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
`finite-client-review-v2` and covers the entire reviewed contract, including
the native binary, configuration, private state, broker instance and exact resource-helper arguments. The adapter binding is `finite-client-contract-v2`; old v1 proof bindings cannot qualify the new preparation. A proof
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

Concrete resource limits reuse `ResourceLimits::args`, including CPU, address
space, open files, processes and file size. Their exact flags enter the approval
digest. `validate_with_resources` refuses changed current limits; the default
review retains the existing resource defaults, including requested memory.
Nothing silently disables memory enforcement on macOS.

`OwnedBroker::prepare_codex_state` uses the existing `HostState` lifecycle in a
private store disjoint from the workspace. It creates a 0700 home and temporary
directory, and a 0400 `config.toml`, without importing environment or credentials.
The configuration selects the reviewed model/effort and the retained loopback
broker's local fixture endpoint. Shell tools, apps, web search and retries are
disabled. This prepares configuration bytes; native consumption is not observed.
`with_codex_state` consumes that producer's nonserializable owner and checks
configuration, model and endpoint binding. Approval retains this state, and the
existing cleanup runs when the owner is dropped. Temporary-directory identity
and modes are checked alongside all previous file and directory pins.

Only a validation guard retaining generated state can present a private
`FiniteClientLaunchPlan`. On macOS it derives the resource-helper / Seatbelt /
finite-worker argument vector, plus the native `app-server --listen stdio://`
invocation. Both clear the inherited environment and use the generated home.
The preview contains the stdin digest, timeout and output bound, never task
bytes. It refreshes all filesystem identities before presentation. Other OSes
refuse this preview until their concrete wrapper template is qualified.
The serializable plan has no deserializer or conversion to a process command;
a caller still needs a fresh validated context and durable runtime authority.

Executable bytes are not copied into an immutable execution view in this slice.
The plan is not connected to `Runtime::execute`, physical-send admission or a
qualified worker protocol. Rendering resource flags proves no enforcement.
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

Next connect the private preview to a verified supervisor and immutable executable
view while retaining the broker and generated state. Qualify the selected OS
boundary and requested resource limits, then run the useful fake-provider task with
F06a correlation. Existing diagnostics remain dated evidence, not automatically
fresh proof for the new composition.

[Source-bound local verification](../bench/verification-finite-client-contract-2026-10-09.json)
records the frozen quality gate and its separate evidence-only follow-up.

The second slice also tests all 5 resource-limit substitutions, unchanged-listener
approval rebinding, generated-state ownership and cleanup, configuration/model/
listener mismatch, temporary-directory substitution and workspace drift after
validation. Its verification is recorded separately from the first frozen gate.

[Source-bound state/resource/launch-preview verification](../bench/verification-finite-client-preparation-2026-10-09.json) records this second slice.
