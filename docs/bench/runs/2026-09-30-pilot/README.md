# Pilot of 2026-09-30: router value protocol, host-direct

Manifest `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856`, sealed at 2026-09-30T14:00:01Z (`created_at_unix_seconds` 1790776801).

This folder records the 10-task pilot of the [router value protocol](../../router-value-protocol.md): the sealed inputs, the harness, the base repository and the results exported as metrics only. The procedure and the campaign log are in the [bench playbook](../../playbook.md).

## Contents

| Path | What it is |
| --- | --- |
| `pilot/` | Copy of the pilot folder's inputs and harness, byte-identical except the four files under Redactions: `PLAN.md` (French working plan), `pilot-config.json`, `pricing.json`, `pricing-cache-writes.json`, `pricing-sources.md`, `catalogs/`, `corpus/` (10 tasks with statements, hidden tests and reference solutions, and the base package), `run_pilot.py`, `run_r1.py`, `run-pilot.sh`, `run-pilot-r1.sh`, `fake/fake_host.py` |
| `base-repo/` | Working tree of the base repository each run cloned, without `.git`. Its commit was `051ff869be3fcbb6a673a59f1ba5999f782e0e6e` (`base-repo.HEAD`) |
| `results/` | Metrics exported by `export_results.py`: manifest, ledger, repetition filter journal, arm A route requests and decisions, one reduced JSON per run and per overhead probe, and the calibration report |
| `export_results.py` | Exports the pilot results as metrics only (see "What is not stored") |
| `summarize.py` | Prints the result tables from `results/` |
| `SHA256SUMS` | SHA-256 of every file above except itself, redacted copies included |
| `REDACTIONS` | Original and redacted SHA-256 of each redacted file, and the exact substitution rules |

## Verify

```sh
cd docs/bench/runs/2026-09-30-pilot
shasum -a 256 -c SHA256SUMS
```

Every file under `pilot/` that the manifest seals also matches its hash in `results/manifest.json` (`inputs`). The check is in the playbook.

## Facts of this campaign

| Fact | Value |
| --- | --- |
| Mode | host-direct: `agent-router route` decides arm A, one executor runs the host CLIs for every arm |
| `agent-router` binary | built from `c43d289`, SHA-256 `f789b2d80858aa09bcfb3a0fcce727f4472c5a82bf3c75be8f6a57881bab3c6b` (binary not stored) |
| Host CLIs | `2.1.285 (Claude Code)`, `codex-cli 0.159.2` |
| Host config | `user-config`: the maintainer's own Claude and Codex configuration, unchanged |
| Scope | 10 tasks, arms A, B, G, C, E, 2 providers, repetition 1 of the sealed order (100 of 300 runs), through `run_r1.py` without resealing |
| Budget caps | Claude 65 USD, Codex 18 USD, catalog list prices |
| Exported | first on 2026-09-30T15:38Z while the run was going (28 runs), then again after the run ended (last run at 18:11:16Z): 100 of 100 runs, one attempt set aside after an infrastructure failure (`codex-B-t07-r1.retried`), 8 overhead probes, final report `results/reports/final-report.json` |

## What is not stored

The project does not keep raw provider responses. `export_results.py` therefore leaves out, for every run and probe:

- host streams (`*.stream.jsonl`) and host stderr;
- prompts (`*.prompt.txt`): the execute prompt embeds the model's plan text;
- worktrees (`wt/`), which hold model-written code, and acceptance logs;
- in each phase, the 200-character excerpt of the model's final text (`final_text_head`) and the raw host error events, kept as `error_count`;
- the agent verdict text, normalized to `PASS`, `FAIL` or `OTHER`.

`final_text_sha256` is kept as an integrity reference. The export refuses to write a file containing a home directory path; two raw run results (`codex-C-t04-r1`, `codex-C-t09-r1`) contained one, in fields the export drops.

## Redactions

Four files of `pilot/` differ from the pilot folder, because the repository's strict source export refuses a private home directory path (`private_home_path`, `scripts/package-core.py:29`) and the project does not keep raw provider output. Only two substitutions were applied, nothing else:

- prefix substitution: the maintainer's home directory prefix is replaced by the literal `$PILOT_HOME`. The files are not usable as is: set the local paths again (see the playbook, step 3);
- in `PLAN.md` only, the quoted raw output of the Claude probe is replaced by "la sonde a répondu OK".

| File | Original SHA-256 (as sealed or copied) | Redacted SHA-256 (in `SHA256SUMS`) | Rule |
| --- | --- | --- | --- |
| `pilot/PLAN.md` | `27036f21aa9fd8c7f93529c562a39dc50e956a8c1cf8e979aa4cd7547fc115e1` | `0f22e9658f3214184711560b61677f62e7403fe5a782c91bfd71e2dee9160733` | raw host output quote replaced by "la sonde a répondu OK" (1); home prefix replaced by `$PILOT_HOME` (4 occurrence(s)) |
| `pilot/pilot-config.json` | `7b57463c8af63026b2aa72048694c22b77b513a5c7454cd17c6d9bcc7f46a1fc` | `289e510b0fb252d23bb6a02b10fd6bf6cf049b677e6427241e6e272c77614bfc` | home prefix replaced by `$PILOT_HOME` (3 occurrence(s)) |
| `pilot/run-pilot.sh` | `b5208b12ba0c4cce99966cc6110c54b499057cd55d4ab1da0b772ef79f778aae` | `b401f0e610d9a2bcae1c89c5c5a5477a865458eb3fc2d7add2a48b28e6667be4` | home prefix replaced by `$PILOT_HOME` (3 occurrence(s)) |
| `pilot/run-pilot-r1.sh` | `e747b0f4f4bbd8d826eaea7b0fe94a66ee61298e27ccd4ec809eddf9514d39a5` | `0733ef6b06a447be503fa1f5cd55692817948d52f209dc12e14ab7e45b28e6a5` | home prefix replaced by `$PILOT_HOME` (3 occurrence(s)) |

`REDACTIONS` records the same mapping with the exact rules. Applying them in reverse restores the original bytes: `pilot-config.json`, the only sealed file among the four, then matches its hash in the manifest. The other 60 sealed inputs are byte-identical copies.

## Deviation from the sealed protocol

The base repository has no `.gitignore`. When an agent runs the tests, Python creates `tests/__pycache__/`, and the sealed harness counts it as a write outside `pilot_lib/`, which rejects a run whose hidden test passed. On 2026-09-30, after observing this in the running campaign, a corrected acceptance column was added to the analysis, next to the raw verdict, without changing the run, the harness or the manifest. Rule: hidden test passed, and no remaining out-of-scope path other than those with a `__pycache__` segment (`**/__pycache__/**`). In the final export it changes 8 Codex runs, all with a passed hidden test and `tests/__pycache__/` as their only out-of-scope path: `codex-A-t06-r1`, `codex-A-t07-r1`, `codex-A-t09-r1`, `codex-C-t01-r1`, `codex-C-t02-r1`, `codex-C-t05-r1`, `codex-C-t07-r1`, `codex-G-t04-r1`. No Claude run is concerned. `summarize.py` prints both columns and the list of changed runs; conclusions are stated on both. The fix for the next campaign is in the [playbook](../../playbook.md#next-campaign-harness-fixes).
