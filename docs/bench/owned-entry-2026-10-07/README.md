# Owned entry: physical-send boundary stops activation

On 2026-10-07, source inspection and local schema discovery support a NO-GO
for unmodified direct Codex app-server and codex-acp under the required
per-physical-provider-send admission contract. [ADR 0023](../../adr/0023-owned-task-entry.md)
records the decision and its reopening conditions. No agent task or provider
request was executed; no global configuration was changed.

## Evidence collected in this investigation

[decision.json](decision.json) records source file hashes and upstream links,
compatibility limits, early-stop status and the unexecuted comparison cases.
[native-schema.json](native-schema.json) records the fresh native discovery.
The AR source reference is `e822bc7b3984d3f9e25f5c02b570405f75ca41fc`.

| Check | Result | Scope |
| --- | --- | --- |
| Native executable SHA-256 and version | PASS | Exact installed native binary, Codex 0.159.2 |
| `app-server --help` | Exit 0 | Interface discovery only |
| Experimental JSON-schema generation | Exit 0 | Fresh disposable state, no native turn |
| Server-request method inventory | 11 methods, enumerated in JSON | No provider-send admission callback in this generated enum |
| Thread/turn parameter fields | Present | Model, effort, approvals and sandbox schema fields; no runtime propagation claim |
| Native retry/source inspection | Internal transport retry and authentication-recovery paths found | Pinned source, no physical request executed |
| ACP lock inspection | Codex 0.159.1, ACP SDK 1.5.0 | Does not qualify ACP with Codex 0.159.2 |
| Per-send budget enforcement | NOT_ESTABLISHED | Admission has no inspected pre-send callback and no admitted broker |

The prior #131 audit recorded 54 upstream codex-acp mock tests. Those tests
were not rerun here. Their old temporary clones were unavailable when checked;
only the source files needed for this decision were downloaded again at the
recorded refs. The earlier mock results remain historical source-audit evidence.

## Equivalent behavioral fixtures

The early stop avoids building a client transport solely to make new client
fixtures pass. This matrix defines the comparison required if the route is
reopened. Every current result is `NOT_RUN_EARLY_STOP`; it does not claim fixture
parity or native compatibility.

| Scenario | Required invariant for both candidates | Direct | ACP |
| --- | --- | --- | --- |
| Permission refusal | Denied or absent authority produces no tool effect or authority expansion | NOT_RUN | NOT_RUN |
| Unsupported model/effort | No silent default or provider submission with an unapproved selection | NOT_RUN | NOT_RUN |
| Rerouting | Structured requested/effective identity; refuse changed selection | NOT_RUN | NOT_RUN |
| RPC IDs | Wrong or duplicate IDs cannot resolve another call; intervening events preserved | NOT_RUN | NOT_RUN |
| Malformed messages | Typed failure without accepting malformed success or losing accounting state | NOT_RUN | NOT_RUN |
| Buffer overflow | Bounded memory; durable loss/failure state; no complete-evidence claim | NOT_RUN | NOT_RUN |
| Cancellation | Logical cancellation separated from observed process/descendant cleanup | NOT_RUN | NOT_RUN |
| Late result | Cannot finalize a canceled or replaced attempt or apply its patch | NOT_RUN | NOT_RUN |
| Ambiguous interruption | UNKNOWN retained, no refund or blind replay until reconciliation | NOT_RUN | NOT_RUN |

Source inspection identifies extra ACP work before such a comparison can pass:

- `CodexAcpServer.ts:2822-2878` resolves the turn as interrupted in `finally`,
  including when the interrupt request fails. A cancel acknowledgment cannot
  be used as a physical cleanup receipt.
- `PendingNotificationBuffer.ts:54-64` drops notifications above its bound.
  A durable loss indicator is required for complete AR accounting.
- `AgentMode.ts:68-95` selects the automatic reviewer by default. AR must
  select its explicit approval preset and reject unauthorized changes.
- `CodexAcpClient.ts:1088-1115` forwards model, effort and sandbox fields to
  `runTurn`; native acceptance and effective values still need verification.

Exact source URLs and byte hashes are in [decision.json](decision.json).

## Reproduce native schema discovery

Provide the absolute path of the native executable, not the npm launcher.
The script refuses any executable whose SHA-256 differs from the recorded
pin before executing it. It calls only version, help and schema generation,
with fresh HOME/CODEX_HOME and no inherited authentication environment.

```sh
python3 docs/bench/owned-entry-2026-10-07/inspect_native.py \
  --binary /absolute/path/to/native/codex > native-schema-current.json
cmp docs/bench/owned-entry-2026-10-07/native-schema.json native-schema-current.json
```

A changed binary or schema requires a new investigation; do not edit the pin
to manufacture a passing comparison. Schema discovery does not attest
sandbox enforcement, native agent execution, provider compatibility, physical
send admission, cleanup of a running agent or an accepted patch. The script
uses temporary local state and removes that state on normal completion.
No installed integration or hook is created.

## Issue acceptance

The decision investigation reaches an explicit NO-GO with reproducible source
and schema evidence. Full #131 behavioral acceptance remains incomplete.
Downstream useful-patch/native qualification and per-send budget admission
remain separate. A future dedicated AR entry should own submission before any
native task turn, retain the supervised runtime journal and produce a patch
bound to verified file preimages and the accepted revision.
