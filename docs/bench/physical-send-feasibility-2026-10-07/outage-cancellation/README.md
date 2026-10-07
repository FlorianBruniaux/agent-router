# Controller-commanded cancellation with broker unavailable at startup

The local cancellation candidate passes: the pinned native Codex emits one
correlated `willRetry: true` notification while the broker is already
unavailable at native startup; the
controller sends one `turn/interrupt`, receives its response and observes a
correlated `turn/completed` with status `interrupted`. The broker listener is closed
before launching the native process. This qualifies the selected response to
that startup condition. It does not establish autonomous
termination when the controller waits without interrupting.

The [initial seven-case campaign](../README.md) remains **NO-GO**, including
its 15-second absent-broker timeout. Global admission, #131, F03b and monetary
or token budget qualification remain open. The initial scripts and results
are unchanged; this follow-up adds a separate candidate and evidence.

## Native observation and selected contract

[result.json](result.json) records the final local run, with Codex 0.159.2 and
SHA-256 `16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
The controller checks the notification's thread and turn IDs against the
native start responses **before** constructing an interrupt. Missing IDs,
another thread/turn and a non-retry notification select no interrupt.

| Evidence | Observed |
|---|---|
| Native launches / turn submissions / interrupt commands | 1 / 1 / 1 |
| Correlated retry notifications | 1 |
| Interrupt response | Acknowledged |
| Native terminal event | Same thread/turn, `interrupted` |
| Broker / provider HTTP requests | 0 / 0 |
| Provider accepted / rejected TCP connections | 0 / 0 |
| Process exit / output / owned process group | 0 / complete EOF / absent |
| Provider and broker handlers | Stopped |
| Both listener descriptors and connection checks | Closed; `ECONNREFUSED` |
| Protocol ceiling / post-interrupt ceiling | 15 seconds / 3 seconds |
| Final observed elapsed time including setup/cleanup | 1.182 seconds |

The response to `turn/interrupt` is not used as cleanup proof. Process-group
absence, output-reader completion, handler termination and socket closure
are checked separately. Only one turn is submitted and no replacement process
or inference request is launched. The original driver still reports its own
`native.checks_pass: false`, because that classifier expects autonomous
`failed` termination; this candidate separately reports `candidate_pass: true`
for commanded cancellation.

Two native runs were retained: the first observed cancellation, and the final
run added validation of identities before emitting the interrupt. An independent
review then found four classifier inconsistencies. Negative tests reproduced
them; the classifier was corrected and the unchanged final capture was
reclassified successfully. No native rerun followed that classifier correction.
[verification.json](verification.json) separates executed source hashes from
current candidate hashes and records eleven rejected mutations of the actual
capture. Eight Python tests pass.

## Reproduction and bounds

Run from the repository root on macOS with Python 3.11+, `sandbox-exec`, and
the exact pinned native binary. The parent diagnostic's HTTP, protocol,
connection, output and cleanup limits apply unchanged.

```sh
rtk proxy python3 -B docs/bench/physical-send-feasibility-2026-10-07/outage-cancellation/test_outage.py
NATIVE=/absolute/path/to/pinned/native/codex
STATE=/private/tmp/ar-f03a-cancellation-new-unique-directory
rtk proxy python3 -B docs/bench/physical-send-feasibility-2026-10-07/outage-cancellation/outage.py \
  --repo "$PWD" --binary "$NATIVE" --state "$STATE"
```

The state directory must not exist. The candidate imports the reviewed parent
fixture; it does not edit it. It supplies disposable native state and a
synthetic credential. The same Seatbelt profile denies networking except the
broker loopback port; there is no unsandboxed fallback or real-provider route.
Private requests, configuration, RPC and identities stay in the private state
directory. Public results include no raw RPC, prompt, endpoint port or local
path. No hook, live configuration or production capability is activated.

This proves the single tested startup outage response. Broker disappearance
during an active native session was not tested. Cancellation after an ambiguous
upstream send, tool/descendant isolation, restart, alternative transports,
Linux and real-provider behavior remain unverified by this follow-up.
