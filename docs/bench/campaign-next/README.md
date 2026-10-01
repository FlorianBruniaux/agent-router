# Next campaign runner candidate

Status: **not ready for a real campaign**. This directory is a review candidate, not a recorded campaign. It contains no model outputs, no user host configuration and no new run results.

`pilot/run_pilot.py` is copied from `docs/bench/runs/2026-09-30-pilot/pilot/run_pilot.py`. The candidate adds a `__pycache__/` ignore rule, rejects a cheaper-role model unless its input, cached-input, output and applicable cache-write prices are lower in every class, records completed turns per host process, and counts overhead probes in cumulative spend. The historical pilot directory remains unchanged.

The three tasks in `pilot/corpus/tasks/` exercise cursor-cycle/limit handling, half-open interval merging, and semantic-version prerelease precedence. They are synthetic challenge fixtures. They have not been shown to separate model arms and do not satisfy the protocol's merged-PR provenance rule. A real corpus still needs source commits, frozen parent worktrees, leak review and maintainer arbitration.

## Readiness gates

The copied pricing table and model catalogs are snapshots from the prior pilot, not current price evidence. The Codex pool in that snapshot fails the strict cheaper-role comparison; the Claude cache-read prices also tie. Refresh and human-check every applicable token class, record dated sources, and make the test suite pass before sealing a manifest. Missing, equal, or inverted prices fail closed.

The versioned tree includes `pilot/pilot-config.example.json` with command names and a relative router path, but no machine-local paths or credentials. The runner defaults to this template. `selfcheck` uses only the local corpus and reference solutions and makes no model call. A real campaign still needs a reviewed private configuration, a router binary at the selected commit and a verified host setup.

Real calibration and `run` require `--confirm-spend`, one `--global-budget-cap-usd` for all providers, and a finite `--estimated-remaining-usd`. The current authorization ceiling is 5 USD total across providers and attempts; prior spend must be subtracted from that amount. Prior spend with UNKNOWN cost, missing prices, missing estimates, a cap above 5 USD, or `prior spend + estimate > cap` is rejected. The candidate currently sets `REAL_SPEND_CAP_ENFORCED = False`: its host-direct subprocesses have no verified per-process cost ceiling, so even a supplied estimate cannot prove that the cap will hold. Real calibration and runs therefore remain disabled until that bound is verified and enforced. No provider request was made for this candidate.

Prior spend for the global catalogue budget uses list-priced cost only. If cache-write usage makes that list cost unknown, a product-priced or host-reported amount does not substitute for it; the record blocks real work as UNKNOWN.

An existing `results/base-repo` is reused only when its Git tree is clean and its tracked files match the current `pilot/corpus/base` byte for byte. A divergent or dirty repository is preserved and rejected before run execution; choose a fresh results directory to build another base.

## Local checks

From this directory:

```sh
python3 -m unittest discover -s tests -v
```

The tests verify price-class rejection, per-process turn counts, the fail-closed budget gate, counting probes in total spend, `__pycache__` ignore behavior, and that each hidden fixture fails on the base then passes with its reference solution. They do not establish model performance, current pricing, host readiness, or a hard spend bound.
