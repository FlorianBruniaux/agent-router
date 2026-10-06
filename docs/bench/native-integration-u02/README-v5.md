# U02a v5: native code-mode roundtrip observed

On Codex CLI 0.159.2, the pinned native executable accepted one scripted
`functions.exec` custom tool call, requested the registered dynamic canary through
`item/tool/call`, and included its exact fixed output in the next provider request.
The unexpected-nonce control was refused before any successful callback response.
A third, fresh process and home had no canary declaration or callback and used the
baseline model/effort. All three owned process groups disappeared and scratch was
removed. See [result-v5.json](result-v5.json).

This proves U02a for the installed CLI app-server stdio surface and a fake loopback
provider. It does not prove remote inference, model competence, daily automatic
interception, Codex App behavior, Agent Router decision application or an AR runtime
receipt. External egress remains UNKNOWN without packet capture or an OS denial
proof. No provider credentials, global configuration or installation were used.

## Actual contract

The outgoing request has an `additional_tools` item at `/input/0`. Its nested
`functions.exec` custom tool declares the dynamic canary in its description.
The top-level `tools` array is absent. Historical inventory checks therefore do
not establish absence of the canary. No `features.code_mode` override was needed
for this observed model path; v5 is not an enabled-versus-disabled feature study.

The scripted SSE emits one `custom_tool_call` with namespace `functions`, name
`exec`, outer call ID `call_u02_exec`, and fixed source:

```javascript
text(await tools.u02_registration_canary({nonce: "U02_CODE_MODE_NONCE"}));
```

The host generates a separate `exec-<uuid>` dynamic call ID. The probe checks the
callback thread/turn, tool, namespace and exact arguments before replying once.
It then requires a `dynamicToolCall` completion with that native call ID, exact
arguments/content, `status=completed` and `success=true`. The next provider request
must contain exactly one matching outer `custom_tool_call_output` with an exact
`input_text` content item `U02_CODE_MODE_CALLBACK_ACCEPTED`. Substrings in an error
or a different content type do not satisfy the test. The final native message and
turn completion must match the same thread/turn.

A separate native negative case changes only the nonce in the scripted source.
The callback receives a protocol error; no canary result is sent and the owned
process is stopped. The negative verdict also requires the expected wrong nonce
with the correct thread, turn, tool, namespace and callback identifier shape.
An unrelated refusal cannot satisfy that control. Offline tests additionally refuse a changed thread, turn,
tool, namespace, empty call ID, invalid request ID and a duplicate callback.

## Reproduce

Set `CODEX_NATIVE` to the installed native `bin/codex`, not the npm JavaScript
launcher. The probe rejects a different native digest/version before running.
The code-mode-host digest and unchanged helper digest are recorded in the result.
Run from the repository root:

```sh
rtk proxy python3 -m unittest discover -s docs/bench/native-integration-u02 -p 'test*.py' -v
rtk proxy python3 docs/bench/native-integration-u02/probe_v5.py \
  --binary "$CODEX_NATIVE" \
  --output /private/tmp/u02-v5-result.json \
  --raw-dir /private/tmp/u02-v5-new-private-raw \
  --timeout 20
```

The raw directory must not already exist. Raw requests and RPC messages remain
under `/private/tmp`, with directory mode 0700 and file creation mode 0600. Only
inventory pointers, exact-match booleans, native correlation IDs, model/effort,
counts and digests are exported. Copy a checked result into a new versioned
artifact; do not overwrite historical results.

Limits: 20 seconds per native case in this run, 1 MiB per HTTP request body, 256 KiB per
RPC line, 2 MiB stdout and 2 MiB stderr, eight accepted connections maximum per
case, and two/one/one HTTP requests allowed for enabled/refused/disabled. HTTP
attempts and TCP contacts are counted separately, including malformed requests.
Provider retries are disabled in the isolated config. Unexpected traffic cannot
produce a passing verdict. Cleanup has a bounded process-group reap check and
bounded listener/handler joins, inherited from the existing helper.

## Validation and history

TDD: four initial assertions failed before implementing the callback/projection/
SSE behavior; two verdict assertions failed before their implementation. Follow-up
RED cases caught substring output acceptance, missing native-item checks and a
missing process-group check. The corrected v5 suite passed 32 tests, including 19 new
v5 checks and 13 historical checks; the separate U02b projection adds one test. The final native run passed all three cases.
An intermediate native run correctly failed an immediate process-group check;
waiting up to the existing three-second cleanup limit resolved the macOS reap
race without weakening the absence criterion.

The inspected schema and binary provenance are in
[protocol-v5.json](protocol-v5.json). All earlier probes and results are retained.
The helper under `egress-admission` was reused without modification.

Independent review found that the first v5 negative verdict accepted any refused
callback. [result-v5-before-review.json](result-v5-before-review.json) preserves
that pre-review run byte-for-byte; its stored positive flag is historical and
does not qualify the negative-control criterion. Two RED regressions reproduced
the weakness before adding the exact correlated wrong-nonce requirement. The
current result must come from the corrected script hash, including a fresh native
run; an unverified cleanup result cannot pass.

The [first independent rerun after review](result-v5-first-review-cleanup-unknown.json)
passed the protocol checks but failed the enabled case's process-group absence
check. That failure is retained without changing its verdict. The observer now
records whether signal-zero found the group, found it absent, or lacked permission.
Only a permission error permits a second numeric-only `ps` observation. That
observation requires exit code zero, EOF, a nonempty complete numeric listing,
and absence of the owned PGID. Its reader stops at 256 KiB and the remaining
three-second cleanup deadline. Empty, malformed, truncated, oversized, failed or
expired observations remain UNKNOWN and fail qualification. A group positively
observed by signal-zero is never treated as absent by this fallback. Private
PGIDs remain in the raw directory; exports contain observation reasons only.
