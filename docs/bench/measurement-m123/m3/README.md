# M3 offline comparison preparation

This subtree prepares three unresolved tasks and rehearses an 18-assignment
comparison. It does not dispatch a model, start Agent Router native execution,
authorize spending or establish benefit. Model choices, host version, collection
consent, known prior spend, remaining budget and approval of the exact live
manifest belong to M4.

## Frozen task corpus

All tasks start independently at
`6a8de24589d0329c22d35ffc89bcb0bf641aaf94`.
`candidate.json` records exact allowed paths, statements, acceptance and verifier
hashes, and relevant baseline source hashes. It deliberately contains no solution.

| Task | Useful unresolved change | Baseline evidence |
| --- | --- | --- |
| T1 | Include invalid-run costs in the historical pilot report | One accepted run costing 1 USD plus an invalid run costing 2 USD reports 1, expected 3 |
| T2 | Add `--compact` to the manifest-validator CLI | The requested flag is absent; the public CLI criterion fails |
| T3 | Return a `fields` diagnostic for malformed `input_hashes` | The null-map public case returns a generic `json` diagnostic |

`fixtures/baseline.json` records failed public and private criteria on the base.
T1 reproduction constructs a `Pilot` without invoking its initializer and calls
only its report method on fabricated result records. No historical dispatcher
or product runtime is run. T2 and T3 execute the offline Python validator.
These tests must be updated or the corpus reselected when a baseline task is
solved. Current tests intentionally prove the criteria remain unmet.

Run a public criterion against a candidate checkout:

```sh
python3 -B docs/bench/measurement-m123/m3/acceptance.py --repo CANDIDATE_CHECKOUT --task t1
```

Exit 0 means that criterion passed; 1 means it failed. `--hidden` supplies the
separately retained `verifier/hidden_cases.json` to the verifier. The checker
does not enforce write scope or sandbox candidate Python. An independent
reviewer must inspect the diff, then execute the frozen checker in an isolated
candidate environment. Existing owning tests must also pass.

## Protocol and existing contracts

Three arms (`strongfixed`, `cheapfixed`, `router`) each receive three tasks twice.
Aliases retain the calibration vocabulary `E`, `S`, `A-v2`. The live cheap fixed
arm is a declared constant selection rule; M4 must document that hypothesis.
There are six task/repetition blocks of three arms. Blocks and arms are shuffled
with seed `20261010`; the exact order and source/configuration hashes are sealed
in `fixtures/plan.json`. Its first three entries are the M4 first-block shape.

`rehearse.py` reuses `calibration-v2/validate_manifest.py` task, path and hash
checks and the historical `campaign-next/pilot/run_pilot.py` block ordering.
This supplement is **not** a native-external calibration v2 manifest. That
validator's `execution_path` cannot honestly describe Codex delegation without
an explicit contract update. No readiness bit or alias bypasses its gates.
M1 references and M2 accounting references remain declarations until M4 seals
their concrete versions and model/effort choices.

Every real assignment requires a fresh clean checkout of the base and a new
session. No sibling solutions, transcripts or output directories may be shared.
Use identical host/tools/instructions and the same plan/implement/verify steps,
with at most one retry after a known terminal infrastructure failure. Run each
block sequentially. Record observed cache behavior and concurrency; cache coldness
is not assumed. Keep wall time, local-test time and active human time separate.
Actual enforcement of these host constraints remains an M4 check.

`worker_input()` emits only the task ID, source commit, write scope, statement
and common constraints. Its test confirms that verifier inputs are omitted.
The worker must receive only that projection and its independent base checkout,
never this whole subtree. Verifier inputs are in versioned repository storage,
so this proves projection separation only. It does **not** prove OS isolation,
confidentiality or independent review. Those evidence fields remain `UNKNOWN`.

For review, a coordinator assigns opaque review IDs, strips model/effort, arm,
session metadata and costs from the review packet, and supplies only the task,
candidate diff and frozen criteria. Record the verdict and active review time
before revealing the mapping. Retain any unblinding or model self-identification
as a limitation; do not treat redaction alone as verified reviewer blinding.

## Reproduce the rehearsal

Run from the repository root; outputs below belong in a fresh temporary directory:

