# F03a: physical-send broker feasibility, 7 October 2026

The bounded campaign is **NO-GO overall**: six native transport cases pass,
but removing the broker leaves the native turn waiting for network beyond the
15-second deadline. The supervisor then stops the process; both listeners
record zero HTTP requests and cleanup passes. Native terminal behavior for
that outage is **UNKNOWN**, not a successful failure result.

The six passing cases establish a useful control point for the pinned Codex
0.159.2 executable: an HTTP broker can reserve one local request unit before
opening its provider socket, including when Codex retries a scripted 503.
They do not admit native execution, establish a monetary/token ceiling, or
close [ADR 0022](../../adr/0022-provider-egress.md),
[ADR 0023](../../adr/0023-owned-task-entry.md), #131, #132 or #133.

## Observed native behavior

[results.json](results.json) is the sanitized projection of the second native
campaign. Each row starts a real pinned app-server with fresh private state.
The client submits exactly one `turn/start`; the broker never retries upstream.
A separate HTTP listener counts provider requests independently of the gate.
The admitted request bodies have matching SHA-256 values on both listeners.

| Case | Broker HTTP | Provider HTTP | Native turn | Observation |
|---|---:|---:|---|---|
| `allow` | 1 | 1 | completed | Fixed synthetic output observed |
| `deny_first` | 1 | 0 | failed | Initial reservation refused |
| `retry_allow` | 2 | 2 | completed | Provider's first 503 triggers native retry |
| `retry_deny` | 2 | 1 | failed | Native retry reaches gate; second send refused |
| `guard_disabled` | 1 | 0 | failed | Disabled admission refuses forwarding |
| `guard_failed` | 1 | 0 | failed | Injected admission exception refuses forwarding |
| `broker_absent` | 0 | 0 | UNKNOWN | Native network-wait notifications; 15-second timeout |

The retry cases use `request_max_retries = 1`, `stream_max_retries = 0`, HTTP
Responses and `supports_websockets = false`. In the outage case, the raw RPC
contains two `error` notifications with `willRetry: true` and the message
`Reconnecting... waiting for network`. Thus those retry settings do not give
the controller a terminal outage guarantee within this bound.

The first six-case campaign passed before the absent-broker case was added.
The final seven-case campaign exits 2 because the absent-broker terminal
assertion fails. That failed assertion and its raw evidence are retained.
Twelve diagnostic Python tests pass, covering initial/second denial, guard
failure/disablement, concurrent reservations, HTTP method/path/model/size
refusals, retaining a reservation after an unavailable upstream, and rejecting
inconsistent HTTP counters or missing request hashes. An independent review
found that the first classifier ignored total HTTP counts. Three negative
assertions reproduced that defect before correction. Reclassification of the
unchanged native captures preserves all seven outcomes; native was not rerun.
The verification record distinguishes the executed source hashes from the
corrected classifier hashes.

## Control point and evidence boundary

```text
real Codex app-server, one turn
  -> local broker POST /v1/responses
     -> validate bounded request and requested model
     -> reserve one request unit atomically, or return HTTP 403
     -> http.client.HTTPConnection.request (one physical upstream send)
        -> separate scripted provider listener
```

`Gate.admit()` runs immediately before the broker creates its upstream HTTP
connection. The upstream address and path are fixed in the diagnostic; caller
URLs, general CONNECT forwarding, ambient proxy variables and redirects do
not supply a second transport. The broker buffers at most 1 MiB of a response;
it is not a production streaming gateway. A successful reservation remains
consumed if upstream connection/response handling fails. There is no refund,
price conversion, persistent budget, real provider authentication or journal
receipt in this diagnostic. Existing AR stores and runtime code are unchanged.

The native process inherits a replacement environment with synthetic key only
and runs under a diagnostic Seatbelt profile denying networking except the
broker's loopback port. Before each case, a socket control under that exact
profile reaches the broker and gets `EPERM` connecting directly to the fake
provider. The control connection carries no HTTP and is excluded from the
HTTP table above. No external provider endpoint is configured or contacted.

This demonstrates the tested socket rule, not complete OS admission. The
profile allows default filesystem/process operations and does not qualify
built-in exclusion, controller/executor credential separation, inherited
file descriptors, system-service channels, malicious config replacement,
external network routes or Linux. Removing the OS profile or changing the
native provider config is outside this result. Tool execution, ACP, Claude,
WebSocket reconnects, auth recovery and other provider APIs remain untested.
Global admission stays **NO-GO**. A synthetic response does not establish model
identity, inference quality, real token consumption or an invoice amount.

## Reproduce without live configuration

Python 3.11+, the exact native executable and macOS `sandbox-exec` are required.
Loopback binds and applying a Seatbelt profile must be permitted by the caller's
execution environment. There is no fallback to an unsandboxed native run.

```sh
REPO="$PWD"
PROBE="$REPO/docs/bench/physical-send-feasibility-2026-10-07/probe.py"
NATIVE=/absolute/path/to/pinned/native/codex
STATE=/private/tmp/ar-f03a-new-unique-directory
rtk proxy python3 -B "$REPO/docs/bench/physical-send-feasibility-2026-10-07/test_probe.py"
rtk proxy python3 -B "$PROBE" --binary "$NATIVE" --state "$STATE"
```

`STATE` must not exist and its parent must exist. The program creates it with
mode 0700 and never removes or overwrites another run. Native SHA-256 must be
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
Only disposable homes/configuration are written. No hook or global host
configuration is enabled, and no personal credentials are read by the driver.

Each native case has a 15-second protocol bound plus bounded cleanup. Reused
U02 helpers cap protocol lines at 256 KiB, stdout/stderr at 2 MiB each,
connections at 8, concurrent handlers at 4, request bodies at 1 MiB, socket I/O
at 2 seconds and connection lifetime at 5 seconds. The broker adds a 1 MiB
response cap. Owned process groups are stopped and checked absent; listeners
and handlers are joined. Private homes and raw evidence are retained after
cleanup for inspection. Do not run this against a remote or real provider.

Private evidence contains full synthetic provider requests, native RPCs,
configuration and exact sandbox profiles. Keep it private because native
messages contain local paths and identities. Public exports contain counts,
request hashes, decisions and bounded outcome fields, with no raw request or
RPC body. [verification.json](verification.json) records source hashes,
commands, test results and the explicit campaign failure.

## Next dependency

Resolve the broker-outage terminal/cancellation contract with a bounded native
observation, then decide the replacement transport under #131. Production
admission still needs ADR 0022's controller/executor separation and native
negative-path comparison. F03b must bind reservations to existing AR journal
identities and test concurrency, ambiguous effects, restart and delayed usage.
No new paid trial is authorized by this evidence.

## Follow-up: controller-commanded outage cancellation

The separate [outage cancellation diagnostic](outage-cancellation/README.md)
observes a correlated interrupt and physical cleanup after the first native
retry notification with the broker already unavailable at native startup. It qualifies that selected controller response only; this
initial campaign and global native admission remain **NO-GO**.
