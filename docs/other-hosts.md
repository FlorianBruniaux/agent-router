# OpenCode and Copilot supervised transports

`adapter-opencode` and `adapter-copilot` implement the product
`router_adapter_api::HarnessAdapter` contract. They prepare a typed invocation
and decode bounded output. The existing runtime owns execution, confinement,
termination, durable receipts and verification. These crates neither install
hosts nor start processes, enable hooks, grant permissions or modify the parent
conversation. They are separate from `integrations/bridge-contract`.

## Integration contract

Both adapters expose `new(config)`, `from_json(bytes)`, `config()` and
`config_hash()`. Parsing rejects unknown configuration fields and duplicate JSON
keys. Hashing is deterministic, domain separated and includes the entire typed
configuration. An outer execution approval must bind this configuration as well
as the request and executable identity. Changing a model variant or version pin
requires a new approval.

Both expose `final_text(&self, bytes, outcome, max_bytes) ->
Result<Option<String>, AdapterError>`. This uses the same parser as
`decode_stream`; it does not scan arbitrary JSON for a result string. It returns
content only after successful, complete transport without host errors. OpenCode
binds text parts to the final completed message and deduplicates part updates;
Copilot keeps only the final root assistant message before the terminal result.
Reasoning, tool output, child messages and incomplete deltas are excluded. The
returned text remains untrusted confidential content for explicitly declared
artifacts, never ordinary preview/audit output.

