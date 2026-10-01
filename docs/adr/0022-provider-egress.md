---
adr: "0022"
title: Provider egress requires an external tool boundary
status: accepted
created: 2026-10-01
last_modified: 2026-10-01
scope: "Admission contract and independent ADR 0021 infrastructure work. Native provider egress is not implemented or admitted. CLI interface discovery is structural evidence only."
amended_by: []
supersedes: []
superseded_by: []
related: ["0014", "0021"]
---

# ADR 0022: Provider egress requires an external tool boundary

## Decision

Provider access and tool networking require separate execution boundaries.
A provider endpoint allowlist on the current native process does not establish
that separation. Native execution remains subject to ADR 0021's batch 4 stop
until the selected host, binary and OS pass the admission checks below.

Keep the native host's model and tool-selection loop. Route executable tools
through a runtime-owned executor under its own OS sandbox. Do not replace
Codex or Claude Code with a new provider-API agent loop. The next investigation
uses a real pinned CLI against a local scripted provider, not a new fake-host
architecture that has no demonstrated CLI integration.

ADR 0021 batches 5 and 6 may ship independently with synthetic tests. Batch 9
may ship after its proof and validation dependencies, with synthetic tests and
no native activation. None of these batches opens provider egress, runs a paid
canary, or promotes a real host capability from UNKNOWN.

## Current boundaries

At `b47250ddb795cbf621beed700109a90e77aa1059`, `SandboxPolicy` has one network
boolean and requires enforcement on every descendant
(`crates/router-runtime/src/sandbox.rs:12-17`). The runtime derives it from
`Capability::Network` (`crates/router-runtime/src/lib.rs:1533-1537`). Seatbelt
adds `(allow network*)`; bubblewrap adds `--share-net`
(`sandbox.rs:479-480`, `bubblewrap.rs:272-280`). Neither rule distinguishes an
inference request from a tool request made by the same process or child tree.

ADR 0014 supplies an outgoing client for a signed request. Its provider adapter
rejects tools and workspace writes
(`integrations/bridge-contract/src/provider.rs:114-120`); its transport returns
evidence rather than provider text (`transport.rs:1-7`). It is not an incoming
proxy for native CLI traffic. Its loopback endpoint does not authenticate the
listener, and its transport has no TLS or credential broker. Reuse its bounds
and validation where applicable without widening its existing contract.

The current startup path exposes approved API credentials to the native child:
Codex receives a private `auth.json`, and configured environment values reach
the process (`host_state.rs:60-69`, `sandbox.rs:351-352`). An executor that can
read this state or inherits that environment has not isolated the credential.

## CLI interfaces checked on 2026-10-01

Interface discovery used help, version and schema generation commands.
The later local-only investigation is recorded below. No paid model request,
login, personal credential copy, or real-provider canary was performed.

### Claude Code

