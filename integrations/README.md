# Host integrations

The Codex and Claude Code integrations prepare native CLI arguments through the
same `HarnessAdapter` contract. They contain no process launcher, credential
reader, network client, global configuration writer or permission grant. The
manifests in this directory describe the packaged adapters; they are inert and
are not native host plugin manifests. No installation or activation is implied.

## Authority boundary

The application must authenticate its input sources, rebuild the phase context,
recompute and compare the complete `PhaseReceipt`, and run
`PhaseRouter::validate_selection` before calling `authorize_execution`. The
common adapter gate checks exact observed model and effort, known capabilities,
restrictive permission intersections, bounded prompts, instruction hashes and
safe typed identifiers. It cannot establish that a caller-supplied receipt,
snapshot or approval came from an authoritative source merely from its hash.

For execute mode, `execution_authorization_digest` binds the full request,
including receipt, source state, scope, model, effort, prompt and skill content,
to the validated snapshot hash and effective restrictions. Supplying that digest
is an application responsibility. It is not a transferable permission grant.
Dry-run and shadow produce previews only; the supervisor must refuse to spawn
them. No serialized preview can be deserialized into `ValidatedExecutionRequest`.

The supervisor supplies process timeout, cancellation, bounded capture, exact
executable resolution and working-directory/symlink checks. It must establish
filesystem and network confinement before starting a worker. Host permission
flags alone are not proof of OS confinement. HostPolicy tool selection retains
the host's existing policy and does not authorize arbitrary tools or MCP
servers. Any hook, plugin or MCP entry point must call this same application
boundary and must not call an adapter to evade receipt validation.

## Native translations

| Request | Codex | Claude Code |
| --- | --- | --- |
| Exact model | `exec --model ID` | `--print --model ID` |
| Exact effort | `--config model_reasoning_effort="LEVEL"` | `--effort LEVEL` |
| Read-only | `--sandbox read-only` | `--permission-mode plan` |
| Workspace write | `--sandbox workspace-write` | `--permission-mode manual` |
| Output | `--json` | `--output-format stream-json --verbose` |
| Prompt and verified instructions | stdin | stdin with `--input-format text` |
| Tool subset or disabled tools | Explicitly unsupported | `--tools LIST` or empty argument |
| Named agent without installation evidence | Reject, or explicit generic-role degradation | Reject, or explicit generic-role degradation |
| Native subagent target | Reject, or explicit external-worker degradation | Reject, or explicit external-worker degradation |
| Parent conversation model change | Unsupported | Unsupported |
| Executable skill | Unsupported | Unsupported |

Codex arguments also use ephemeral execution and disable terminal color. Claude
arguments disable session persistence and use `--permission-prompts none`,
which denies prompts rather than approving tools. Claude accepts low, medium,
high, xhigh and max at this CLI version; other effort levels are rejected even
if a synthetic snapshot advertises them. The common gate still requires the
chosen model to explicitly advertise the requested effort. The adapters never
add dangerous approval/sandbox bypasses, hook-trust bypasses or `--allowedTools`.

Native flag evidence was collected from `codex-cli 0.153.4` (`codex exec --help`)
and Claude Code `2.1.283` (`claude --help`). The Codex effort configuration key is
also documented in the official
[configuration reference](https://developers.openai.com/codex/config-reference/).
These facts identify the inspected clients, not a guarantee about future or
older client versions. A runtime deployment must pin or recheck its executable.

## Observed evidence

`decode_stream` accepts only bytes captured from the supervised native process
and a `TransportOutcome` supplied by the supervisor. Do not pass assistant text,
user-uploaded JSONL or a self-reported exit code as authenticated execution
evidence. A digest provides integrity, not authentication.

The decoder retains bounded event kinds and metadata hashes; raw prompts,
assistant text, tool results and provider errors are absent from its output.
Exit code zero means transport success. It does not mean validation or objective
success: both remain `unknown`. Missing terminal records and interrupted partial
JSON are reported as incomplete streams. Malformed completed streams and
duplicate JSON fields are rejected. Size limits cover the whole stream, each
line and the number of events.

Codex's inspected exec events do not carry sufficient executed-model metadata,
so its executed identity remains unknown. Claude's native
`assistant.message.model` is recorded with a source hash, independently of the
requested model. Text containing forged model names or completion claims is
never interpreted as metadata. Executed effort remains unknown on both hosts.

`ClaudeStreamProbe::from_native_stream` passively projects explicitly supplied
native evidence. It requires init and matching observed assistant metadata,
records the native CLI version and content digest, and reads no files or
credentials. Declared tools and `per_turn_effort_active` do not establish actual
capability or supported effort. The observed model is retained, while supported
efforts remain empty and all capabilities remain unknown until separate evidence
establishes them. Such a snapshot cannot authorize an execution effort.

## Shared checks

`cargo test -p adapter-claude -p adapter-codex -p router-adapter-api --locked`
runs shared acceptance and rejection cases for both adapters. Fixtures in
`crates/adapter-claude/tests/fixtures` are synthetic and contain no live provider
response. Cases cover digest replay after changed scope/source/prompt, stale and
unknown evidence, tool subset handling, explicit degradation, input redaction,
instruction trust/hash checks, timeout, cancellation, truncation, malformed
JSON, bounds and forged assistant text. These checks do not invoke a paid model
or prove that a deployment's hooks, permissions or sandbox are active.
