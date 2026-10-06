# U07 native hook failure diagnostic

[Observed result, 6 October 2026](observations.md): all five tested unexpected
failures let the parent contact the provider. Automatic child dispatch through
this hook is not admitted.

[Local verification](../verification-hook-failure-2026-10-06.json): 1,594 Rust
result lines and 275 Python tests pass; all 21 selected quality checks pass.

This diagnostic measures the pinned native Codex host's `UserPromptSubmit`
behavior before attempting automatic routing. It never starts a router child,
changes the parent model, generates inference, or writes native hook trust.
All parent provider requests reach a bounded loopback listener that returns a
fixed HTTP 503 refusal. Reports are external native observations, never Agent
Router execution receipts. U07 completion, quality and cost remain unproved.

The fixed hook command dispatches only these exact canaries:

| Case | Prompt | Hook behavior |
|---|---|---|
| block | `U07:BLOCK` | Valid JSON block control |
| exit1 | `U07:EXIT1` | Exit 1, fixed stderr marker |
| malformed | `U07:MALFORMED` | Invalid JSON, exit 0 |
| timeout | `U07:TIMEOUT` | Sleep 3 seconds against a 1-second native timeout |
| signal | `U07:SIGNAL` | Send SIGTERM to itself |
| exit2_reason | `U07:EXIT2_REASON` | Exit 2 with a blocking stderr reason |
| exit2_empty | `U07:EXIT2_EMPTY` | Exit 2 without stderr |
| disabled | `U07:DISABLED` | Hook disabled through the native UI |

Exit 2 with a reason is an explicit blocking-convention control. It does not
prove safe handling of an unexpected failure. The timeout case records a late
return if the host waits for its eventual JSON block. Its automatic
`safe_failure_refusal` remains false even when blocking is observed; native
status evidence must be reviewed separately before claiming timeout refusal.

## Prepare and real UI trust

Use Python 3.11 or newer. These commands assume the three files have been
integrated in the repository. Keep the source and existing U02/U06 helpers
unchanged throughout the experiment.

```sh
PY=/absolute/path/to/python3.11-or-newer
REPO="$PWD"
NATIVE=/absolute/path/to/pinned/native/codex
STATE=/private/tmp/ar-u07-native-unique-session
PROBE="$REPO/docs/bench/native-integration-u07/failure_probe.py"
"$PY" -B "$PROBE" prepare --repo-root "$REPO" --binary "$NATIVE" --state "$STATE"
```

`STATE` must not exist. The program creates private HOME, CODEX_HOME, XDG and
workspace directories, freezes its hook script, saves the initial configuration
and hashes, starts the rejecting provider, and prints the location of
`ui-launch.json`. The binary must hash to
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`
(Codex 0.159.2 in the previous verified experiment). It is not executed by
`prepare`. No user environment, profile, real key or proxy is inherited.

While `prepare` is running, launch exactly the `argv`, replacement `env` and
`cwd` from `ui-launch.json` in a fresh PTY. The parent operator owns this PTY;
limit it to 180 seconds and 2 MiB of private capture. Complete the workspace
trust prompt, open `/hooks`, inspect the exact fixed UserPromptSubmit command
and current hash, then use the native trust action. Exit the UI without sending
a conversational prompt. Keep the PTY transcript private. Do not edit
`trusted_hash`, `enabled`, or the generated hook definition manually.

After the UI has exited, interrupt `prepare` with SIGINT. It closes the listener,
interrupts and joins its bounded workers, and writes `prepare-cleanup.json`.
It also exits automatically after 180 seconds. Require `provider_stopped: true`
and inspect its TCP/HTTP counters before continuing. The UI itself is controlled
by the parent operator; the diagnostic does not claim to supervise that PTY.

## Fresh sessions and withdrawal control

Run each enabled case once. Every invocation creates a new native app-server
session, verifies config/source/binary hashes and native `hooks/list` trust
before sending the fixed prompt, then closes the session and listener.

```sh
for CASE in block exit1 malformed timeout signal exit2_reason exit2_empty; do
  "$PY" -B "$PROBE" run --repo-root "$REPO" --binary "$NATIVE" \
    --state "$STATE" --case "$CASE"
done
```

A case directory cannot be overwritten. Inspect `case-*/report.json` after each
run. Exit status 2 means UNKNOWN, not successful refusal. Exit status 0 means
an observation was classified; `PARENT_FALLTHROUGH_OBSERVED` is an unsafe result
for any attempted safe-failure claim, despite that zero-inference provider's
refusal. Stop progressing to routing implementation when such a result occurs.

For the disabled control, reopen the exact native UI using `ui-launch.json`,
disable the same hook through `/hooks`, and exit without a conversational prompt.
The configured loopback endpoint is closed between runs while the UI is reopened. Do not launch a prompt during this UI-only step. Then run:

```sh
"$PY" -B "$PROBE" run --repo-root "$REPO" --binary "$NATIVE" \
  --state "$STATE" --case disabled
```

The disabled control requires native `enabled=false`, no hook notifications or
journal entries, a terminal parent turn, and at least one rejected parent HTTP
request. Native retries may yield two requests. An occupied provider port makes
the next run refuse before native launch.

## Evidence, bounds and cleanup

`BLOCK_OBSERVED` requires correlated thread, turn and hook identities, exactly
one synchronous hook start/completion, matching private journal entries, a
terminal parent turn, zero accepted and rejected TCP connections, zero HTTP,
complete EOF, exit 0, absent owned native and hook process groups/PIDs, unchanged
pinned bytes, and closed provider workers. Zero HTTP alone is UNKNOWN. A failed
or incomplete case retains its counters but cannot claim safe refusal.

Private evidence contains raw protocol messages, hook PID/group identities and
native paths; keep the entire state directory private. The public report hashes
the hook run ID and omits raw configuration, command and protocol payloads.
`preimages.json`, `config-before.toml`, per-case configuration images and
`private-evidence.json` allow local review. The driver permits only the exact
native UI trust state, the single workspace trust entry, and the observed
screen-reader/model-nux UI fields (counter 1 to 8). Any other drift refuses.

Each case has a 15-second protocol deadline, then bounded cleanup. The hook has
an independent 5-second alarm; input/journal are capped at 64 KiB and each
journal row at 2 KiB. U02's transport bounds apply unchanged: 256 KiB protocol
lines, 2 MiB stdout/stderr, at most 8 admitted TCP connections, 4 concurrent
workers, 1 MiB provider request bodies, 2-second socket I/O and 5-second
connection lifetimes. Excess TCP connections are counted and closed. Cleanup
uses bounded joins and verifies process absence; UNKNOWN is not cleanup proof.

The private state directory is intentionally retained for UI withdrawal and
postimage review. After every native/UI process and provider is confirmed
stopped, the operator may remove that exact private directory. This diagnostic
does not modify shared helpers, global configuration or the user's native
installation. It does not establish OS-wide egress isolation or production
failure semantics beyond the exact pinned local observations.

## Offline and loopback tests

```sh
U07_REPO_ROOT="$REPO" "$PY" -B \
  "$REPO/docs/bench/native-integration-u07/test_failure_probe.py"
```

Tests execute fixed Python hook fixtures and a scripted Python app-server,
never native Codex. One test opens a rejecting loopback listener and verifies
HTTP refusal, occupied-port rejection and close/rebind cleanup. It requires
local bind permission; a sandbox denial is a test failure, not a skipped pass.
