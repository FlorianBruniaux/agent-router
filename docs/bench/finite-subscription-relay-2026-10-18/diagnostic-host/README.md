# Subscription transport diagnostics, offline candidate

The historical campaign stopped with `usage_unknown`. Its upstream error was not retained, so its cause and consumption remain **UNKNOWN**. This version fixes the loss of diagnostic evidence for future requests. It does not reconcile that attempt or authorize another model call.

## Change and source boundaries

The unchanged pinned subscription sender replaces connection and streaming exceptions with generic transport errors, and returns HTTP failures without retaining their error details. The historical host persisted no separate transport diagnostic. A refusal, TLS failure, timeout or incomplete response therefore could not be distinguished from the retained evidence.

This version wraps that sender through its existing connection-factory argument. The destination, request bytes, subscription authentication, no-retry behavior and seven qualified controller dependencies stay unchanged. A new host-only observer records a bounded diagnostic alongside the private campaign directory. It cannot produce an accounting receipt or change the results manager's verdict.

The launcher binds the observer's SHA-256 into its outer review, verifies those bytes again before loading them, and keeps the channel and controller in the same reviewed Session namespace. Its sibling parent is byte-identical to the historical parent; the companion copy selects this version's launcher. The five historical Python files under [host](../host/README.md) remain byte-identical. [verification.json](verification.json) records their hashes, the seven controller pins and this candidate's Python hashes.

Historical approvals cannot activate this candidate. New code, paths and diagnostic material require a new review. This candidate has offline evidence only; the separate Linux F12a qualification does not qualify this new host path or prove provider availability.

## Retained evidence

For each trusted gateway terminal, `<assignment_id>-transport-diagnostic.jsonl` can retain only:

- Public sealed assignment and sequential request identifiers.
- Transport phase, finite failure category, HTTP status and normalized media category.
- An allowlisted upstream error code, if present in a bounded error response or SSE terminal.
- SSE terminal category, usage validity and response-model match category.
- The gateway completion flag and `accounting_authority: false`.

No response body, output text, exception text, traceback, headers, token, account identifier or raw returned model is serialized. Error bodies are read up to 16 KiB plus one sentinel byte, within the original observer deadline. SSE observation is incremental and bounded. Diagnostics reset after every terminal, including when the next request is refused before transport.

The owned directory must be private (`0700`). The diagnostic is reserved before authentication or connection creation, with exclusive creation, no symlink following and mode `0600`. Existing files are refused. A sidecar contains at most 64 records of 1 KiB each. A diagnostic write failure propagates through the existing conservative stop path; it never permits another trial.

The 300-second preparation deadline, 60-second worker window, 6,000 observed-token threshold and existing reactive equivalent-credit threshold stay unchanged. These observed thresholds do not establish a hard provider billing cap. Missing usage or cleanup still stops advancement. Neither a diagnostic HTTP rejection nor a valid usage field is authority to replace unknown consumption with zero.

## Offline verification

Run from the repository root:

```sh
python3 -B docs/bench/finite-subscription-relay-2026-10-18/diagnostic-host/test-offline.py
```

The checker verifies the historical five hashes, seven controller pins and sealed public packet hash. It runs the 26 historical tests in disposable rebound copies, 20 transport regressions and three host integration tests: **49 tests**. Each test interpreter and simulated Runtime child loads the existing audit guard, which rejects sockets, credentials, live campaign registry opens, writes outside the fixture and non-fixture subprocesses. Four refusal probes run first. No provider, credential or real registry is used.

The integration fixture exercises the actual parent, controller, private pipes and synthetic results manager with a simulated HTTP 401. It retains a safe diagnostic, confirms cleanup and leaves one assignment stopped with the other 23 `not_run`; the next assignment stays unavailable. The transport tests cover successful bytes and usage, missing usage, TLS/auth/timeout failures, split and malformed SSE events, file refusal before authentication, deadline preservation, cleanup failures and isolation between requests. They do not identify the historical failure.

CI runs this checker on Linux and macOS with Python 3.12. Local verification details are recorded separately; a local pass is not a remote CI result.

## Remaining launch gate

Keep the actual campaign and its registry unchanged. Historical consumption still needs reconciliation through trusted evidence before advancement. A future live run also needs a fresh review of the versioned host, applicable native qualification and explicit authorization. This patch prepares observable transport failures; it does not clear `usage_unknown`, approve a budget or restart the benchmark.
