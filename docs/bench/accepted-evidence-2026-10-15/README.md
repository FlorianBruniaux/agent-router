# M4 v2 through the exact accepted revision

This offline dossier joins the existing M4 v2 participant measurements to the
later explicit acceptance of the Router candidate. It retains all three
assignments and host responses: **156,434 host tokens, one accepted revision**.
The exact rejected Sol artifact remains in the cohort. The identical Luna
output remains pending, with no second acceptance inferred.

The original [result](../measurement-m4-v2-result-2026-10-11/result.json) keeps
its historical pending verdicts. The later
[acceptance receipt](../measurement-m4-v2-result-2026-10-11/integration-acceptance.json)
and [integration note](../m4-fix-integration-2026-10-11.md) record the subsequent
decision. Verification checks that the accepted file bytes in integration
commit `38f2b2d04f84eed3b10e9b29d3f25b36b98abc06` match the tested candidate.
The input files are pinned to baseline
`a93b0f731915559bef18e40caf2ca8966a3caacb`.

## Reproduce without a model call

Run from the repository root with Python 3.10 or newer and a built Agent Router
CLI. No package installation or real model/provider trial is required:

```sh
python3 -B -m unittest discover \
  -s docs/bench/accepted-evidence-2026-10-15 -p 'test_*.py' -v
python3 -B docs/bench/accepted-evidence-2026-10-15/verify.py \
  --cli target/debug/agent-router
```

Use the executable path from the selected Cargo target directory when it differs.
`verify.py` never builds or installs the CLI. It reruns the historical artifact
verifier, derives the projection, calls the **existing** `eval accepted-changes`
calculator twice, checks deterministic output and compares the three committed
JSON files. Add `--write` only to regenerate those files after successful checks.
`project.py` prints the derived dossier and calculator input without writing files.

## Read the three files

| File | Scope |
| --- | --- |
| [dossier.json](dossier.json) | All three host attempts, tokens, exact artifacts, historical and current acceptance, separate prior scope and unknowns |
| [accepted-changes-input.json](accepted-changes-input.json) | Conservative supplied-evidence projection into [ADR 0024](../../adr/0024-accepted-revision-accounting.md) |
| [accepted-changes-report.json](accepted-changes-report.json) | Unmodified output of the existing calculator, with `evidence_trust: supplied_unverified` |

The calculator input has three assignments, two produced file revisions and
historical pending verdicts followed by one later accepted Router verdict.
Sol produced a rejected patch, with no applied file revision. Its null selection
therefore reports `unknown` in the calculator. Its artifact rejection and all
52,570 host tokens remain explicit in the dossier. This distinction preserves
the existing contract rather than inventing a revision for an unapplied patch.

Candidate identities hash the exact returned patch bytes. Revision identities
hash the resulting **single file**, rather than a Git commit or whole tree.
The criteria identity hashes the full sealed manifest, including the no-repair
rule, scope, four public cases, 17 existing tests and final human review.
Task-start identities hash the compact exact assignment objects from that
manifest. Projection sequences are local evidence positions, not observed
native event IDs or a reconstruction of wall-clock order. Verifier digests
hash retained evaluation objects; the reviewer digest hashes the later user
acceptance receipt. These content bindings do not authenticate an evaluator.

## Missing evidence stays unknown

The host response identities do not establish physical provider request
identities. The input consequently supplies **no physical observations** and
marks model, CI and human inventories `unknown`. Three host attempts are still
retained in the dossier; an empty physical-observation array does not mean no
model work occurred. The report's numeric zero known subtotals are empty sums
of supplied monetary observations, never measured zero cost or free work.
Every monetary total and ratio remains null.

Preparation, coordination, publication, integration usage, estimated money,
invoice money, subscription credits, CI cost, end-to-end elapsed time and human
effort remain incomplete or unknown. No human rate or conversion is introduced.
The earlier **279,563 M4 v1 host tokens** remain a separate measured scope;
they are neither included in the v2 participant envelope nor erased. This
development task and its coordination are not measured benchmark work here.

Historical command/test outcomes and the later human decision are retained.
They supply no qualified evaluator or exact required-case comparison under
#174. The efficiency verdict remains **inconclusive**. Router and Luna fixed
used the same model/effort; injected context and cache differed. There is no
selection advantage, financial saving or native-execution claim.

## One bounded proposal

Use this M4 dossier as the first real-data acceptance case for #120's future
qualified producer feeding the existing accepted-revision calculator. Keep the
host-response identity distinct from independently observed physical identity;
add an observation only after its authority, physical identity, usage coverage
and exact assignment/revision join are supported. Missing prices leave money
unknown. This addresses the concrete missing join exposed by these sources
without adding another calculator or a general optimizer.

Acceptance for that next slice: repeated snapshots of one observed physical
request count once; an actual retry counts again; all three existing assignments
remain present; only Router's exact file is accepted; incomplete coordination,
human and CI evidence cannot become complete zero inventories. Qualified #174
evidence and preserved exact required cases are required before efficiency
comparison. No new trial or budget is authorized by this proposal.

Frozen M4 artifacts and seals are unchanged. The separate unpublished
Luna/Astra preparation is not an input to this dossier.
