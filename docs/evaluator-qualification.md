# Offline evaluator qualification

`router-eval::evaluator_qualification_report` checks the internal consistency of
supplied evaluator evidence and compares required cases for each declared task
instance. It runs no evaluator, model, provider or runtime process. Its report
always records `oracle_authenticity: "supplied_unverified"` and
`adoption_authorized: false`.

The CLI reads one bounded input and emits the same report:

```sh
agent-router eval evaluator-qualification --input tests/fixtures/evaluation/evaluator-qualification/rare-case-permutation.json --json
```

CLI success means a valid report was computed, including a report whose status
is `regression` or `inconclusive`. It does not mean adoption or acceptance.
Malformed, excessive or inconsistent input exits with an error.

The [synthetic rare-case fixture](../tests/fixtures/evaluation/evaluator-qualification/rare-case-permutation.json)
has one passed case in each revision. The candidate costs 1 supplied microusd
versus 100 for the reference, but loses the mandatory `rare` case. Its comparison
is `regression`. These numbers test a contract, not measured model performance.

## Existing checks and remaining gap

| Criterion | Existing implementation and local check | Added coverage |
| --- | --- | --- |
| Three-state observed outcomes | `crates/router-eval/src/replay.rs`, `ObservedOutcome`; `crates/router-eval/tests/replay.rs` | Per-required-case outcomes; infrastructure failure maps to unknown |
| Pair identity, held-out families, supplied review and metric regression | `crates/router-eval/src/promotion.rs`, `validate_pairs`, `compare_shadow`; replay tests | Explicit task-instance, input, source, criteria, evaluator and cohort bindings; rare-case permutation |
| Accepted revision and historical cost frontier | `crates/router-feedback/src/accepted_changes.rs`; `crates/router-feedback/tests/accepted_changes.rs` | No replacement: this report neither accepts revisions nor allocates run costs |
| Known defective M3 baseline | `docs/bench/measurement-m123/m3/acceptance.py`; `tests/test_candidates.py`; `scripts/check-m3-baseline.py` | Retains case identity instead of only `cases`/`passed_cases`/`passed` |
| M4 v2 local acceptance and sealed artifacts | `docs/bench/measurement-m4-v2/acceptance.py`; `docs/bench/measurement-m4-v2-result-2026-10-11/verify.py` | Explicit qualification witnesses and supplied execution limits; no historical M4 verdict is rewritten |

M3/M4 scripts are inventoried, not executed or imported by this library. Their
historical results do not establish independently authenticated evaluator labels.

## Versioned contract (1 and 2)

The input is strict JSON. Unknown fields, duplicate object keys, trailing data,
invalid enum values, duplicate identities and unsupported schema versions fail
with static errors. Errors never include raw input, parser details or a supplied
identifier. Limits are 4 MiB of JSON, 256 instances, 64 evaluator qualifications,
4,096 verdicts and 4,096 total nested rows. Identifiers are 1 to 128 ASCII letters,
digits, dots, underscores or hyphens. SHA-256 values use 64 lowercase hex digits.

- `cohort` fixes task instance, intention (`design`, `implementation`,
  `validation`), source/input/criteria digests, required case IDs and candidate
  authors. `cohort_sha256` seals its complete ordered contents.
- `qualifications` binds evaluator ID/version, executable/configuration digests,
  applicable criteria, supplied duration/output limits, and a sealed corpus.
  Structural consistency requires correct-output, known-defect and
  infrastructure-failure witnesses with outcomes passed, failed and unknown,
  respectively. Reviewer IDs must differ from all supplied candidate authors.
  Reviewer IDs and oracle evidence digests remain unauthenticated claims.
- `verdicts` retains each named version, exact task/cohort binding, evaluator,
  revision and stored-output digests, and individual case outcomes. An
  `infrastructure_failure` execution makes its effective outcome unknown, even
  if the supplied outcome says passed or failed.
- `comparisons` explicitly selects reference/candidate verdict IDs per instance.
  Missing selections, verdicts or required cases produce `inconclusive`.
  Unknown or changed evaluator identity, inapplicable criteria, changed bindings,
  extra cases, unknown outcomes, qualification case-ID overlap, and
  qualification/tuning output overlap also
  prevent a conclusive comparison. Version 2 additionally requires tuning case
  identities; their overlap with required cases is inconclusive.
- `permitted_differences` may explicitly allow `revision`, `output` and `cost`.
  It cannot allow changed inputs, criteria, cohort or evaluator identity.

