# Finite client under native supervision, 8 October 2026

The resumed pinned macOS diagnostic completed successfully. The finite worker received its complete input and EOF before native spawn, then owned two incremental Codex RPC requests: initialize followed by thread/start. The actual ResourceSupervisor and unchanged framed-v1 helper observed exit 0 and a group-clean lease. This establishes the feasibility of that bounded client arrangement; it does not admit a product run or every future task.

## Requalification after concurrent integration

The remote branch advanced during verification with merged publication hardening (#147) and native diagnostic reconciliation at `1351f08`. Both are preserved in merge `a0f60e99efcb81c158e8a49a12224f753265d0c1`. A newly rebuilt finite client completed the same native/canary observations in a fresh private directory on this integrated source; the nine targeted tests passed again. [Reconciled native report](native-results-reconciled.json) and [fresh source verification](verification-reconciled.json) are the current evidence. The first [verification](verification.json) and [native report](native-results.json) retain their earlier source scope. F12a remains NO_GO and no model turn was submitted.

The source-owned paths are this evidence directory, `crates/router-runtime/examples/finite_client_supervision.rs` and the continuation paragraph in the finalization plan. Product libraries, adapters, CLI, harness and global settings are unchanged.

## What was observed

- Codex 0.159.2 native digest `16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704` was checked before/after. A fresh private executable copy was read-only. Client/helper hashes and selected configuration bytes were checked.
- The returned thread metadata matched requested gpt-6.1-sol/low, local_fixture, exact private workspace, ephemeral thread, on-request/user and readOnly with networkAccess false. No turn/start was sent. Returned effort is not proof of inference effort.
- A trusted diagnostic variation adds only TCP localhost at the selected listener port to the real Seatbelt deny profile. Both listeners were reachable in outside controls. The supervised client reached the selected one; the other connection failed with EPERM and its accepted count remained at the control count. The listener is a banner canary, not a provider HTTP broker.
- Read/write allow and refusal canaries passed. Native and helper exit 0 and the lease are distinct from the RPC responses. Group-clean does not prove exhaustive containment.
- The fixture binding includes PreparedRun fields and a separate stdin digest because PreparedRun serialization omits stdin. The worker refuses changed bytes, unknown/duplicate fields, expired or implausibly future input, wrong pin/ports and oversized input before its side effects. Native thread identity and contradictory metadata have explicit refusal tests.

## Negative history and diagnostic limits

Before the resumed capture, a 256-process diagnostic reached native spawn and returned EAGAIN while 1,327 user processes were observed. A 2,048-process variant reached RPC but hit the 12-second outer deadline; instrumentation measured about 9.9 seconds for the preceding validation on that build. Its candidate listener/file controls and group-clean lease were observed. Those temporary logs are no longer available after resume, so their detailed integrity is UNKNOWN. They are retained as observed history, not silently counted as successful captures.

The resumed capture uses a 45-second total diagnostic bound, a six-second RPC bound and explicit NPROC 2,048. Pre-RPC validation took about 6.5 seconds in its private trace. NPROC is a user-wide ceiling, not a task process count. Address-space enforcement is explicitly not requested. Product ResourceLimits defaults are unchanged. The broader Mach lookup granted by the existing Seatbelt profile leaves the Mach/DNS boundary unqualified.

**F12a remains NO_GO; F02b is NOT_ATTEMPTED.** The runtime's ControllerBrokerOnly implementation remains unsupported. The trusted candidate wrapper validates its exact diagnostic request, invokes the real Seatbelt command with Deny, then adds the selected TCP canary rule. This is an explicit diagnostic variation; synthetic request fields, freshness checks and a digest do not replace product approval or authenticated capability evidence.

## Next product contract

Keep the dedicated client as the finite worker candidate. Before product wiring, bind broker transport, client/native/helper/profile/configuration identities, private state and task/route scope to the exact reviewed approval and fresh authenticated evidence. Refuse changed endpoint, stale proof or unsupported required OS limits before spawn. Qualify Mach/DNS and task-level containment separately. A native useful-patch task with a fake provider follows positive selected-host supervision and F06a correlation; real-provider work still requires its sealed manifest and exact approval.

The positive finite-worker observation removes the need to assume a duplex-helper rewrite. It does not waive the remaining OS, broker or task acceptance.

## Reproduction

The example is compiled by ordinary source checks and invoked explicitly. It is not part of the native ignored-test batch and contains no provider credentials or model turn.

```sh
cargo test -p router-runtime --example finite_client_supervision --locked
AR_CODEX_NATIVE=/absolute/pinned/codex AR_SANDBOX_HELPER=/absolute/pinned/helper AR_FINITE_PRIVATE=/private/tmp/new-unused-directory cargo run -p router-runtime --example finite_client_supervision --locked
```

Private diagnostic files are created in a fresh directory. Do not overwrite a previous capture. The public [native report](native-results.json) contains projected fields and hashes, never native IDs, temporary paths or raw protocol text. [Verification](verification.json) records the current checks separately from prior delivery proofs.

The TCP SBPL form was checked against Apple's [CUPS sandbox generator](https://github.com/apple/cups/blob/master/scheduler/process.c), then compiled and exercised by this actual native capture. Only this tested listener/canary scope is claimed.
