# Human and structured CLI output

Product commands display a labeled human view by default. Add `--json` for the
complete versioned result used by scripts. Both views come from the same command
result and preserve its exit status, decisions and approval requirements.
Changing output format never approves a run or applies an artifact.

```sh
agent-router normalize --task task.json --risk-context risk-context.json
agent-router plan --task task.json --risk-context risk-context.json --json
agent-router run --definition definition.json --store runtime --sandbox sandbox.json
agent-router status --store runtime --run-id example
agent-router doctor --request diagnostics.json
```

`normalize` and `plan` use the [versioned risk rules](risk-classification.md).
They ask for missing authority or risk facts and return exit code 3 when a task
cannot yet be planned. They do not infer a budget or permission from task text.
New normalization and plan envelopes use schema 2; historical workflow plans
remain readable. An assessment is an explanation, not an approval.

Human output preserves full receipt and approval identifiers. `UNKNOWN` stays
unknown; JSON null is displayed as `not set (null)`, never as false or zero.
Control characters and bidirectional control marks in external text are escaped
so they cannot issue terminal commands or conceal adjacent fields.

Large results are explicitly shortened at the display limit. This changes only
the view, never an operation's success or failure. Use `--json` when collecting
a whole index or audit for another program. For commands that write files, the
published artifact is independent of the display limit; changing formats does
not undo or repeat that write.

`config --layers` accepts strict bounded JSON, TOML and YAML; see
[configuration layer formats](configuration-formats.md) for the supported schema
and refusal rules. Task input and other product manifests retain their JSON
contracts. An interactive task questionnaire is not implemented by the output
renderer.

`escalation bound` displays an accounting preview; only its explicit approval
persists a parent-attested ceiling. `escalation propose` displays an inert,
source-pinned proposal, and ordinary `prepare` / `run` perform the separate run
review and approval. See [durable escalation and CLI boundaries](escalation.md).
The run view distinguishes `decision_origin`, `escalation_decision` and the
unchanged `baseline_receipt_id`; changing the output format adds no authority.

## Exit codes and error codes

A failed command writes one line, `error: <code>`, to stderr. The code is a
stable dotted identifier such as `runtime.approval.required_or_changed`. Its
exit status comes from one table in the CLI (`exit_class` in
`crates/agent-router-cli/src/main.rs`), keyed on the code alone, so every
command shares the same classes.

| Status | Meaning | What to do | Codes |
| --- | --- | --- | --- |
| `0` | Success. | Nothing. | none |
| `2` | Invalid input or refusal, including user input that drifted after approval. | Change the input or obtain the missing authority. | every code not listed below, including `*.changed`, `*.io` and `*.storage_io` |
| `3` | Valid request, outcome not final (clarification, abstention, `no_route`, pending dependencies, failed or unverified run). | Read the JSON result. | none: a command result, not an error line |
| `4` | Internal failure, including unreadable host output. | Report it; retrying the same input is not expected to help. | `route.internal`, `decision.render`, `*.serialization`, `*.json`, `*.storage`, `*.storage_database` |
| `5` | Transient contention. | Relaunch the same command unchanged. | `*.busy`, `*.locked`, `*.storage_locked` |
| `6` | Outcome unknown. | Inspect the state before any retry; the effect may already exist. | `backend_store.published_durability_unknown`, `runtime.cleanup.unknown` |
| `7` | Integrity failure: a stored record, a hash chain or a signature no longer matches. | Treat the store or sealed input as altered; do not retry blindly. | `*.integrity`, `*.storage_corrupt`, `policy.store.invalid`, `feedback.store.invalid`, `feedback.policy.signature`, `assets.review.trust_mismatch`, `backend_store.anchor` |
| `130` | `serve` forced to stop by a second signal. | Restart; the next start replaces the stale socket. | none: a command result |

`*` matches the namespace, compared on the last dotted segment: `runtime.busy`
and `backend_store.busy` are both `5`, while `snapshot.json.invalid` is `2`.
Outcome-unknown codes and integrity codes without the `integrity` suffix are
explicit lists; other codes ending in `unknown` (for example
`task.budget.unknown`) are refusals.

The line between `7` and `2`: `7` means something the product stored or sealed
no longer matches, a stored record, a hash chain or a signature. `*.integrity`
also covers a sealed input (definition, bundled request) edited after its
approval, because the library detects it as a mismatch with the stored run.
`2` covers every refusal, including user input that drifted after approval and
is reported as `*.changed` (for example `policy.active.changed`). A changed
input that only alters the recomputed digest is
`runtime.approval.required_or_changed`, also `2`. `backend_store.conflict` is a
refused concurrent update, `2`.

`*.io` and `*.storage_io` stay `2` because a missing or unreadable caller path
reaches them. `4` also covers `runtime.json` raised by a host stdout line that
is not valid JSON: the output could not be read, which says nothing about the
caller's input. SQLite `SQLITE_BUSY` and `SQLITE_LOCKED` on the runtime and
coordinator stores map to `runtime.busy` and `coordinator.busy` (`5`).
`SQLITE_CORRUPT` and `SQLITE_NOTADB` on the runtime, coordinator and feedback
stores map to `runtime.storage_corrupt`, `coordinator.storage_corrupt` and
`feedback.storage_corrupt` (`7`). A unit test scans the CLI sources and fails if
an emitted code containing `integrity`, `corrupt`, `busy`, `locked` or
`durability` falls outside its class.

Breaking change: before this table, every product error exited `2`, including
contention, unknown outcomes and integrity failures; only `route.internal` and
`decision.render` exited `4`. Scripts that treated any `2` as "fix the input"
now receive `5`, `6` or `7` for those cases. `policy apply` with an empty or
oversized trusted-key set now reports `feedback.invalid` (`2`) instead of
`feedback.policy.signature`, which is reserved for a signature mismatch (`7`).

