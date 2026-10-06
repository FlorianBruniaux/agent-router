# U02b: native prompt trigger observed, routing application unproven

Codex CLI 0.159.2 invoked the fixed `UserPromptSubmit` canary after trust through
the native `/hooks` UI. Two separately submitted identical prompts were blocked
with `U02B_FIXED_CANARY`, with zero provider connections or requests. A subsequent
app-server turn captured matching `hook/started` and `hook/completed` thread, turn
and run IDs, status `blocked`, and the exact feedback, again before provider contact.
See [u02b-ui-result.json](u02b-ui-result.json).

After disabling the hook through `/hooks`, a fresh native UI session submitted a
control prompt without a hook block. The rejecting loopback provider observed two
requests, both refused; no inference occurred. The repeated enabled prompts test
two intentional submissions, not transport replay suppression. Daily Agent Router
decision/application and replay deduplication remain unproven, so U02b as a whole
is not complete. No parent model was changed.

The earlier [discovery diagnostic](u02b-result.json) remains a separate observation:
`initialize`, `initialized` and `hooks/list` found the enabled but untrusted candidate,
with zero provider contacts and no model turn. It does not itself prove invocation.

## Direct hook output cannot select a model in this contract

The pinned executable embeds the exact input/output schemas exported in
[u02b-hook-schemas.json](u02b-hook-schemas.json). The input exposes the current
`model`, `session_id`, `turn_id`, prompt and execution context. The output has
`additionalProperties=false` at the top level and in `hookSpecificOutput`.
Its supported actions are continue/stop, block, messages and additional context.
There is no model or reasoning-effort output field. Direct model/effort application
through this hook output is **unsupported by this installed contract**.

That limit does not rule out separate-session delegation through another native
interface. It also does not establish that an instruction added as context will
be applied. No parent model was changed.

## Native trust boundary and next evidence

The inspected experimental app-server schema exposes `hooks/list`; it exposes no
hook trust RPC. Installed CLI help provides no hook-trust command. It does advertise
a bypass flag, which this diagnostic does not use. The repository's
[native trust procedure](../../harness.md#fresh-session-canaries) requires reviewing
the exact definition in `/hooks` and using native trust UI where required.

The inert candidate in [u02b-hook-candidate.toml](u02b-hook-candidate.toml) contains
one fixed `/usr/bin/printf` command returning a block decision. Native discovery
reported current hash
`sha256:d1b5e9e0bb462b5e8b1a0b10374658006edd46708d6ed50de39d2afff9ad03d7`.
The native UI diagnostic used an empty disposable workspace and isolated
`HOME`, `CODEX_HOME`, XDG and temporary directories, with no inherited credentials.
The exact candidate and configuration preimages were saved privately before launch.
In the native UI, review the disposable folder, open `/hooks`, select
`UserPromptSubmit`, inspect the fixed command, then press `t` to trust it. The
native-written `trusted_hash` was compared with the current hash above. Trust was
never synthesized by writing configuration state and no bypass flag was used.

The enabled UI was bounded to 180 seconds; the fresh disabled UI to 120 seconds;
each terminal log to 2 MiB; the final correlated app-server turn to 15 seconds.
The disabled session made two rejected requests despite zero configured retries;
this result makes no exactly-once provider claim. All native sessions exited and
provider threads stopped, process groups were absent, and the owned disposable
session directory was removed after a final configuration preimage comparison.
Private terminal/protocol evidence is retained outside the repository; its hashes,
sanitized IDs, bounds and cleanup observations are in the UI result. Re-discover
the hash before trusting if candidate bytes or installed version change.

Reproduce the delivered discovery from the repository root. `CODEX_NATIVE` is the
installed native `bin/codex` verified by the v5 digest:

```sh
rtk proxy python3 docs/bench/native-integration-u02/u02b_probe.py \
  --binary "$CODEX_NATIVE" \
  --output /private/tmp/u02b-result.json \
  --raw-dir /private/tmp/u02b-new-private-raw
```

The raw directory must be new and remains private. Discovery is bounded to ten
seconds of protocol time, the v5 HTTP/RPC byte limits and existing helper cleanup
limits. Provider request allowance is zero. If macOS denies the signal-zero
process-group check, this diagnostic falls back to a bounded numeric-only process
group inventory requiring exit zero, EOF and a complete nonempty numeric parse;
failed, truncated or malformed observations retain UNKNOWN. The test does not write machine
paths or raw hook input to the exported result.

## Smallest supported U06 path, not implemented here

A separate explicit app-server controller can consume an actual
[`agent-router route --json` decision](../../reference/json-contracts.md#selected-decision),
reject `no_route`, and bind the chosen stage's `model` and `effort` plus decision ID
to a new native thread. [V5](README-v5.md) already observes explicit
`turn/start` model/effort overrides in the outgoing request, their correlated
completion, and typed-invalid-effort refusal before provider contact.

The next U06 fixture needs the real CLI decision as input instead of v5 constants,
with a recorded stage selection, one distinct native thread/turn, one local
verification, attempt accounting and fresh-session withdrawal. It must fail on an
unsupported model/effort, lost correlation or duplicate effective application.
This path can be tested with the existing fake provider. It needs no daemon and
makes no assumption about changing the parent session. Native agent-tool model
selection was not qualified here. U06 and U07 remain open; an external native
session must not be given a fabricated AR runtime receipt.
