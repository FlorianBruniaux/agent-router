# Dedicated Codex entry: bounded qualification

8 October 2026. This diagnostic qualifies a dedicated direct client with the
local broker and controlled cancellation for the next simulated useful-patch
investigation. [ADR 0026](../../adr/0026-owned-client-simulated-entry.md) records
the bounded choice. Production integration, full OS admission and real-provider
execution remain unqualified. This is not an installed daily-use entry.

## What was exercised

The executable is Codex 0.159.2, SHA-256
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.
The client requests `gpt-6.1-sol/low` and one fixed synthetic task. The broker
validates that task and selection before forwarding. The fake provider's body,
independent request count and hash are checked against client evidence.

| Native case | Broker HTTP | Local provider HTTP | Correlated terminal | Cleanup |
| --- | ---: | ---: | --- | --- |
| Allow one request | 1 | 1 | completed | verified |
| Deny first request | 1 | 0 | failed | verified |
| Broker absent before launch, then interrupt | 0 | 0 | interrupted | verified |

See [native-results.json](native-results.json) for the final source-bound public
capture and [verification](../verification-owned-client-2026-10-08.json) for gates,
review and retained failures. Raw protocol, paths and IDs remain private.
Zero paid-provider calls occurred. The observed model/effort is an outgoing request
label, not evidence of real model inference. One reserved local HTTP request is
not a monetary or token budget.

The first native allow attempt refused the actual `thread/start` response before
any provider request: our fixture had omitted `networkAccess: false`. The fixture
was corrected from the redacted response and strict refusal tests added. Review
also required a durable refusal of `model/rerouted` and exponent-overflow JSON
numbers. A later final-source run also retained a NO-GO when the denied case's process
group was still visible at an immediate cleanup observation. Its cause remains
unproven. The corrected diagnostic records a separate check that accepts group absence only when observed within
three monotonic seconds after teardown. Late absence, persistent presence or
permission failure refuses; scheduling delay can extend the return time.
The final capture passes all three cases on unchanged recorded source hashes.
Initial failures remain recorded; a later success does not erase them.

## Client and broker contract

[direct_client.py](direct_client.py) admits one task and one turn, checks integer
RPC identities, correlates thread/turn and rejects unsupported selection before
transport. It retains messages within 256 KiB per line, 2 MiB total and 2,048
messages; malformed, duplicate-key, nonfinite or overflowing evidence refuses.
Unexpected permission callbacks receive a refusal and stop the client. Rerouting,
late results and ambiguous interruption cannot become successful completion.
The total protocol deadline is 15 seconds, with no separate proven 3-second
post-interrupt terminal bound. Physical teardown has its own bounded checks.

[native_probe.py](native_probe.py) composes that client with the existing
[F03a broker](../physical-send-feasibility-2026-10-07/README.md), disposable home
and the pinned executable. The diagnostic network profile denies outbound access
except the broker; no account state is copied. Returned read-only policy is checked,
but the outer profile is not a complete filesystem/process sandbox qualification.
Process stop, reader drain, handler stop and closed listener refusal are checked
separately from the interrupt acknowledgement.

The [ACP comparison](acp-comparison.md) executes real pinned components and rejects
its unchanged JSON reader under the evidence-loss contract. Component observations
are not full ACP/native parity; #131 remains open for its incomplete acceptance.

## Reproduce the bounded tests

From the repository root, ordinary tests use scripted peers and loopback fixtures;
they do not launch native Codex:

```sh
rtk proxy python3 -B -m unittest discover \
  -s docs/bench/owned-entry-qualification-2026-10-08 -p 'test_*.py' -v
```

An explicit macOS native diagnostic requires the exact pinned binary and a fresh
private output directory. `AR_CODEX_NATIVE` is an absolute path to that binary,
not the JavaScript launcher. It refuses another hash; it does not install or
activate hooks. The destination must not exist:

```sh
rtk proxy python3 -B docs/bench/owned-entry-qualification-2026-10-08/native_probe.py \
  --binary "$AR_CODEX_NATIVE" --state "$AR_PRIVATE_NEW_DIRECTORY"
```

The local quality runner registers this suite plus F03a and startup-outage unit
suites. Native probes remain explicit. Follow the [local runner guide](../../local-ci.md)
for the selected quality checks and coverage gaps.

## Verification scope

All 24 selected local quality checks passed on unchanged source hashes: 1,636
successful Rust workspace results plus one core-only check, and 355 Python tests.
Rust totals sum result lines, including subprocess invocations, rather than unique
test definitions. The 44 owned-entry tests are included, not added again. The 42
ignored native Rust tests are not successes; Linux, full OS admission, fresh audit,
fuzzing and release packaging are outside this selected run.

Independent review includes 17 schema tests, 28 CLI probes, runner failure/empty
suite probes, 20 client repair checks, five HTTP broker cases, nine ACP component
characterizations, eight cleanup observations and 28 negative cleanup mutations.
These overlapping review runs are not added to quality totals. Final captures were
independently reconciled against their source hashes and provider body files.
One test assertion received a floating-point tolerance after capture; runtime and
helpers remained identical. The initial interrupted quality run has no final
verdict and remains UNKNOWN; its logs were preserved before the complete rerun.

## What remains

F02a must produce and verify a useful simulated patch with preimage and path
binding. F12a/F02b/F03b must qualify the OS, native composition and integrated
send budget. Observation provenance, timing and external correlation still need
F04b/F05/F06 before real accounting collection. No automatic personal trial,
accepted real patch, Linux/Claude parity or savings is claimed here.
