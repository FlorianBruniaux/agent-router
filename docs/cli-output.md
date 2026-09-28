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
