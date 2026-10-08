# Pinned ACP component comparison, 8 October 2026

The unchanged ACP native reader fails Agent Router's complete-evidence requirement: an invalid JSON line disappears, then the pending call accepts a later success. This executed counterexample supports rejecting that unchanged reader for the owned-entry contract. It does not establish full ACP/native compatibility or complete the nine-scenario session comparison in #131.

A dedicated direct client with the broker and controlled cancellation remains the proposed choice for **simulated F02a investigation**, contingent on its nine fixture domains, the three reviewed native cases (allow, zero local send budget, broker unavailable before startup with cancellation), and independent review. This report does not assert that those direct checks passed. F03b and #132 are not prerequisites for choosing that bounded entry. The original F03a v2 NO_GO and global native-admission NO_GO remain unchanged.

[Structured component results](acp-component-results.json) contain each observation, its AR invariant disposition, source hashes, dependency integrity and private harness hashes. A `checks: PASS` means the asserted observation was reproduced, including adverse behavior. It does not mean the corresponding AR invariant passed.

## Executed source and dependency boundary

ACP commit: `ca1d97173ad37b471d5a4e5847725a4657d34e29`. Repository reference for this comparison: `a0b7417f42423391eee634929ae759bd7475b0a9`. Six complete upstream modules were loaded unchanged:

| Executed source | SHA256 |
| --- | --- |
| [src/CodexAppServerClient.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/CodexAppServerClient.ts) | `538107f45e0d74027cee1bcb70cf8ccb470db022e90ddb9721195c2d71240775` |
| [src/StdUtils.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/StdUtils.ts) | `8911d11b9908cd68f6706fee0093aa94725b83df90f30017186ba1d676af5d9c` |
| [src/Logger.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/Logger.ts) | `73e8a5441a721082417053639490f7301d1fa4a2a7374854f61a32623d4028be` |
| [src/mcp/McpOauthCompletions.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/mcp/McpOauthCompletions.ts) | `e0b1303c5c92d1988e494530ea8f3e196861492552b3f30ac1b16a439e6dd905` |
| [src/mcp/McpStartupTracker.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/mcp/McpStartupTracker.ts) | `4b5615a9b59a22e7fb147748a75387bf178c1bb7bee9e354051d3fbc99520029` |
| [src/subagents/PendingNotificationBuffer.ts](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/subagents/PendingNotificationBuffer.ts) | `f9aedc0dc1b2b5cc298320ac641e4b4b177628933ea075278d62611b79597538` |

The JSON result also records hashes and links for the inspected high-level server, client, approval handler, event handler, mode presets and native connection wiring. Inspection of these files is not execution of their methods.

Dependency provenance is the [pinned package lock](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/package-lock.json), SHA256 `d1b3c4a9526a98446d63352b2ae062716223f2ab00ca769d5027bf3d5a31a7e7`. Only the following archives were acquired, with their lock SHA512 integrity verified. No npm lifecycle scripts or global installation ran; the lock's native Codex 0.159.1 package was not installed or executed.

