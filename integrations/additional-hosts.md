# Additional host and provider contracts

`router-bridge-contract` is a separate integration package. It includes native
argument preparation and bounded event normalization for Copilot CLI and
OpenCode, plus OpenAI Responses and Anthropic Messages request translation and
SSE normalization. It makes no network call and starts no process. Availability,
authentication and runtime capabilities remain unknown until independently
observed; implementation of a translator does not establish host support.

## Copilot and OpenCode

Both implement the common `HarnessAdapter` and consume only
`ValidatedExecutionRequest`. The application must first rebuild and validate
the phase receipt. Prompts and selected instruction skills use stdin, with the
same bounded preparation and redacted previews as Codex and Claude Code.

Copilot receives exact `--model`, `--effort`, JSONL output, working directory
and controls disabling auto-update, remote export and diagnostic logging.
Read-only requests start in plan mode. An explicit tool subset uses
`--available-tools`; no permission-granting option is emitted. Fully disabling
all tools is rejected because empty-list semantics were not established.
`auto` model selection is rejected. Unsupported effort levels are rejected.
Named roles and native subagent requests require the existing explicit
degradation permission to become generic external workers. These translations
follow GitHub's official
[command reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
and [programmatic reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference).

The Copilot decoder uses the
[official SDK event contract at d106d29](https://github.com/github/copilot-sdk/blob/d106d29dc6c5112da2abdae59008571b6692f12b/nodejs/src/generated/session-events.ts).
Only native `assistant.usage.data.model` without child-agent attribution can
establish the worker's observed model. Assistant message text cannot. Exit code,
terminal event, validation and objective are separate evidence.

OpenCode preparation follows the
[official CLI source at b471c2b](https://github.com/anomalyco/opencode/blob/b471c2b4495747353af768fbf2e0790c9d820ce2/packages/opencode/src/cli/cmd/run.ts):
`run --model provider/model --variant LEVEL --format json --dir WORKSPACE`,
with stdin input. It rejects unqualified model IDs and unsupported tool subsets.
The JSONL decoder recognizes step, text, tool and error events. It does not infer
an executed model absent native metadata. A step finish only establishes a
complete stream when its reason is `stop`.

OpenCode's [V2 model documentation](https://opencode.ai/v2/docs/models) describes
`provider/model#variant`. This differs from the inspected CLI source. The
adapter does not silently change dialects: deployments must verify the
executable and model-to-variant mapping before supervision. Neither Copilot nor
OpenCode was installed in the implementation environment, so actual invocation,
hook trust, permission enforcement and provider access were not verified.
The runtime must supply filesystem/network confinement and bounded supervision.

## Provider APIs

`OpenAiResponsesAdapter` and `AnthropicMessagesAdapter` implement the distinct
`ProviderAdapter` trait. They accept only a read-only external-worker request
with tools disabled and no host agent. A provider protocol does not provide a
workspace, native subagents or a tool loop. Instruction skills are rendered as
bounded prompt content, not executed.

OpenAI payloads target `POST /v1/responses`, with model, input, explicit reasoning
effort, output-token bound, streaming and `store: false`. Anthropic payloads
target `POST /v1/messages`, with model, user content, explicit
`output_config.effort`, output-token bound and streaming. Static headers contain
no credentials. Unsupported effort vocabulary is rejected; model-specific
support must already exist in the validated snapshot. Sources:
[OpenAI Responses](https://platform.openai.com/docs/api-reference/responses/create),
[Anthropic Messages](https://platform.claude.com/docs/en/api/messages/create),
[Anthropic effort](https://platform.claude.com/docs/en/build-with-claude/effort),
[Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming).

The runtime owns `EnvelopeAuthority` and signs a route envelope using Ed25519 in
the `agent-router:provider-route:v1` domain. The envelope binds run, receipt,
model, effort, mode, authorization digest, complete request context hash and
payload hash. It remains outside the provider payload, along with workspace
and source context. The runtime never gives its private key to an adapter child
or provider. `verify_provider_request` requires an independently trusted public
key, the validated execution request and expected run ID, then checks signature,
payload, context, route, static headers, output bound and timeout.

The signer is an authority boundary. Before signing, the runtime must validate
the phase receipt, user approval, data policy and output-token budget. A caller
must not treat a key supplied by an untrusted envelope as trusted. The public
key is therefore supplied to verification separately. Preparing or signing a
dry-run request does not authorize transmission; the supervisor must check mode.

SSE normalization retains event kinds and model metadata only. It rejects
duplicate fields, mismatched event types, oversized frames and malformed
completed streams; interrupted partial streams remain incomplete. Provider
text cannot supply objective or validation success. Transport status comes from
the supervisor, not SSE content. For an HTTP supervisor, map successful HTTP
transport to `Exited/0`, non-success status to a nonzero outcome, and cancellation
or timeout to the corresponding termination. No normalized output includes
provider text, prompt content, raw errors or authorization headers.

These are protocol implementations with synthetic conformance fixtures. A live
HTTP transport, authenticated endpoint selection, credentials, native probes,
API compatibility canary and plugin activation must be supplied and verified by
the deployment. The conservative `Deferred*Bridge` types remain available for
surfaces where no checked native contract is selected; they always refuse.

```sh
cargo test -p router-bridge-contract --locked
```
