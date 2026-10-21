# Native score correction and external acceptance

This replay connects one useful **synthetic** correction to its actual native export. The public t02 score function must sum before division, reject more than 64 entries in any component, preserve wide intermediates and return an error outside i64. The original function loses rounding contributions and lacks these bounds. It is an exercise, not a newly discovered Agent Router product defect.

## Observed result, 2026-10-21

On merged main `8265b8ed319febeb3f090f1316e23b8e36647cf5`, the native path completed with six coded responses and five real tool returns. Its public feedback failed before correction and passed afterwards. External checks failed eight of twelve cases on the original file and passed all twelve on the exact reconstructed export. The resulting single-file Git tree is `075ee9fd801052ee57fef3cae6f5fc6b6e80de3a`, mode `100644`.

[Acceptance by required case](acceptance.json), [captured patch](change.patch), [actual submission](submission/lib.rs), [nine negative input results](negative-results.json) and [source-bound qualification](qualification.json) retain the evidence. All 17 binding tests and two CLI checks pass. Workspace formatting, warnings-denied Clippy, 1,920 reported passing test results across 196 suites (including nested results), harness parity and Gitleaks pass; 43 ignored and 498 filtered results are separate.

Initial environment/setup refusals and one verifier defect remain in the qualification record. Git ignored a malformed mode header in an altered diff; the verifier now requires byte equality with the canonical diff reconstructed from the index. Both the valid artifact and all nine altered inputs were replayed after that correction. The unchanged 8 GiB cell recorded no OOM event or remaining live worker. Raw private evidence was archived and verified before disposal.

## Evidence boundary

The existing finite-campaign entry runs the real Linux ARM64 native client, MCP tools, Runtime ownership and namespace cleanup against coded responses. `--useful-score` reuses the already published authored solution. Public feedback runs before and after the write. The external evaluator receives only the stopped run's export, source preimage, captured patch and Runtime report. It checks their bindings, applies that patch to a new private Git index and executes twelve named required cases against the reconstructed file. The resulting tree ID identifies the exact file bytes and Git mode. No working copy or source branch is integrated.

This is separate-process verification of a fixed public fixture, **not an independent author review**, blind evaluation or the frozen campaign's hidden oracle. The public case suite has its own digest; the historical hidden-criteria digest is recorded only as a packet binding. No evaluation result is sent back to the worker. Runtime remains `Verifying`, objective/validation remain `UNKNOWN`, and human acceptance remains null. The result does not close #132 or #174, qualify automatic routing, reconcile real usage or prove savings.

## Required cases

[The executable cases](required-cases.rs) cover empty inputs, summing before division, negative quality, each of the three component work limits, exactly 64 entries, sums wider than u64, negative and positive i64 overflow, cancellation before the final range check, and both unchanged i64 extrema. Expectations use literal arithmetic outcomes from the public statement. Every required case must pass; a missing case or successful process alone is insufficient.

[The external verifier](verify.py) accepts only the already published authored t02 replacement. It is not a general runner for arbitrary candidate code. It requires Linux cgroup memory exactly 8,589,934,592 bytes and swap zero, retains the 8 GiB address-space limit in subprocesses, and uses bounded inputs, CPU/time/file-size limits, private temporary files and disabled Git global configuration. Run it only in the same disposable, network-disabled cell with public inputs and no account/private mounts. An infrastructure failure produces no passing acceptance receipt. Existing output is never overwritten.

## Replay

First prepare the unchanged Linux image, source, pins and namespace recipe described in the [F12a report](../linux-f12a-main-2026-10-21/README.md). The parent directory `/task/campaign` must exist and each assignment root must be absent. Do not reuse an interrupted assignment or a real campaign ledger.

```sh
python3 -B docs/bench/finite-campaign-launcher-2026-10-18/qualify-offline.py t02-Lm-r1-H1 --useful-score --output /private/prepared-cell --recipe /private/namespace-recipe.json
```

Copy only that run's `source-lib.rs`, `lib.rs`, `change.patch` and `runtime-result.json` into a new directory of the isolated cell. Copy this verifier and its cases alongside the unchanged repository inputs, then run inside Linux:

```sh
python3 -B docs/bench/native-useful-score-2026-10-21/verify.py --input /task/acceptance-input --output /task/acceptance.json
```

No provider/account/registry access is needed. The result binds source, candidate, patch, report, criteria, verifier and reconstructed tree. The raw report and protocol logs remain private; publish only their hashes and the bounded acceptance receipt.

The offline binding regressions and command-line refusal checks run without Docker or a model:

```sh
python3 -B docs/bench/native-useful-score-2026-10-21/test_acceptance.py
python3 -B docs/bench/finite-campaign-launcher-2026-10-18/test-useful-score.py
```

The Linux negative replay changes the candidate, source, patch, cleanup, admission, packet identity, executable mode, diff header and file scope. Every altered input must be refused without producing a passing receipt:

```sh
python3 -B docs/bench/native-useful-score-2026-10-21/test-linux-inputs.py --input /task/acceptance-input --output /task/new-negative-cases
```