| Dependency | Version | Verified lock integrity |
| --- | --- | --- |
| [vscode-jsonrpc](https://registry.npmjs.org/vscode-jsonrpc/-/vscode-jsonrpc-9.0.2.tgz) | 9.0.2 | `sha512-SbQSV9yRemARxeXw6LU5sS6Zq0e9/DgCCX5yelH263ZQWukbTk8EF8fjTrr1dziasf4GwlJbvTwFnTrnQFWZXQ==` |
| [@agentclientprotocol/sdk](https://registry.npmjs.org/@agentclientprotocol/sdk/-/sdk-1.5.0.tgz) | 1.5.0 | `sha512-524jwbB2iYWA+kWWyv9fhKbhU89dH/lu9u5EXwVNmfYzopV8BujCDxByBDZhxRUkB7RWIJzISCnwivdDk+bdVg==` |
| [zod](https://registry.npmjs.org/zod/-/zod-4.6.5.tgz) | 4.6.5 | `sha512-v5l/aFXZQeai4awLbOpSoHecE9UiMrnfx75tEXLjNonXVARxQ5mOeipTjROUchszUNCqnE+hqAMujRsRHsut2Q==` |

The SDK declares the zod peer range `^3.25.0 || ^4.0.0`; the exact locked version above was used. These packages are a private diagnostic import closure, not proposed product dependencies. The report makes no measured code, dependency-cost or maintenance-saving claim.

## Nine observations, with their limits

| Scenario | Observed behavior on real components | AR invariant and evidence limit |
| --- | --- | --- |
| Permission refusal | With no registered handler, command approval returns `cancel`; permission approval returns empty permissions, turn scope and `strictAutoReview: false` | Denial fallback demonstrated. The ACP UI permission-response route is not executed. |
| Unsupported model/effort | `CodexAppServerClient.turnStart` forwards synthetic unadmitted values unchanged | No gate at this component. Higher server setters validate selections in inspected code, so this is not a whole-ACP refusal failure. |
| Rerouting | `model/rerouted` reaches the registered session callback | Notification preserved. The higher event handler's notice/thought behavior is inspected only; full session refusal is not demonstrated. |
| RPC IDs | Wrong and duplicate response IDs do not settle another call; an interleaved notification survives | Tested correlation cases pass at the real reader/JSON-RPC layer. No artifact-generation claim. |
| Malformed messages | Invalid JSON is dropped, the call stays pending, then accepts a valid success | Complete-evidence invariant violated at the native reader. No durable loss status reaches the caller. |
| Buffer overflow | An oversized notification is dropped; `take()` returns an empty list without a loss record | Loss-visible accounting violated for `PendingNotificationBuffer`. This is the subagent buffer, not every main-session receive path. |
| Cancellation | Interrupt acknowledgement leaves `runTurn` pending; a native-shaped interrupted terminal resolves it | Ack/terminal separation demonstrated at the low-level API. No actual native or process cleanup. |
| Late result | A completed terminal for a marked-stale turn neither rewrites its resolved interrupted result nor reaches the session callback | Tested stale-turn behavior passes. Raw transport telemetry can still observe the message; session replacement and artifact application are untested. |
| Ambiguous interruption | An interrupt RPC error leaves completion pending; explicitly calling `resolveTurnInterrupted` then resolves it without a native terminal | Local synthetic resolution is not native outcome evidence. The high-level server's decision to call that resolver is inspected only. |

The missing high-level paths are concrete. [CodexAcpServer](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/CodexAcpServer.ts) installs [CodexApprovalHandler](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/permissions/CodexApprovalHandler.ts) during prompt handling; the handler calls ACP `session/request_permission`. Its model/effort setters validate the available inventory. [CodexEventHandler](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/CodexEventHandler.ts) handles `model/rerouted`. The server's interrupt path calls the local resolver in `finally`; its interrupt request handling can log and return after an error. None of those high-level method flows was executed by this harness. Testing the lower client's absent-handler branch or direct `turnStart` call cannot substitute for them.

## Malformed-line counterexample

This normalized transcript uses synthetic RPC ID `1`; it contains no native identity or user task. With an `initialize` request pending, write these two incoming lines through the real reader:

```text
{not-json}
{"id":1,"result":{"correct":true}}
```

After the first line the promise is still pending at the 15 ms observation. After the second, the observed result is `{"correct":true}`. The invalid line produces logger-only handling, with no typed or durable loss state exposed by the reader. [CodexJsonRpcConnection.startCodexConnection](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/CodexJsonRpcConnection.ts) wires that same reader to native stdout in the inspected source. This is an executed necessary counterexample on the real shared component, not a native session result. Rejecting the unchanged candidate does not require first making it compatible with native 0.159.2.

## Method and reproduction boundary

The retained private harness constructs two real `vscode-jsonrpc` message connections over Node `PassThrough` streams using ACP's `createJSONRPCReader` and `createJSONRPCWriter`. It constructs the real `CodexAppServerClient` on one connection and scripts the other as the native peer. A Node loader uses `registerHooks` and `stripTypeScriptTypes(mode: 'transform')` plus relative `.ts` resolution; it replaces no upstream class behavior. The buffer case instantiates the real `PendingNotificationBuffer` with a 100-byte cap and supplies a notification containing a 200-character value.

To reconstruct the checks, acquire the six complete modules and exact dependency archives above, verify their hashes/integrities, and wire those in-memory streams. Drive each row's method and event sequence. Hold the initialize response when testing IDs and malformed input; deliver the correct response only after observing the pending state. For cancellation, start `runTurn`, acknowledge `turnInterrupt`, check that completion remains pending, then deliver `turn/completed` with interrupted status. For the late-result case call `markTurnStale` and `resolveTurnInterrupted` before a completed terminal. For ambiguity, make the interrupt peer request fail, observe pending completion, then explicitly invoke the resolver. These are characterization assertions on actual source, not a reimplementation of ACP.

The successful run used Node 24.16.0, `node --max-old-space-size=128 component-comparison.mjs`, a disposable HOME, minimal environment and a 15-second outer subprocess deadline. Pending-state assertions use 15 ms observations; they do not establish universal timing guarantees. Each output stream remained below 1 MiB. All nine assertions passed with exit 0. There were zero native launches, socket listeners and provider requests. The harness used no networking during execution; no OS network-containment proof is claimed. Node emitted its experimental TypeScript-transform warning.

The first harness attempt failed on an incorrect provisional package entry; using the package's actual `vscode-jsonrpc/node` export fixed the harness without changing upstream source. The final run added duplicate-ID/interleaved-notification checks and the schema-declared reroute reason `highRiskCyberActivity`. The public JSON retains hashes of the final private harness, loader and raw result for audit. Scripts remain private because this diagnostic is not a self-contained public rerun package. No third-party source, private paths or raw session captures are copied here.

## Decision scope and remaining comparison

A positive direct selection must fix the submission owner, exact client/native/broker pins, explicit approval and sandbox presets, task/thread/turn binding, and the permitted simulated artifact stages. It may authorize the isolated F02a investigation. Useful patch application, financial accounting, paid execution and native F12a admission remain separate work and require their own evidence. A zero local HTTP budget proves send refusal; the direct client's unsupported-selection fixture must independently prove refusal before any transport send.

The direct test set must also cover a correlated `model/rerouted` notification during the turn followed by an apparent completed terminal. Checking only the selection returned by `thread/start` misses that case. This report does not claim the candidate's correction or review passed.

Full nine-scenario ACP session parity remains **INCOMPLETE**, and native compatibility remains **UNKNOWN**. Completing that comparison requires executing the real high-level classes and their import closure against the same scripted peer, including permission responses, model setters, reroute handling and high-level cancellation. Extracted/reimplemented methods would not qualify. These missing cells must stay visible in #131 even if the narrower direct choice is accepted for simulated F02a.
