# U02 dynamic-tool diagnostic: unresolved

Two diagnostic variants used Codex CLI 0.159.2 and one scripted loopback provider request each. No real model inference, actual tool callback, upstream availability or automatic prompt interception was tested. Both native runs produced correlated synthetic output and clean process/scratch cleanup.

## Observed facts

1. Selecting only `gpt-5-codex` instead of `gpt-6-astra`, while retaining the initial thread model `gpt-6.1-sol`, advertised `u02_registration_canary` among eleven function names. The original fixture verdict passed. This establishes sensitivity to the selected model in this fixture, not an upstream model capability comparison.
2. The installed native executable embeds both gpt-6 model IDs, each with `tool_mode: code_mode_only`. Missing live model cache is therefore not a sufficient explanation that these model IDs are unknown. The isolated CLI lists `code_mode=false` and `code_mode_host=true`. Model catalogue metadata does not by itself prove which internal path executed.
3. Restoring the original gpt-6 IDs and adding only isolated `features.code_mode=true` still produced an empty `tools` inventory. New observation-only booleans found the canary in the outgoing request, but not in `tools` or `instructions`. The field containing it was not recorded. The original direct-inventory verdict still failed.

The baseline did not measure canary membership across the whole request. Therefore the second variant does not establish that enabling code mode caused the canary to appear. It does establish that an empty `tools` array is insufficient to claim the canary was wholly absent from this request. Do not mark U02 resolved or infer a working callback from the canary string.

## Binary provenance correction

The historical field `codex_binary_sha256=61b0194f3bb6534439c8d26a3ed57d0805f84b884588b761795323eeb92fcf70` hashes the npm JavaScript launcher, not the native executable. Native executable SHA-256 is `16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.

Variant 1 recorded this native digest from a separate read-only measurement; its wrapper contained a literal native digest and did not assert it. Variant 2 asserted both executable and launcher hashes before execution. Preserve that evidence distinction. Original probe and helper hashes are retained in each result.

## Next fixture change to validate, not executed

First inspect the installed code-mode protocol or its generated schema to identify the exact scripted response event and callback contract. Extend the local provider projection with sanitized JSON pointer locations of the canary, plus counts, without retaining instruction text or credentials. Run a controlled original-config versus `code_mode=true` comparison with identical observations. Then have the scripted provider request exactly one fixed code-mode call to the registered canary, verify correlated host `item/tool/call`, return a fixed synthetic result, and require the next outbound provider request to contain its matching output. Preserve strict request, byte, time and process limits and reject any unexpected tool call. Verify canary removal using a fresh disabled session. No claim of real model competence, automatic interception, AR execution authority or production usability follows from this fixture.

These are diagnostic projections, not an admitted execution contract. The temporary wrappers are identified by their hashes; the documented changes are relative to the pinned probe and helper, not new executable integration code. The bundled catalogue export omits unrelated model names. No global configuration was changed.