Each crate also exposes `parse_version(&[u8]) -> Result<String, AdapterError>`.
OpenCode requires exactly one version line ending in LF or CRLF. Its format comes
from the pinned
[CLI entry point](https://github.com/anomalyco/opencode/blob/b471c2b4495747353af768fbf2e0790c9d820ce2/packages/opencode/src/index.ts),
SHA-256 `bddea4eb640adb6fe72193f292c1ae75526ff1bf5bc507ff7c539e4e93825dbc`.
Copilot requires the full two-line `--version` template from its distributed
source: the version follows `GitHub Copilot CLI `, ends in a period, and is
followed by the fixed update hint. The `copilot version` subcommand is different
and performs an update check. No substring scan or first-token fallback is used.
Tests separate source-derived templates from synthetic malformed input; neither
is labeled as an observed native execution.

An `expected_cli_version` is an operator requirement. It is not discovery or
runtime evidence. Before dispatch, the application must compare a fresh version
probe of the approved executable with this pin. A cache, a provider name or this
configuration never makes a host capability supported. The shared execution
gate requires a valid host snapshot, exact model and effort availability,
capabilities and effective restrictions; execute mode additionally requires its
exact authorization digest.

Prompts and selected instruction skills go only to `PreparedRun.stdin`. They do
not appear in arguments, serialized previews or debug formatting. The runtime
must pipe these bytes directly and close stdin. Preparation accepts neither a
shell command nor caller-supplied extra flags. Named native agents and native
subagents require explicit degradation to an external worker with an instruction
role. Skills remain hashed instructions, never tool grants.

## OpenCode

Host and program are `opencode`. `OpenCodeConfig` schema 1 contains
`expected_cli_version`, `protocol: "run_v1"` and
`model_variants: { "provider/model": { "high": "provider-variant" } }`.
Every requested model/effort pair needs an explicit mapping. There is no
universal conversion from router effort to provider variant.

The adapter emits `run`, the exact model and variant, JSON output and working
directory. Only an already authorized `ToolSelection::HostPolicy` is supported.
Disabled tools and allowlists are rejected even when worker degradation is
allowed. The adapter never auto-approves a permission request. OS read/write and
network restrictions remain the supervisor's responsibility.

The pinned implementation is
[OpenCode run.ts](https://github.com/anomalyco/opencode/blob/b471c2b4495747353af768fbf2e0790c9d820ce2/packages/opencode/src/cli/cmd/run.ts),
SHA-256 `bde848ca17f30834beb4ef0dfecb3b819730746ec2023c1ac55ecef7ca25dd37`.
It reads a non-TTY prompt from stdin. The adapter recognizes the emitted
`step_start`, `step_finish`, `text`, `reasoning`, `tool_use` and `error` envelopes,
checks their session association, and distinguishes an intermediate tool step
from a final stop. Model and effort execution remain `UNKNOWN`: run-v1 JSON does
not expose authoritative model-call identity. Textual claims do not change this.
OpenCode v2 uses a different dialect and is not silently substituted.

This interface has no verified per-invocation switch disabling native session
persistence or configuration-driven auto-sharing. Both limitations are present
in preparation evidence. The surrounding host configuration and supervisor must
be reviewed before using confidential inputs; the adapter does not rewrite them.

## Copilot

Host and program are `copilot`. `CopilotConfig` schema 1 contains
`expected_cli_version` and `protocol: "cli_jsonl_v1"`.

The adapter requests the exact model, documented effort and JSONL output.
Piped prompts are supported independently of `-p`, which would override stdin.
[GitHub programmatic invocation](https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically)
documents this behavior.

Accepted efforts are `low`, `medium`, `high`, `xhigh` and `max`, additionally
subject to the snapshot gate. Automatic model aliases and other efforts are
rejected. A nonempty allowlist restricts availability to known built-in tool
identifiers. It never grants permission to run them. `HostPolicy` preserves the
already authorized native policy. Empty/disabled tool sets and unknown tools
are unsupported. The native host may reject an invocation when its existing
permissions do not suffice; the adapter does not retry with broader grants.

The invocation disables auto-update, remote export/control, interactive
questions, shell environment initialization, custom instruction loading and
ordinary host logging. Read-only requests additionally select plan mode.
Plan mode is not OS confinement. Flags that a particular version or account
cannot honor must fail; they are not silently removed.
[GitHub command reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
defines these options and tool availability values.

The adapter does not disable native session persistence or establish hook,
plugin, authentication or model availability. These remain external observations.
No session is resumed, shared or attached by the prepared command.

CLI JSONL must not be confused with SDK event streaming. The reviewed source is
`package/app.js` from the official
[@github/copilot 1.0.44 distribution](https://registry.npmjs.org/@github/copilot/-/copilot-1.0.44.tgz),
SHA-256 `473255d34d10e49702e781686cd04cfbda89bbe42cfd494c820f7d0e02d6b857`.
The archive integrity was checked against its registry metadata; archive SHA-256
is `b7881e6c105d62224cd9cffdcefbe2b0eba346749cc738ef7a7ef35c2e7f3664`.
This source filters SDK `session.idle` and `assistant.usage` and emits a separate
`result` with `timestamp`, `sessionId`, `exitCode` and `usage`. The adapter checks
that terminal shape, session association and agreement with the supervisor's
exit status. SDK-shaped usage and selected-model fields cannot prove a CLI model
call. Executed model and effort stay `UNKNOWN` for this dialect.

The terminal schema source pin is not a claim that every emitted modern flag
exists in CLI 1.0.44. In particular, remote-export control requires a newer host
with the documented option. The operator must supply a version supporting the
whole invocation, then verify that exact binary in a canary before production.
The current 1.0.88 distribution was inspected as an inert archive; it contains a
packed executable and was not run. No claim of current native conformance follows
from the older readable source.

## Evidence and verification limits

Both decoders cap total output at 4 MiB, one event at 256 KiB and a stream at
2,048 events. Duplicate keys and malformed recognized envelopes fail. A partial
last line is tolerated only after interrupted transport and keeps the stream
incomplete. Unknown events cannot supply completion or execution identity.
Only the supervisor supplies exit status. Transport, stream completion,
validation and objective outcomes remain separate; the last two always start
as `UNKNOWN` regardless of assistant text.

Local tests use synthetic events, an explicitly synthetic host snapshot and a
local stdin echo process. They cover exact approval, refused permissions,
redaction, stream limits, interruption and attribution. Neither executable was
installed on the development machine during this adapter work. There was no
paid model call or native end-to-end conformance run. Installed version,
authentication, runtime capabilities and provider behavior therefore remain
`UNKNOWN` until an independently approved native canary supplies evidence.

Run the crate checks with:

```sh
CARGO_BUILD_JOBS=4 cargo test -p adapter-opencode -p adapter-copilot --locked --offline
CARGO_BUILD_JOBS=4 cargo clippy -p adapter-opencode -p adapter-copilot --all-targets --all-features --locked --offline -- -D warnings
```

## CLI configuration and dispatch

Build optional transports explicitly:

```sh
cargo build -p agent-router-cli -p router-sandbox-helper --features agent-router-cli/opencode,agent-router-cli/copilot --locked
```

The existing sandbox manifest accepts these optional top-level fields when the
corresponding feature is built. Existing manifests omit both fields and retain
their prior approval serialization:

```json
{
  "opencode": {
    "schema_version": 1,
    "expected_cli_version": "1.0.0-fixture",
    "protocol": "run_v1",
    "model_variants": { "synthetic/model-v1": { "high": "provider-high" } }
  },
  "copilot": {
    "schema_version": 1,
    "expected_cli_version": "1.0.0-fixture",
    "protocol": "cli_jsonl_v1"
  }
}
```

This fragment uses synthetic values. Supply the actual version and exact
provider variant mapping for an installation, alongside all required sandbox
fields from the execution guide. The configured `programs` map must bind
`opencode` or `copilot` to its absolute executable path. Missing configuration
or an unavailable feature fails review. The complete typed configuration and
approved binary hashes participate in the outer CLI approval digest; a changed
pin or variant mapping requires a new approval.

After capacity is acquired and before any model worker or worktree is created,
the runtime executes only `--version` through the same approved native sandbox
and resource supervisor. The probe has empty stdin, a private empty working
directory, no project scope, read-only access, no network, a five-second deadline
and an 8 KiB combined output ceiling. A durable watchdog cleanup proof, clean
exit, empty stderr and exact host-specific version parse are all required.
Raw version output is neither logged nor included in the CLI projection.
The approved binary is hashed again when the actual worker command is built.

Version evidence establishes only the native version response. It does not
establish credentials, model availability, executed model identity, effort or
provider behavior. Fake-host canaries exercise real sandbox/probe mechanics;
OpenCode and Copilot themselves have not been executed on the development host.
Their declared final-text artifacts use the adapters' strict complete-stream
parsers, with the same private retention and explicit verification requirements
as other workflow results.

`agent-router-cli/tests/native_hosts.rs` covers both hosts through the public CLI
under a real native sandbox and resource helper. Synthetic executables verify
the exact version/worker arguments, private output and host/model feedback.
Wrong versions produce zero attempts and no worker worktree. An ordinary CLI
test also proves that changing a version pin invalidates the old approval.
These tests do not substitute for running the actual vendor executables.

```sh
cargo test -p agent-router-cli --test native_hosts --all-features --locked -- --ignored --test-threads=1
```