Local `claude --version` reported `2.1.286`. Its help lists `--tools`,
`--mcp-config`, `--strict-mcp-config`, `--restricted` and `--bare`. The
[official CLI reference](https://code.claude.com/docs/en/cli-reference) states
that `--tools ""` removes built-in tools while leaving MCP tools available,
and documents a residual `EndConversation` tool when MCP tools remain. This
provides a concrete candidate: remove executable built-ins and expose only a
runtime-owned MCP tool surface. `--strict-mcp-config` selects the supplied MCP
configuration. `--restricted` alone is insufficient: its documented exclusions
do not remove all built-ins. Permission prompt delegation decides approval;
it does not relocate tool execution.

The [MCP documentation](https://code.claude.com/docs/en/mcp) describes local
stdio servers as child processes. Therefore a supplied MCP command must be a
pinned relay or sandbox launcher, never an unsandboxed executor inheriting
provider access. Complete tool inventory, managed-setting interactions, relay
startup, credential isolation and OS enforcement remain unmeasured for this
candidate. This is a viable interface to test, not a capability proof.

### Codex

Local `codex --version` reported `0.159.2`. Running
`codex app-server generate-json-schema --experimental --out <scratch-dir>`
generated `v2/ThreadStartParams.json` with `dynamicTools`, plus
`DynamicToolCallParams.json` and `DynamicToolCallResponse.json`. The
[app-server documentation](https://developers.openai.com/codex/app-server/)
describes the experimental `item/tool/call` request handled by the embedding
client. This is a concrete external-execution interface. It adds tools; it
does not by itself prove that built-in execution paths have been removed.

Local app-server help also exposes `--code-mode-host`. Moving that host is
not evidence that every nested tool callback executes there. Before selecting
it, trace the callbacks and demonstrate their execution boundary for the
approved binary. The generated thread schema also contains `environments`;
its empty-list description disables environment access, not a documented
complete tool inventory. Neither option is accepted as a shortcut.

The [configuration reference](https://developers.openai.com/codex/config-reference/)
documents `features.shell_tool`, `features.unified_exec` and `web_search`,
but a list of individual switches is not a complete removal proof. It also
states that the sandboxed-command network proxy does not filter apps, MCP or
hosted tools. The repository's current `codex exec` adapter only accepts
`ToolSelection::HostPolicy` (`crates/adapter-codex/src/execution.rs:24-26`).
App-server integration needs its own adapter contract and event handling.
Complete built-in suppression remains UNKNOWN; this review does not conclude
that the upstream CLI cannot support it.

## Required process and authority separation

The runtime supervises three roles:

1. A provider broker owns the real API credential and the approved provider
   destination. It exposes only the provider operations required by the pinned
   host, with method, path, model, request-size, response-size, time and
   cumulative-use bounds. It refuses general CONNECT forwarding, caller URLs,
   redirects and ambient upstream proxies. TLS and destination validation are
   required before remote use; a local scripted provider needs neither a real
   credential nor paid access.
2. The pinned native controller retains its native inference loop. Its OS
   profile permits only the selected broker connection and required control
   IPC, with no direct provider connection. It receives a run-scoped broker
   credential if its HTTP client requires one, never the real provider key.
   Executable built-ins, unrelated MCP servers, hooks, plugins, subagents and
   background execution paths must be removed or explicitly accounted for.
3. The runtime's executor handles typed tool requests under a separate OS
   profile. Its `Network` grant comes only from the phase, never from provider
   access. It receives neither controller state nor broker authentication,
   connection descriptors or endpoint access. The supervisor launches workers
   outside the controller's inherited provider permissions. A Claude stdio
   relay may forward bounded messages to the supervisor, but does not execute
   arbitrary tool arguments itself.

The controller and relay are trusted dispatch code; tool arguments and tool
processes are untrusted. An internal tool running in the controller shares its
authority and defeats this boundary. A PID check, token, client certificate or
endpoint allowlist cannot distinguish two functions in that process. Tool
exclusion must therefore be established before enabling controller egress.

The broker authorizes inference, including the prompt and tool results sent
to the model. This decision does not promise to prevent data leaving through
those approved messages. Another unsandboxed process of the same user remains
outside the protection offered by ADR 0021's per-user store and this design.

## OS admission is separate

On macOS, the controller profile must grant the broker endpoint without
granting arbitrary sockets. Workers keep a separate profile that denies that
endpoint when tool networking is absent. The current broad `mach-lookup`
permission means socket denial alone does not prove absence of DNS or system
service channels (`sandbox.rs:455-459`). Record those observations separately;
do not label this a complete ban on all information channels.

On Linux, retaining `--unshare-all` means a host loopback listener is not an
assumed transport into the controller namespace. An explicitly supervised
namespace-local relay or private IPC transport must be proved. Using
`--share-net` to reach the broker is refused as an admission shortcut. Tool
workers get their own mount, PID and network boundaries, with broker sockets
and controller state absent. Descriptor closure and descendant behavior need
tests on each OS. Passing macOS checks does not admit Linux.

## Admission checks and next implementation boundary

Start with Claude's documented built-in removal plus MCP interface. In
parallel or afterwards, investigate Codex's native app-server interface.
Neither host depends on the other becoming admissible. Before building a
production broker, use each actual pinned CLI with a local scripted provider
that returns deterministic tool requests and final messages:

- Capture the complete tools sent to the provider. Require the approved
  external set and explicitly classified non-executing controls only. Return
  a forbidden built-in call from the fake provider and verify no effect and
  no fallback execution. Repeat for subagent and background paths if present.
- Exercise a permitted write through the external tool interface. Check exact
  nonce bytes, scope and provenance. A final assistant message without the
  requested write does not pass. Check the next provider request contains the
  external result, proving the native loop resumed.
- From that executor and a grandchild, attempt connections to the broker,
  fake provider and an unrelated listener. Under absent tool `Network`, all
  fail and the listeners observe zero accepted requests. Verify that reads of
  controller auth, broker secrets and approval keys fail. Test inherited
  descriptors, environment leakage and direct IP/proxy bypasses. With explicit
  tool `Network`, verify the approved tool target works while provider-broker
  requests from tools remain refused.
- Keep a positive inference control while tools cannot reach the broker.
  Test malformed tool requests, response bounds, cancellation and process
  death. Unknown isolation, failed cleanup or an unclassified tool blocks
  admission; no automatic fallback to a network-open native run is allowed.

The acceptance record binds the CLI and helper hashes, version, adapter
template, tool inventory, effective configuration, broker policy, OS profile,
model protocol and test outputs. Changes invalidate it. Only after this
record exists may the runtime add the corresponding native mode and preview
a new approval digest. Real provider connectivity and paid canaries still
require the task's explicit credential and spending authorization. A local
fake-provider success does not create a real-host capability proof under
ADR 0021.

Implementation ownership remains: `router-adapter-api` for typed host
contracts and evidence, host adapters for protocol preparation,
`router-runtime` for broker/executor lifetimes and sandbox enforcement, and
`agent-router-cli` for configuration and approval binding. Do not add provider
authority to `xtask` or the development harness. Introducing a new model loop,
privileged daemon or OS identity requires a separate scope decision.

## Independent ADR 0021 batches

Batch 5 closes the binary replacement window using synthetic executables.
Batch 6 authenticates proof records and closes handwritten-snapshot claims
across shared validation. Its synthetic proof issuer stays in test fixtures;
it cannot mint evidence for installed Codex or Claude. Batch 9 retains a
separate attestation tier and explicit policy acceptance, but an attestation
cannot satisfy provider-egress admission or override sandbox refusal.

These changes improve the existing runtime without changing the outcome of
batch 4. Batches 7 and 8, real capability promotion and native provider egress
remain blocked per host until their own conditions are met. The repository
must report infrastructure implementation and native activation separately.

## Local Claude investigation, 2026-10-01

The [reproducible fixture and measured results](../bench/egress-admission/README.md)
used Claude 2.1.286, a fresh home and state, a synthetic API key and a scripted
provider bound to loopback. The controller's Seatbelt profile allowed only
that loopback port. A socket control reached it and received EPERM on a second
local port. No general network grant or personal authentication was used.

Claude initially failed on its default temporary directory. Pointing
`CLAUDE_CODE_TMPDIR` at private state fixed that reported failure, but the
native process then timed out before any provider request. The bounded
45-second case still produced no request. Removing only `--restricted` did
not resolve startup. Debug logs and a bounded native stack sample do not
establish the remaining cause. Tool inventory, forced built-in refusal and
the native MCP round trip remain UNKNOWN.

A direct MCP protocol control under its own network-denying profile completed
initialize, list and call, wrote the exact nonce, and received EPERM when
connecting to either local listener or reading controller-private state. It
had no provider credential in its environment. The same worker launched via
a second `sandbox-exec` inside the controller profile failed before executing
with `sandbox_apply: Operation not permitted`. Thus this profile does not
support the tested nested-launcher shortcut. A supervisor-launched sibling
and its relay still need behavioral verification.

These observations do not satisfy admission. The next investigation must
resolve the controller startup failure without expanding network authority,
then measure the actual CLI's tool dispatch. No production broker or new
model loop is justified by the standalone fixture result.
