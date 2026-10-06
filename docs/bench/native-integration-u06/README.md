# U06: apply one AR decision to a separate native Codex turn

Observed on macOS, 6 October 2026: the [native replay](result.json) passed.
The real CLI selected `gpt-5.6-terra` / `medium`; the enabled native turn sent
exactly that pair to the scripted provider. A fresh disabled turn sent the
baseline `gpt-6.1-sol` / `low`. Each case made one provider request, returned
the exact fixed output, exited zero and completed cleanup. Source and binary
hashes in the report identify this observation. No paid inference was used.

This local diagnostic runs the real Agent Router `route` CLI on frozen copies of
the committed Codex implementation request and catalog. It consumes the selected
`execute` stage once, applies its model and effort to a new native Codex
app-server turn, and verifies the exact scripted response and thread/turn/item
correlation. It does not execute the plan or verify stages or attach selected
skills. It does not change the parent session.

The provider listens on `127.0.0.1` and returns a fixed Responses SSE message.
It accepts one request only when model, reasoning effort and the exact prompt
carrying the AR decision ID match. No dynamic tool is registered and every
incoming native RPC callback fails the diagnostic. A second fresh session omits
the route overrides and verifies the baseline `gpt-6.1-sol` / `low` request.

The report kind is `external_native_fixture`. It is not an AR approved attempt,
receipt, runtime journal or execution authority. Native settings select the
read-only sandbox, but this fixture does not independently attest an OS sandbox
or broad egress denial. Paid-provider eligibility, real-world task quality,
cost and automatic daily interception remain UNKNOWN or NOT_TESTED. Usage in the
scripted response is synthetic. Supported routed pairs are an explicit bounded
subset of the active platform's models, not a claim about provider access.

## Offline checks

Use Python 3.11 or newer. From the repository root:

```sh
AR_U06_REPO_ROOT="$PWD" python3 -m unittest discover \
  -s docs/bench/native-integration-u06 -p 'test_*.py'
```

These checks use Python child processes and loopback HTTP, never native Codex or
inference. A sandbox that refuses loopback listeners needs permission for that
test invocation. The fixture tests cover route rejection, one-use dispatch,
changed selections, exact provider parameters, correlation failures, duplicate
completion, native-start refusal, cleanup requirements and byte/time ceilings.

## Explicit native replay

Run only after reviewing the candidate and explicitly authorizing this local
native diagnostic. Supply the exact approved router binary hash. The native
binary must be Codex `0.159.2`, SHA-256
`16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704`.

```sh
python3 docs/bench/native-integration-u06/probe.py \
  --repo-root "$PWD" \
  --router-binary /absolute/path/to/frozen/agent-router \
  --router-sha256 APPROVED_ROUTER_SHA256 \
  --native-binary /absolute/path/to/pinned/codex \
  --output /private/tmp/u06-result.json
```

The program prints the sanitized report path and private evidence directory.
Raw provider/RPC data and frozen routing inputs remain in an owner-only
`/private/tmp/ar-u06-evidence-*` directory. Native HOME, configuration, cache,
workspace and processes are isolated and removed. Environment variables are
allowlisted through the frozen helper; no existing credentials are loaded.
The helper's API key is a fixed nonfunctional fixture string.

## Bounds and evidence

- Route inputs and stdout/stderr: 1 MiB each; route deadline: 10 seconds.
- Each native protocol exchange: 15 seconds; inherited RPC line cap: 256 KiB;
  native stdout/stderr cap: 2 MiB each.
- Provider request body: 1 MiB; one accepted request per session. Excess requests
  fail. Inherited transport ceilings bound four simultaneous handlers and eight
  total connections, with two-second socket and five-second connection limits.
- Owned process groups and provider handlers are shut down using the frozen
  helpers' three-second cleanup bounds. Missing cleanup/output evidence fails.
- The report records router/native/source/input/output hashes, the selected
  stage, requested and observed model/effort, native IDs, exact local verification,
  counts, cleanup and the disabled control. It does not export raw prompts.

The implementation imports, without editing or duplicating, the frozen helpers
in `docs/bench/egress-admission/codex_appserver_local.py` and
`docs/bench/native-integration-u02/probe_v5.py`. The disabled control demonstrates
absence of this fixture's override in a new isolated session, not removal of
live user configuration. Duplicate prevention is controller-local, not global
idempotency or a hook transport replay guarantee.
