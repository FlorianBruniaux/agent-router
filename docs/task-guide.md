# Guided task intake

`task-guide` collects explicit task facts and uses the same risk normalizer as
`normalize` and `plan`. It creates no execution approval and launches no host.
A description supplies only the objective; the operator supplies the task kind,
risk, data sensitivity, budgets, permitted mutations, acceptance and scope.

```sh
agent-router task-guide --text 'Review the parser' --json
agent-router task-guide --text 'Review the parser' --interactive \
  --output-directory ./parser-review --json
```

Supply exactly one input: `--text`, `--text-file` containing UTF-8 text, or
`--task` containing the existing strict `TaskInput` JSON. `--risk-context` accepts
an existing `TaskRiskContext` JSON document. Unknown fields and duplicate JSON
keys are errors. Explicit facts in a supplied task are preserved.

Without `--interactive`, the command never reads stdin. It returns the missing
questions with exit code 3, or the complete normalization with exit code 0.
`--interactive` sends prompts to stderr and leaves stdout for the final result.
Enum answers use the displayed snake_case names; lists and budgets use JSON.
A budget specifies integer micro-USD, milliseconds and tokens. Zero cost is
allowed; elapsed time and token limits must be positive. The displayed budget
example is not a default and is never applied without an answer.

An empty answer, EOF or the literal `unknown` leaves the task unresolved.
`cancel` returns a cancelled report. Both exit with code 3 and create no output
directory. Invalid answers exit with code 2 and do not echo their contents.
Write or network conflicts stop the dialogue rather than offer a new grant.
Revise the task's explicit facts separately to resolve those conflicts.

Operations, target environment and reversibility are collected when the risk
normalizer requires them. Context can raise the effective risk, but cannot grant
permissions. For example, a declared low-risk deletion can retain `low` in the
original input while normalization records a higher risk floor. Answers are
operator statements, not authenticated host capabilities or verified facts.

## Result and export

The version 1 report contains `status`, `objective`, `input`, `risk_context`,
`questions`, `result` and `output_directory`. Status is `complete`,
`needs_clarification` or `cancelled`. `result` holds the existing version 2
risk-normalization projection. For text awaiting a task kind, `input` and
`result` are null. Omitting `--json` renders the same report for humans.

`--output-directory` writes only after all questions have been resolved and the
normalized task passes validation. On Unix, it creates a new directory with mode
0700 and new files with mode 0600:

| File | Content |
| --- | --- |
| `input.json` | Original explicit task facts, including the declared risk |
| `risk-context.json` | Explicit context, only if supplied or collected |
| `normalization.json` | Completed normalization and risk assessment |

Each complete, synced file is published without replacement. `normalization.json`
is published last as the completion marker. A crash or filesystem error can
leave a private partial directory; directory publication is not transactional.
Treat a directory without that marker as incomplete. The command never reuses,
overwrites or recursively removes an existing destination, including symlinks.
Choose a new output directory after a failed publication.

The parent must exist and must not be writable by group or others. Its path is
resolved once and its identity is checked during publication. The export assumes
that processes running as the same OS user do not maliciously replace ancestor
paths while it runs; these task files are not an execution trust boundary.
Unix mode checks do not verify filesystem ACLs. Export on non-Unix platforms
returns `task_guide.output.unsupported`; stdout intake still works.

```sh
agent-router normalize --task ./parser-review/input.json --json
agent-router plan --task ./parser-review/input.json --json
```

If the directory contains `risk-context.json`, pass it explicitly to both
commands with `--risk-context ./parser-review/risk-context.json`. Neither command
discovers sibling context automatically. Review the resulting plan before the
separate preparation and execution approval steps.

## Bounds and unresolved evidence

Objective text is limited to 16 KiB, structured task input to 1 MiB, and supplied
risk context to 64 KiB. The dialogue accepts at most 32 answers, each at most
64 KiB including its newline, with at most 1 MiB total input. The existing
normalizer retains its 64-item list, field, relative-path and risk-scan bounds.
Exported documents are each limited to 1 MiB. Unexpected question codes fail
closed with `task_guide.question.unsupported`.

This command does not infer task fields with a model, verify budget accounting,
change parent-conversation models, or establish native permissions. The
existing configuration, host evidence and execution gates remain necessary.