Use `evaluator_task_contract_sha256`, `evaluator_cohort_sha256` and
`evaluator_qualification_corpus_sha256` to seal typed contents. V1 hashes are
SHA-256 of a domain string including its final NUL byte followed by compact
Serde JSON. Domains are `agent-router:evaluator-task:v1`,
`agent-router:evaluator-cohort:v1` and `agent-router:evaluator-corpus:v1`.
Vector ordering and struct field ordering are part of the seal. A digest checks
supplied content consistency; it does not authenticate the underlying artifact.

## Comparison and history

A conclusive structural comparison is `required_cases_satisfied` only when every
candidate case passes and every reference outcome is known. Any reference pass
lost to a candidate failure is `regression`. Other known candidate failures yield
`required_cases_failed`. Missing or incomparable evidence yields `inconclusive`;
case observations remain visible without becoming a qualified claim.

Qualification witness case IDs cannot match any required case ID of a compared
task. Qualification and tuning output digests are also excluded from held-out
comparison outputs across all supplied evaluator versions, including versions
not selected for either verdict.

Version 2 requires `tuning_case_ids` in every qualification. It is a unique,
bounded set using the same identifier syntax as required cases. Declare `[]`
when no tuning is supplied. Both `tuning_case_ids` and `tuning_output_sha256`
must be empty together or nonempty together. They are separate sets, not paired
rows: one case may have several tuning outputs. Every ID counts toward the
4,096 nested-row limit. Any tuning case ID reused by a decision case yields
`decision_corpus_overlap`, even when its output hash differs.

Version 1 remains readable with `tuning_case_ids` absent or null; a non-null set
is rejected. A nonempty legacy `tuning_output_sha256` now makes every comparison
`inconclusive` with `tuning_case_identity_unknown`. Existing v1 evidence declaring
no tuning keeps its prior comparison semantics. To migrate, set `schema_version`
to `2` and supply the complete known tuning case set for every qualification.
Do not invent IDs or clear output hashes to remove a blocker. Missing/null IDs
in v2, duplicate IDs and inconsistent empty sets are malformed input.

The report echoes the input schema version. Task, cohort and witness seals keep
their v1 domains because their sealed structures are unchanged. Neither version
seals or authenticates the tuning inventory. These checks establish only exact
supplied identity separation, not semantic independence, inventory completeness
or whether someone secretly tuned on a held-out case. No boolean can upgrade
oracle authenticity.

A stored-output reevaluation names an earlier `verdict_id` through `reevaluates`.
Its instance, task, cohort, revision and output must remain identical. The report
retains both old and new verdicts in `verdict_history`. It never rewrites a
previous verdict, adds a run or infers a cost. The caller must preserve history
between invocations; this stateless checker cannot detect omitted historical
records that were never supplied.

Optional costs belong to the explicit comparison selection. Report totals are
checked sums of supplied values, with missing values remaining null and overflow
rejected. They are not prices, reconciled full-run costs, savings or adoption
criteria. Exact accepted-revision accounting remains in
[accepted changes](accepted-changes.md). Independently proven authority
violations and runtime refusals remain with their existing validating components;
this input accepts no authority attestation and grants no permission.

## Local evidence and uncovered criteria

Run the contract checks without network access:

```sh
cargo test -p router-eval --test evaluator_qualification --locked --offline
```

The suite covers rare-case permutation, unknown/infrastructure/missing outcomes,
duplicate identities, version/size/row limits, redacted errors, exact seals,
evaluator applicability and identity, permitted differences, source/input/criteria
changes, qualification/tuning separation across evaluator versions, legacy
unknown tuning identity, v2 migration and row limits, history preservation and
cost overflow. The CLI suite checks the v2 inconclusive result without creating
state.
The fixture is synthetic. Passing it establishes these offline rules only.

Still uncovered by this foundation: independently established and authenticated
correct-output/defect labels, execution of an evaluator under its recorded
limits, semantic hold-out separation, verified correspondence between a digest
and external executable/output, persistent append-only history, authentic
independent review, and measured quality or cost benefit. Issue #174 therefore
remains partially implemented; no real trial is authorized by this report.

The [2026-10-21 tuning identity verification](bench/evaluator-tuning-identity-2026-10-21.md)
records the tested sources, local checks and remaining #174/#132 criteria.

The [2026-10-21 frozen t02 oracle probe](bench/t02-oracle-qualification-2026-10-21/README.md)
executes independently labelled controls in offline Linux at unchanged 8 GiB.
It exposes a missed unsigned-sum defect: the retained oracle is unqualified.
This finite diagnostic preserves historical verdicts and does not authenticate
the oracle, establish held-out adequacy or authorize runtime acceptance.
