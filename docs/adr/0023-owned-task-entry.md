---
adr: "0023"
title: Owned task entry stops before native activation
status: accepted
created: 2026-10-07
last_modified: 2026-10-07
scope: "NO-GO for unmodified direct Codex app-server and codex-acp under the required physical-send admission contract. Source and local schema evidence only; equivalent behavioral fixture comparison remains incomplete."
amended_by: []
supersedes: []
superseded_by: []
related: ["0022"]
---

# ADR 0023: Owned task entry stops before native activation

## Decision

Select **neither unmodified direct Codex app-server nor codex-acp for native
activation** under [issue #131](https://github.com/FlorianBruniaux/agent-router/issues/131).
Both let a dedicated client submit a task before a parent model request starts.
Neither inspected client interface grants Agent Router admission control before
each physical provider send, including native retries. The required budget
boundary is therefore unavailable on the inspected client surfaces.

This is an early NO-GO decision, not a claim that a future broker cannot provide
that boundary. [ADR 0022](0022-provider-egress.md) defines the controller, broker
and executor separation needed to investigate it. That broker and its OS
admission are not established by this decision. Do not extend either transport
into the useful-patch path on the strength of schema discovery or mocked tests.

An explicit dedicated entry remains the intended surface: the user opens one
client and submits tasks there; the client routes before creating a native turn.
It does not intercept the existing Codex desktop. A prompt hook cannot own this
submission boundary: the [U07 failure experiment](../bench/native-integration-u07/observations.md)
observed a parent request after each of five unexpected hook failures.

## Physical sends are below both candidate interfaces

The installed native executable reported Codex `0.159.2`; its SHA-256 is
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
Version, help and experimental JSON-schema generation all exited zero in
fresh disposable state. The generated `ServerRequest` enum contains eleven
methods, including approval and dynamic-tool callbacks, with no provider-send
admission callback. The [recorded schema projection and reproducible check](../bench/owned-entry-2026-10-07/README.md)
retain exact schema hashes. This establishes the inspected API surface only.

Pinned native source supplies the missing mechanism:

- The Responses endpoint delegates HTTP streaming to an endpoint session;
  that session applies the provider retry policy around `transport.stream`.
  See [Responses dispatch, lines 135-150](https://github.com/openai/codex/blob/ff6aec96948b70d94983af2641a6b67c94faeff5/codex-rs/codex-api/src/endpoint/responses.rs#L135-L150)
  and [session streaming, lines 133-151](https://github.com/openai/codex/blob/ff6aec96948b70d94983af2641a6b67c94faeff5/codex-rs/codex-api/src/endpoint/session.rs#L133-L151).
- The native retry helper creates and executes a request again for each
  eligible retry. [Retry loop, lines 84-117](https://github.com/openai/codex/blob/ff6aec96948b70d94983af2641a6b67c94faeff5/codex-rs/codex-client/src/retry.rs#L84-L117).
  The model client also continues after recoverable authentication errors.
  [Authentication recovery, lines 1777-1803](https://github.com/openai/codex/blob/ff6aec96948b70d94983af2641a6b67c94faeff5/codex-rs/core/src/client.rs#L1777-L1803).
- codex-acp launches `codex app-server` and connects JSON-RPC stdin/stdout.
  It retains that native request machinery. [ACP connection, lines 15-42](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/CodexJsonRpcConnection.ts#L15-L42).

A `turn/start` gate, turn cancellation or usage notification cannot therefore
serve as the required per-send permit. Disabling one retry setting would not
establish coverage of continuations, authentication recovery, reconnects and
other provider paths. This audit does not qualify all those paths dynamically.

The existing manifest represents parent-declared `cost_microusd` and `tokens`
allocations per supervised runtime attempt. It commits the allocation before
process spawn. [Session allocation contract](../session-budgets.md) explicitly
does not meter invoices or enforce account quotas. Keep that unit and scope;
do not reinterpret one runtime attempt as one physical request. A future
physical-send contract needs its own request identity, attempt ordinal,
conservative reservation and ambiguous-settlement rules. A subscription quota
has no USD conversion established here. A budget that is merely tracked is a
different scope and has not been approved by this ADR.

## Candidate comparison and compatibility

| Property | Direct app-server | codex-acp |
| --- | --- | --- |
| Submission owner | Dedicated AR client before `thread/start` and `turn/start` | Dedicated ACP client before `session/prompt`, adapter then native app-server |
| Existing evidence | Native 0.159.2 schema discovery here; earlier U02/U06 local scripted-provider diagnostics | Pinned source `ca1d97173ad37b471d5a4e5847725a4657d34e29`; no native ACP run here |
| Version boundary | Exact native executable and generated schema hashes | Package 2.1.1 lock selects Codex 0.159.1; compatibility with 0.159.2 is UNKNOWN |
| Parameter handling | AR must construct and validate typed model, effort, approval and sandbox fields | Existing model/effort/config translation, with native choices still needing reconciliation |
| Code avoided | ACP session/config translation and its dependency layer | Some client session/config/permission translation; AR supervision, journal and gates remain necessary |
| Dependencies retained | Existing Rust runtime plus a new bounded JSON-RPC transport if later admitted | Node, ACP SDK, vscode-jsonrpc, codex-acp and native Codex; existing AR runtime still required |
| Evidence at risk | AR must preserve all relevant native messages itself | Buffer can drop notifications; cancel can resolve locally despite interrupt failure; reroute needs a structured AR receipt |
| Physical-send admission | Not exposed by inspected server-request interface | Same native gap, no independent per-send gate established |

No line-count, effort or performance saving is inferred from this comparison.
Direct app-server is the narrower candidate to reconsider after broker admission,
because ACP does not remove AR's authority or evidence responsibilities. This
preference is not a GO or an implementation authorization.

ACP's [default mode](https://github.com/agentclientprotocol/codex-acp/blob/ca1d97173ad37b471d5a4e5847725a4657d34e29/src/AgentMode.ts#L39-L95)
uses `auto_review`. A future AR route must explicitly choose
`approvalPolicy: "on-request"` and `approvalsReviewer: "user"`, with read-only
planning and verification, then workspace-write execution limited to its owned
workspace. Tool network access remains false unless separately approved.
Permission requests without matching AR authority are refused. These native
settings supplement the runtime sandbox; they do not attest its enforcement.
The ACP presets' writable roots and temporary-directory allowances must be
reconciled with that narrower approved workspace before use.

## Minimal future useful artifact and ownership

After a positive replacement decision, the smallest useful path is one task,
one selected execution stage, one supervised isolated workspace and one patch.
Planning and verification need not launch additional models. The entry binds
its decision and exact model/effort to the native thread/turn, preserves native
identities and fails on unsupported or rerouted choices. A reroute cannot
silently alter the approved choice.

The patch must carry the source revision, exact file preimages, changed paths
and local verification result. The existing runtime journal remains the
source of authority. Applying the patch requires the approved scope and
unchanged preimages; the accepted result is bound to the resulting exact
revision and its verification. An assistant final message or successful
process exit is insufficient. The accepted-revision accounting in issue #5
can proceed independently of this native stop.

Ownership stays within existing boundaries: serialized contracts in
`router-types`, host protocol preparation in `adapter-codex`, process/broker/
executor supervision and journal integration in `router-runtime`, application
I/O and approval binding in `agent-router-cli`. `xtask` and hooks gain no
product authority. The existing Codex adapter prepares `codex exec`;
[execution.rs](../../crates/adapter-codex/src/execution.rs) is not an app-server
transport and must not be relabeled as one.

## Acceptance disposition and reopening

The NO-GO decision, surface, compatibility pins, authority preset and budget
scope are recorded. The equivalent behavioral comparison required by #131
is **incomplete**: permission refusal, unsupported model/effort, rerouting,
RPC IDs, malformed messages, buffer overflow, cancellation, late results and
ambiguous interruption were not rerun through both candidates after the early
physical-send stop. The [comparison matrix](../bench/owned-entry-2026-10-07/README.md)
marks every such cell explicitly. Historical upstream mock results do not
satisfy native parity. Do not describe all #131 acceptance boxes as passed.

The next independently startable frontier is a bounded broker/send diagnostic
under #133 (F03a): local scripted provider only, including a denied second send
and an internal retry, with no useful patch or native admission claim. F03a
does not depend on a positive #131 or on #132. Its evidence can reopen this
decision by establishing a concrete control point and a representable budget
unit. A replacement transport decision then permits #132's simulated useful
patch investigation. Final physical-send admission (F03b) still requires the
useful-path evidence and ADR 0022's controller/executor OS admission. This
separates feasibility from final qualification and avoids making #131 wait
for a final #133 result that itself depends on #132.

When reconsidering a route, run the same negative scenarios through both
actual candidate transports. Cancellation needs process and
descendant cleanup evidence; ambiguity retains reservations and blocks blind
retries. A replacement GO also needs native compatibility for the exact chosen
adapter/native pair. This decision does not close U07, admit #132's native
useful-patch path, or establish #133's physical-send budget.
