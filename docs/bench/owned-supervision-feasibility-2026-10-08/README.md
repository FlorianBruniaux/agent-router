# Owned app-server supervision: composition feasibility

8 October 2026. F12a remains **NO_GO** for the unchanged selected client and
product supervisors. This diagnostic starts from the F02a patch-publication
delivery merged by [PR #144](https://github.com/FlorianBruniaux/agent-router/pull/144)
at `5ba1a694daae90ddb03552f445f31465a64e8ed6`.

## Question and evidence owner

Can the dedicated client selected in ADR 0026 use the real runtime OS/resource
supervisors without widening its broker-only authority or losing the interactive
JSON-RPC conversation?

The explicitly invoked Cargo example in
[`owned_supervision.rs`](../../../crates/router-runtime/examples/owned_supervision.rs)
uses the public `SeatbeltSandbox`, `ResourceSupervisor`, `SandboxPolicy` and
`probe_native_version` APIs. It does not recreate their sandbox profiles, install
a hook, import personal settings/account state, or submit a thread/model turn.
Its constructed `PreparedRun` is a diagnostic fixture, not an approved product
request or an execution receipt. This is external composition evidence.

## Confirmed blockers

1. `NetworkAuthority::ControllerBrokerOnly` cannot construct a Seatbelt command.
   The current enum contains no endpoint identity. The runtime refuses it before
   spawning the app-server. Replacing it with `PhaseGranted` would grant broad
   network access and is not an acceptable qualification shortcut.
2. The actual resource helper's `framed-v1` protocol supplies one finite payload
   and then closes the worker's stdin. Keeping the controller life pipe open does
   not keep the native RPC session open. The dedicated client needs subsequent
   requests constructed from earlier thread/turn replies; those requests cannot
   be supplied by this finite-input protocol.

The native diagnostic separately records the exact Codex version/hash, the
supervised version result and cleanup, the inherited Seatbelt canary results,
the full address-space probe, initialization output and helper completion.
When address-space enforcement is unavailable, a second diagnostic explicitly
uses `address_space_bytes: None`. That observation does not weaken product
admission or qualify a memory cap. A successful limit-setting probe on another
system would also need an allocation refusal test.

Process-group cleanup is distinct from exhaustive descendant containment.
The existing macOS sandbox's Mach-service/DNS limitation still applies. No
F02b useful native task, Linux/Claude parity, complete F12a qualification or
financial admission follows from these observations.

## Next implementation contract

Establish broker confinement and choose the supervision owner before retrying F12a:

- A typed, approval-bound broker endpoint and lifetime. Bind the endpoint,
  transport identity, executable/configuration pins and permitted task to fresh
  evidence. Deny direct provider egress and listener retargeting; retain the
  conservative reservation when the broker or native effect is uncertain.
  Do not map broker-only authority to a broad network grant.
- First evaluate supervising the bounded dedicated client as the finite worker.
  That client can own incremental app-server RPC inside its supervised task;
  `framed-v1` need not itself become duplex. Bind the client/script and native
  executable, private host state, broker endpoint and approved task together.
  Qualify its descendants and teardown. If the required ownership cannot be
  preserved that way, a separate bounded duplex protocol needs an independent
  parent-life channel, durable lease and explicit input/output backpressure.
  Both options must test permission refusal, parent death, timeout, malformed
  input, lost output, cleanup and restart. Cancellation acknowledgement alone
  cannot release the lease. This diagnostic does not choose between them.

Then compose the actual selected client, broker and supervisor on the pinned
host. Exercise allowed/refused transport, the useful patch, interruption and
removal, and correlate the external capture through F06a. The first successful
composition must retain the F02a publication digest and independent artifact
checks. F03b and the real-trial gates remain separate.

## Follow-up after publication hardening

PR #147, merged at `88c8d5f007877402f8eaf98722e2dd90fc35b16e`, strengthens the
local diagnostic client's `stop()` contract: success requires observed group
termination. The reconciled lifecycle test checks this immediately after return;
it does not add a grace period that could hide an early return. The historical
race/deadline reports describe their original sources, not this combined tree.

A read-only architecture review selects the finite-client shape for the next
bounded implementation: the client can own incremental child RPC behind the
existing finite helper input. This is a design choice, not an executed worker.
The native child must inherit the supervised group; the standalone diagnostic's
`start_new_session=True` cannot be reused inside that worker. Client and native
executable identities need an explicit binding because current host-state grants
are keyed to the prepared program.

Broker confinement still requires more than an allowed TCP port. Its identity
must survive listener loss/rebinding for the entire live attempt, either through
a retained listener owner or a transport the native client authenticates. Neither
mechanism is qualified here. The observed macOS address-space limitation, broad
Mach-service boundary and absence of exhaustive descendant containment remain
open. A finite input fix alone cannot make F12a positive.

## Reproduce

Build the existing helper, then run only the explicitly opted-in diagnostic.
`AR_CODEX_NATIVE` must be the native Codex 0.159.2 binary, SHA-256
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
`AR_SANDBOX_HELPER` must be an absolute path to the freshly built helper.
`AR_F12_PRIVATE` must be an absolute, fresh private directory; the example refuses
an existing destination and retains its report there.

```sh
rtk proxy cargo build -p router-sandbox-helper --locked
AR_CODEX_NATIVE="$AR_VERIFIED_CODEX" \
AR_SANDBOX_HELPER="$AR_VERIFIED_HELPER" \
AR_F12_PRIVATE="$AR_NEW_PRIVATE_DIRECTORY" \
rtk proxy cargo run -p router-runtime --example owned_supervision --locked
```

The ordinary all-targets quality gate compiles this example. It is not added to
the product fixture matrix or the native `--ignored` test batch. Only an explicit
successful diagnostic invocation establishes its observations. Exit 0 here means
the current blockers were reproduced, not that F12a passed.

[Native results](native-results.json) preserve the observed report and source hashes.

The [reconciliation record](verification-reconciled.json) binds a fresh complete local gate and the independent review to the combined source. Earlier temporary logs were lost and are not reused as evidence. This record preserves the immediate post-stop assertion and does not change the historical native NO_GO.
