# Optional decision backends

`router-decision` constructs a pool from a validated phase context and checks
every selection through `PhaseRouter`. The deterministic receipt remains the
active result. A semantic proposal can only appear as a shadow candidate.
There is no model download, Python dependency, or server startup in the core.

The request contains a context digest, task kind, risk, phase, and at most 20
opaque candidate IDs with numerical utility, cost, token and latency features.
It excludes objectives, file paths, model names, asset bodies and prompts.
The default data policy forbids sending even this reduced request.

Enable the optional HTTP adapters at build time:

```sh
cargo build -p agent-router-cli --features local-sidecar --locked
```

An explicitly started local Laya or Jev-compatible server can then receive a
shadow request:

```sh
agent-router backend --backend laya --endpoint 127.0.0.1:8000 \
  --allow-redacted-metadata --timeout-ms 500 --on-error fallback \
  --context fixtures/examples/phase-request.json \
  --host-snapshot fixtures/examples/phase-snapshot.json \
  --catalog fixtures/examples/phase-catalog.json --now 100 --json
```

These fixtures are synthetic and deliberately use timestamp 100. Real inputs
must contain current host observations. `--bearer-env VARIABLE_NAME` reads one
explicitly named environment variable into memory. Its value is absent from
diagnostics and receipts. No environment variable is changed.

The transport accepts numeric loopback addresses only. It does not follow
redirects, use DNS or proxies, or retry inference. The total I/O deadline is
bounded to 10 seconds. Request and response limits are 32 KiB and 64 KiB;
headers are limited to 16 KiB. HTTP success requires status 200, JSON content,
and a bounded Content-Length. Compressed and chunked responses are rejected.

The adapters use `POST /v1/systemone` with one choice question. This is the
documented [Laya server contract](https://github.com/NandhaKishorM/laya/blob/main/laya/serve.py).
The decoder accepts typed choice answers and optional action metadata. It uses
`answer_confidence`, or the selected answer probability when available. It never
uses the legacy confidence field, which Laya distinguishes as entropy in its
[answer decoder](https://github.com/NandhaKishorM/laya/blob/main/laya/agent.py):
a choice with neither value is refused as `backend.schema`, and an abstention
without a choice reports a confidence of zero.
Probabilities rounded to four decimal places have a bounded mass tolerance.
Neither confidence nor a successful HTTP call establishes routing quality.

Duplicate JSON keys, unexpected answer fields, invented candidates, mismatched
digests, invalid probabilities, low confidence, timeout and transport failure
produce the selected fallback or abstention. Raw server responses never enter
error messages. Unit and local HTTP fixture tests exercise these boundaries;
they do not establish behavior or quality of an installed Laya checkpoint.

`router-eval` independently audits replayed selections and evaluates promotion
criteria. Its verdict alone changes no runtime configuration. Until held-out
human labels and a measured improvement exist, the semantic backend remains
disabled for production selection.

## Standard protocol client

`--backend standard-v1` is an additional explicit choice under `local-sidecar`.
It negotiates `/v1/capabilities`, pins the expected backend artifact and sends
`/v1/rank` under a single deadline. Legacy Jev/Laya behavior is unchanged.
See [the standard sidecar protocol](standard-sidecar.md) for strict wire contracts,
explicit single-event outcome/feedback commands and delivery uncertainty.