```sh
python3 -B -m unittest discover -s docs/bench/measurement-m123/m3/tests -v
python3 -B docs/bench/measurement-m123/m3/rehearse.py prepare > PLAN_JSON
python3 -B docs/bench/measurement-m123/m3/rehearse.py fake --state STATE_JSON --limit 3 --scenario docs/bench/measurement-m123/m3/fixtures/scenario.json
python3 -B docs/bench/measurement-m123/m3/rehearse.py report --state STATE_JSON
python3 -B docs/bench/measurement-m123/m3/project_accounting.py --plan PLAN_JSON --state STATE_JSON > ACCOUNTING_JSON
```

Replace the uppercase output placeholders with files in that directory. `--repo`
selects a repository when the helper is staged elsewhere; `M3_REPO` does the same
for tests. No private path is a required dependency. A subsequent `fake` with the
same state and no limit resumes the remaining assignments without replaying
completed ones. The supplied scenario stays sealed in the state.

All 18 assignments are persisted before the first fake attempt. Each attempt is
persisted as pending before the in-process fake result. A known retry retains
both attempts. Pending or ambiguous state stops every resumed attempt. A stale
writer lock also refuses execution. There is no command to clear ambiguity:
preserve the state and reconcile it outside this rehearsal. Never delete it to
make resume pass. Source, helper, plan, state and scenario drift refuse further
work. `report` verifies state checksum and assignment bindings against the current
sealed plan before emitting counts; ambiguous attempts can still be reported.

The committed first-block fixture has 18 assignments, four fabricated attempts,
two fake acceptances, one rejection and 15 unrun assignments. The fake retry is
included. These are protocol fixture counts, not solutions or observed quality.

`project_accounting.py` only maps those fabricated records to the existing
accepted-changes v1 contract. It recomputes the plan digest, requires the full
18-entry Cartesian assignment set, and reuses the rehearsal state validator
to bind every row to the sealed order before declaring the cohort complete.
Its CLI reuses the 1 MiB bounded, no-follow JSON reader, refuses duplicate keys
and emits a static JSON error with exit 2 on invalid input.
It does not calculate costs. Each fake attempt is
assigned 1,000 microUSD solely as an arithmetic test input, with synthetic
physical/revision/reviewer identities. An ambiguous attempt supplies null cost
and unknown coverage. CI and human inventories are complete empty **in this
fixture only**. Nothing here estimates actual provider or human spend.

Invoke the existing built binary, without rebuilding or any model call:

```sh
AGENT_ROUTER_BINARY eval accepted-changes --input ACCOUNTING_JSON --json
```

The expected first-block model total is 4,000 microUSD and the ratio is
4,000 / 2 selected acceptances. The existing Rust calculator owns this result,
including failed-attempt costs, null totals for unknown coverage, and null ratios
when no revision is accepted. Its trust remains `supplied_unverified`.
The parent agent invoked the existing binary on the committed synthetic input:
exit 0, 18 assignments, two selected acceptances. This is offline calculator
integration evidence, not runtime or provider evidence.

## Validation and limits

Observed locally: 19 Python tests pass. Initial rehearsal tests failed because
the helper was absent; the accounting-projection test failed before that module
was added. Public criteria for all three tasks fail on the frozen baseline; hidden
criteria also remain unmet. No candidate solutions, paid calls, native receipt,
global configuration change or Rust rebuild was made.

Tests cover assignment-before-call, all 18 rows, block completeness, independent
synthetic session/workspace IDs, retry retention/ceiling, unrun retention,
no replay after completion, ambiguous/interrupted resume refusal, source and
version drift, symlink refusal, state corruption, concurrent writer refusal,
invalid fake scenarios and accounting joins. The fake IDs do not establish
that real host sessions/worktrees are isolated. Acceptance-checker positive
behavior on genuine solutions remains unverified because solutions are not
implemented in this preparation.

Projection regressions also reject omitted or substituted assignments after
state rehashing, plan payload drift, a shortened resealed plan, duplicate keys,
oversized valid JSON and malformed inputs. They failed before the projection
boundary repair and pass afterward. An ambiguous attempt can still be projected
with null cost and unknown coverage; this does not authorize a retry.

M4 still needs the exact delegation provenance and configuration, real host
constraints and isolation, independent blind review, reconciled prior costs and
an enforceable or explicitly non-guaranteed budget mechanism. Three tasks support
descriptive exploration only. No acceptance rate or cost saving generalizes from
this rehearsal. Preserve all failed/unrun rows and every attempt when running it.
