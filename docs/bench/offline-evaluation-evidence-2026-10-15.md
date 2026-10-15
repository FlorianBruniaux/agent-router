# Offline evaluation and accounting evidence, 2026-10-15

This implementation advances [#174](https://github.com/FlorianBruniaux/agent-router/issues/174),
[#120](https://github.com/FlorianBruniaux/agent-router/issues/120) and
[#5](https://github.com/FlorianBruniaux/agent-router/issues/5). All three remain open.
No real trial, provider call, native admission change or measured saving is part
of this work.

## Delivered scope

- [Evaluator qualification](../evaluator-qualification.md): check supplied
  witnesses, exact task bindings and required cases. Unknowns remain explicit;
  aggregate pass counts and cheaper supplied costs cannot hide a lost case.
- [Supplied usage pricing](../supplied-usage-pricing.md): join per-physical-call
  usage to existing revision accounting, retain incomplete totals and export a
  separately allowlisted portable projection.
- [Historical M4 dossier](accepted-evidence-2026-10-15/README.md): preserve all
  three attempts, 156,434 host tokens and the later Router-only acceptance.
  Monetary totals and ratios remain unknown. No physical identity is invented.

The CLI exposes `eval evaluator-qualification` and `eval supplied-usage-pricing`.
Both take one explicit input file and have a `--json` view. Pricing also has
`--portable`. A successful command means a valid report was produced; it does
not mean a candidate was accepted or improved.

## Validation

Source baseline: `a93b0f731915559bef18e40caf2ca8966a3caacb`.
The reviewed implementation manifest binds 19 code, fixture and evidence files;
its SHA-256 is `bf5007fce65fcd1d4bb8cc6164a4fb7406f99ecc07ec6831405cbc61283e4278`.
Documentation was reviewed separately. These hashes identify local reviewed
bytes; they are not provider or evaluator attestations.

Targeted checks passed: 78 `router-eval` tests (17 new), 138 `router-feedback`
tests (13 new), four CLI integration tests and six M4 projection tests. The
historical verifier and existing calculator reproduce the retained projection.
The new Python suite is registered in the local quality runner, including a
regression proving that its failures propagate.

The [verification record](verification-offline-evaluation-2026-10-15.json) retains
commands, log digests, source bindings and the initial environment failure.

| Check | Observed result |
| --- | --- |
| Workspace format | PASS |
| All-targets/all-features Clippy, warnings denied | PASS |
| Workspace tests, all features | 0 failed, 42 ignored; 1,807 passing results summed from 194 summaries, including nested helper output |
| Harness parity and ADR validation | PASS |
| Local quality runner | 30 tests passed with Python 3.14.8 |
| Historical projection | Six tests passed; retained artifacts and repeated calculator output match |
| Whole-worktree Gitleaks | No leaks found |

The sum includes nested helper output and is not a count of unique tests.
The 42 ignored native opt-in tests were not executed. These are local source
checks, not a native qualification campaign or full remote CI equivalence.

Initial local-runner invocations used Python 3.9 and correctly refused their Python
3.11 minimum. The final gate uses the installed Python 3.14.8. No dependency was
installed and no project compatibility rule was relaxed.

## Independent review

A separate Rust reviewer found one blocking case: a qualification witness could
reuse a required case ID under a different output hash. A failing regression
reproduced it, then the implementation was corrected to report an inconclusive
comparison for that overlap. The reviewer approved the corrected offline scope.
The review inspected source and fixtures; test results were supplied by the
implementing agents and parent, not independently executed by the reviewer.

## Remaining proof

- #174: independently established oracle labels, actual evaluator execution
  under recorded limits, tuning case identity and durable history.
- #120: authenticated producer collection and complete native physical-call
  correlation. The portable export does not reconcile cross-authority overlap.
- #5: complete costs through the accepted revision, orchestration and human
  effort, followed by a qualified comparable campaign.

Existing runtime, approval, memory and network requirements are unchanged.
Supplied evidence remains unverified. None of these reports authorizes another
trial or establishes a daily-use benefit.
