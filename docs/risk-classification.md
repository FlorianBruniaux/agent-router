# Deterministic risk classification

Risk classification is a conservative lower bound, not permission to execute.
FR-002 and technical specification section 6 require clarification when missing
facts affect security, authority or cost. The classifier uses versioned rules and
keeps the caller's declared risk whenever it is higher than the computed floor.

The existing task and plan documents remain readable. The additional risk
context describes the target environment, reversibility and intended operations.
Those facts must come from the parent or operator responsible for the task. A
model-generated description is untrusted input and cannot authorize an operation.

`router_workflow::normalize_with_risk(TaskInput, Option<&TaskRiskContext>)`
returns `TaskRiskNormalization` schema 2. It wraps the unchanged
`TaskNormalization` schema 1 and a `RiskAssessment` schema 1. The assessment names
the exact ruleset, currently `risk-floor-v1`. Legacy `normalize` and
`validate_task` retain their historical behavior so previously stored v1/v2
documents can still be inspected. New-task callers must explicitly select the
strict entry point and recompute it from the original parent facts.

The context is a separate strict schema 1 document. Unknown fields and unknown
enum values are rejected. The current API has no threshold override or
caller-supplied floor:

```json
{
  "schema_version": 1,
  "environment": "local",
  "reversibility": "reversible",
  "operations": ["workspace_change"]
}
```

Environments are `local`, `test`, `staging`, `production` and `unknown`.
Reversibility is `reversible`, `irreversible` or `unknown`. Operations are
`read_only`, `workspace_change`, `security_change`, `credential_handling`,
`data_migration`, `data_deletion`, `deployment`, `financial_transfer` and
`external_publication`. An empty operation set requires clarification.

The context is an explicit assertion from the caller. It is not a signed source
of authority and its JSON cannot prove that a rollback actually works or that
the target is a test environment. Applications must keep parent context separate
from child output and rerun the classifier when task or context changes.

| Rule group | Floor |
| --- | --- |
| `risk.kind.implement`, workspace changes or required writes | medium |
| Internal data or required network access | medium |
| Security or migration task kind | high |
| Secret data | high |
| Required network access with secret data | critical |
| Security change, credential handling, migration, deletion, deployment or publication operation | high |
| Financial transfer operation | critical |
| Explicit production environment | high |
| An intended effect in production | critical |
| An irreversible effect | high |
| Security, credentials, financial, migration, production, deployment, deletion or publication text/path signal | high |

Rule codes are ordered deterministically and duplicates are collapsed. Text
signals use case-insensitive, whole alphanumeric tokens across the objective,
deliverables, constraints, scope, dependencies and acceptance criteria. The
closed English/French token table lives in `normalize.rs`; total scanned input
is capped at 256 KiB in addition to the existing per-field and list bounds.
Adding a negation does not subtract the associated concern. For example,
`do not DROP production data` prompts for clarification if no parent context
establishes the intended operation and target. Explicit read-only/local parent
facts can clarify that this is a local review, while its high-risk text findings
remain. Language alone is not proof that deletion will or will not occur.

Environment and reversibility must be known for intended effects. A production
signal also requires a known environment. A read-only operation declaration
combined with a structured effect requirement is a context conflict. Lexical
mentions are possible effects, not observed operations: explicit parent facts
can clarify them without reducing the associated floor. A public read-only task
without such signals can be classified without inventing environment facts.

The intended ordering is `low < medium < high < critical`. The classifier takes
the maximum across all applicable rules. A lower declared risk, reassuring text,
negation or a read-only preference cannot subtract a finding. Conflicting or
unknown context produces clarification instead of an executable normalized task.

Rule explanations contain stable codes and risk levels. They do not include
objective text, secret values, path names or arbitrary caller-provided messages.
They are projections of the supplied facts, not proof that those facts are true.
Only the assessment and fixed clarification messages have this redaction
property. The nested normalized task retains its original objective and paths
and must be handled as private task data. Deserialized or rehashed assessments
are never accepted in place of recomputing the rules.

Permissions and budgets remain explicit. Classification never adds a mutation,
capability, budget, credential, network access or approval. The existing workflow,
policy, asset, sandbox and execution gates continue to apply independently.
Write and network operations also produce clarification if the corresponding
mutation permission is explicitly absent. These permissions are necessary
conditions only: workspace-write authority is not authority to delete an
external database, and network authority is not authority to deploy or transfer
money. The classifier does not implement those actions or grant their separate
application permissions.

Text matching is bounded and conservative. It can catch explicit terms for
sensitive changes, but it is not a complete language or intent detector. Absence
of a match does not prove that a task is safe. Authoritative structured context
remains necessary for effects whose target or reversibility is unknown.

The tests use synthetic tasks only. They establish deterministic rules,
monotonic risk restriction, clarification and unchanged authority fields. They
do not establish the safety of a real deployment or the truth of supplied facts.

Run the local contract suite with:

```sh
cargo test -p router-workflow --test risk_classification --locked --offline
```

The CLI `normalize` and `plan` commands apply these rules to the original task
facts on every invocation. Optional `--risk-context <file>` supplies the typed
environment, reversibility and operations. Missing required facts produce
clarification and exit code 3, without creating a plan or granting permissions.
Normalization emits schema 2; new plan envelopes also use schema 2 and include
the assessment. Execution readers preserve historical schema 1 plans and verify
the consistency of schema 2 assessments with the plan's risk and content hash.

For example, explicit parent context for a reversible local edit is:

```json
{"schema_version":1,"environment":"local","reversibility":"reversible","operations":["workspace_change"]}
```

This document does not authorize the edit. Task permissions and later execution
approval remain separate. Applications using the historical Rust `normalize`
function retain its original behavior and must explicitly adopt
`normalize_with_risk` to enforce these rules on new tasks.
