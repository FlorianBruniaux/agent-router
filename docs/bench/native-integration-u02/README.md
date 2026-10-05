# U02: native Codex API evidence, automatic CLI integration not validated

Observed on 2026-10-05 on macOS 26.5.1 arm64, Codex CLI 0.159.2,
binary SHA-256 `61b0194f3bb6534439c8d26a3ed57d0805f84b884588b761795323eeb92fcf70`.
The candidate surface is the CLI binary's experimental app-server stdio API.
This is a standalone diagnostic client, not an installed Agent Router integration.

The native controller transmitted the requested model and effort to a scripted
loopback provider, emitted correlated start/output/completion events, and a fresh
session without overrides transmitted the original defaults. No actual model
inference, upstream availability, provider pricing or productivity was measured.
Automatic CLI prompt interception remains unvalidated. **U06/U07 remain blocked.**

## Capability verdicts

| Capability | Verdict and exact scope | Evidence |
| --- | --- | --- |
| Explicit native turn submission | VERIFIED for a client-issued `turn/start` | `turn/started` in both cases of [result-v3.json](result-v3.json) |
| Task/session/output correlation | VERIFIED in the client transcript | Fixture UUID sent as `clientUserMessageId`; response thread/turn IDs match native start, assistant item and completion. UUID echo from host was not asserted. |
| Model/effort selection | VERIFIED as controller-to-fake-provider parameters | Enabled session starts with `gpt-6.1-sol` / `low`, turn override sends `gpt-6-astra` / `high`; exactly one accepted provider request. No claim that either model executed. |
| Native output | VERIFIED synthetic output | `item/completed` contains exactly `U02_SCRIPTED_OUTPUT`, correct thread/turn IDs; turn completed. |
| Disable per-turn overrides in a fresh session | VERIFIED for this disposable API client | Enabled process stopped; fresh isolated home, app-server and thread omit overrides, then provider sees `gpt-6.1-sol` / `low`; process exits 0, temporary state removed. |
| Dynamic-tool canary registration | NOT_OBSERVED in this scenario | Correctly typed registration requested, provider tool inventory empty. Cause UNKNOWN. Composite probe exits 1, preserving this negative result. |
| Dynamic-tool removal | UNKNOWN | Absent in disabled session, but positive registration prerequisite failed. |
| Malformed effort type rejection | VERIFIED | Object-valued effort gets JSON-RPC error `-32600` before any provider request, both sessions. |
| Unknown effort string rejection | NOT_SUPPORTED by the tested protocol path | Initial run forwarded `invalid-u02-effort`; schema defines any nonempty string. Host model-advertised effort eligibility is not enforced by this test. |
| Automatic CLI prompt hook | NOT_TESTED / integration not admitted | `turn/started` reports an already-started client turn; it is not a demonstrated pre-submission routing hook. |
| Installed hook disable/restore | NOT_TESTED | No hook/configuration was installed. Fresh-client override removal does not establish global rollback. |
| Codex App / Claude surfaces | NOT_TESTED | No inference from the CLI app-server result. |
| OS sandbox / external egress denial | NOT_APPLIED / UNKNOWN | Explicit execution-tool escalation permitted loopback listener. No OS network sandbox or packet capture. |
| AR supervised execution | NOT_TESTED | No runtime receipt, approved execution or worker sandbox fabricated. |

The current AR adapter separately rejects `ExecutionTarget::ParentConversation`
with `AdapterError::Unsupported("parent-model-change")` in
[execution.rs](../../../crates/router-adapter-api/src/execution.rs#L651).
That is a product contract read from source, not a measured upstream limitation
or an observed rejection in the Codex desktop App. The native app-server's
ability to select parameters for an owned thread does not override that contract.

## Reproduce and inspect

Run from the repository root, with Python 3 and the installed Codex executable.
The probe needs explicit permission to bind loopback and spawn the native binary
when a surrounding tool sandbox prevents those operations.

```sh
python3 -B docs/bench/native-integration-u02/probe.py \
  --timeout 30 --output /private/tmp/ar-u02-result.json
python3 -B -m unittest discover \
  -s docs/bench/native-integration-u02 -p test_probe.py -v
```

On the recorded host the native probe exits **1** because dynamic registration
is absent; consult individual capability rows instead of interpreting a blanket
success. The eight offline checks pass. They exercise correlation, wrong model
and effort, duplicate physical requests, cleanup and refusal evidence, absence
of the canary, and sanitized nested tool-name projection. They do not launch
Codex or independently establish native behavior.

The fixture imports bounded process/provider helpers from
[the existing callback fixture](../egress-admission/codex_appserver_local.py).
Each case uses an allowlisted environment, fresh owner-only HOME/CODEX_HOME and
workspace, a synthetic key and loopback endpoint, an ephemeral thread, a deadline
of 5 to 60 seconds, at most one accepted Responses POST, bounded request/output
sizes, bounded concurrent connections and process-group cleanup. It executes
no tools and logs no prompts, credentials or personal configuration. The listener
and output readers stop before scratch removal. Crash recovery and malicious
controller resistance are not proved. The scripted SSE helper retains its legacy
`gpt-5-codex` response label; observed requested selections come from captured
outbound request parameters, not that synthetic response metadata.

No global settings, source outside this directory, commit, push, remote provider
or production state was changed by this diagnostic. Provider spend is USD 0.

## Failed probes remain evidence

[initial-probe.py](initial-probe.py) and [initial-result.json](initial-result.json)
retain the first mistaken assumption that an arbitrary effort string would be
rejected. It was forwarded. The fixture then submitted another turn while the
first was active, hit its one-request limit, and recorded failed completion.
Those failures do not establish invalid model/effort overrides. Do not rerun the
historical script as the current control; it preserves that fixture mistake.

[probe-v2.py](probe-v2.py) and [result-v2.json](result-v2.json) use an object-valued
negative control and stop if it is unexpectedly accepted. Both native turns
completed and the requested values were transmitted, but the tool registration
omitted required `type`. The current [probe.py](probe.py) adds `type: function`;
[result-v3.json](result-v3.json) still has no advertised canary. Do not attribute
that absence solely to the v2 fixture omission. Investigating tool availability
for these exact model IDs is separate from the confirmed parameter transmission.

[local-schema-evidence.json](local-schema-evidence.json) records selected schema
fields and full-file hashes generated by the same installed binary using
`codex app-server generate-json-schema --experimental --out <temporary-directory>`
in an isolated HOME/CODEX_HOME. In particular, `effort` is a nonempty string and
model/effort overrides apply to subsequent turns too. This diagnostic tests fresh
session disable only; removing overrides from an already-modified thread was not
tested and must not be assumed to restore defaults.

The existing gpt-5-codex callback probe remains separate historical evidence;
its successful tool round trip does not turn the absent canary here into success.
