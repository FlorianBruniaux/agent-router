# Bench playbook

How to rerun a campaign of the [router value protocol](router-value-protocol.md), check a recorded one, and log its figures. Each campaign lives in `docs/bench/runs/<date>-<name>/` with its sealed inputs, its harness, its results exported as metrics only, and a `SHA256SUMS` file.

## Campaign log

One line per campaign. The commit is the repository commit the `agent-router` binary was built from.

| Date | Commit | Manifest | Scope | Status | Results |
| --- | --- | --- | --- | --- | --- |
| 2026-09-30 | `c43d289` | `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856` | host-direct pilot; 10 tasks, arms A, B, G, C, E, Claude and Codex, repetition 1 (100 runs) | calibration done; full run in progress at export (28 of 100 runs) | [runs/2026-09-30-pilot](runs/2026-09-30-pilot/README.md) |

## Check a recorded campaign

From the campaign folder:

```sh
cd docs/bench/runs/2026-09-30-pilot
shasum -a 256 -c SHA256SUMS
python3 - <<'EOF'
import hashlib, json, pathlib
sha = lambda data: hashlib.sha256(data).hexdigest()
manifest = json.loads(pathlib.Path("results/manifest.json").read_text())
redactions = {e["path"]: e for e in json.loads(pathlib.Path("REDACTIONS").read_text())["files"]}
def original(name):
    data = pathlib.Path("pilot", name).read_bytes()
    entry = redactions.get(f"pilot/{name}")
    if entry is None:
        return data
    for rule in reversed(entry["rules"]):
        source = "".join(rule["from_parts"]) if rule["type"] == "prefix" else rule["from"]
        data = data.replace(rule["to"].encode(), source.encode())
    assert sha(data) == entry["original_sha256"], name
    return data
bad = [name for name, digest in manifest["inputs"].items() if sha(original(name)) != digest]
print(f"{len(manifest['inputs']) - len(bad)}/{len(manifest['inputs'])} sealed inputs verified" if not bad else f"mismatch: {bad}")
EOF
python3 summarize.py
```

The first command checks every stored file. The second checks that the files under `pilot/` are the ones the manifest sealed: unredacted files directly, redacted files after reversing the rules in `REDACTIONS`. `summarize.py` prints the result tables from `results/`.

## Rerun the pilot

A rerun is a new campaign: it gets its own folder, manifest and log line. Changing any sealed input (corpus, prices, catalogs, configuration, `run_pilot.py`) changes the manifest.

1. Copy `pilot/` of the campaign to a working folder outside the repository, for example `~/Sites/perso/agent-router-pilot/`.
2. Build the router at the recorded commit, from a clean export of that commit, and place the binary where `pilot-config.json` expects it (`agent_router`, relative to the working folder):
   ```sh
   git archive c43d289 | tar -x -C <working-folder>/src
   cd <working-folder>/src && cargo build --release --locked -p agent-router-cli
   ```
3. Set the local paths in `pilot-config.json` (`python`, `programs.claude`, `programs.codex`) and in `run-pilot.sh` and `run-pilot-r1.sh` (`PATH`, interpreter). The versioned copies carry the literal `$PILOT_HOME` in place of the maintainer's home directory: the harness does not expand it in JSON, and the scripts stop on an unset variable. Log in to both host CLIs with your own accounts.
4. Check the harness without tokens: `./run-pilot.sh calibrate --fake`, then `./run-pilot.sh run --fake --budget-cap-usd 'claude=1000,codex=1000'`. Results go to `results-fake/`.
5. Check that each host answers, outside the Claude Code sandbox (Codex cannot write `~/.codex` inside it):
   ```sh
   codex exec --model gpt-5.6-luna -c model_reasoning_effort=low --json --ephemeral --skip-git-repo-check "Reply with OK"
   claude -p "Reply with OK" --model claude-haiku-4-5 --output-format json
   ```
6. Seal and calibrate: `./run-pilot.sh calibrate` (add `--isolated` for the variant without user configuration). It runs the auto-check of the corpus, seals `results/manifest.json`, runs 8 overhead probes and the first block of the sealed order, prints the projected cost per provider and stops.
7. Choose the budget caps, then run. `run` refuses to start without them:
   ```sh
   ./run-pilot.sh run --budget-cap-usd 'claude=USD,codex=USD'
   ./run-pilot-r1.sh run --repetitions 1 --budget-cap-usd 'claude=USD,codex=USD'
   ```
   The second form keeps only repetition 1 of the sealed order and records the filter in `results/run-filter.jsonl`. Both resume where they stopped.
8. Export and record: create `docs/bench/runs/<date>-<name>/`, copy the harness and `pilot/` inputs, run `python3 export_results.py <working-folder> --label final`, write `SHA256SUMS`, fill the tables below and the campaign log.

## Results of the 2026-09-30 pilot

Costs are catalog list prices in USD, not invoices. Every acceptance figure appears in two columns, raw and corrected, and every conclusion drawn from this pilot must be stated on both.

### Deviation from the sealed protocol: pycache correction

- Defect: the base repository has no `.gitignore`. An agent that runs the tests makes Python create `tests/__pycache__/`, and the sealed harness (`run_pilot.py`, `acceptance`) counts that directory as a write outside `pilot_lib/`. A run whose hidden test passed is then rejected.
- Decision: taken on 2026-09-30, after observing the results of the running campaign. The run, the harness and the manifest were left unchanged.
- Rule of the corrected column: a run is accepted when its hidden test passed and every remaining out-of-scope path has no path segment equal to `__pycache__`. Only `**/__pycache__/**` and the `__pycache__/` directory entry are ignored; any other out-of-scope path still rejects the run. The rule reads only recorded facts (`acceptance.passed`, `acceptance.touched_outside_scope`), implemented in `runs/2026-09-30-pilot/summarize.py` (`corrected`).
- Runs concerned at export time (28 finished runs): `codex-C-t02-r1`, `codex-A-t07-r1`, `codex-C-t07-r1`, `codex-A-t09-r1`. All four passed their hidden test. `summarize.py` prints the complete list for the final export.
- The raw column stays the protocol's verdict. The corrected column is a post hoc reading of the same runs, labelled as such.

### Calibration block (task t02, repetition 1)

Source: `runs/2026-09-30-pilot/results/runs/*-t02-r1.json`, table printed by `python3 summarize.py --task t02`; the calibration report is `results/reports/calibration-report.json`.

| Provider | Arm | Accepted raw | Accepted corrected | List cost USD | Product cost USD | Input tokens | Cached input | Cache writes | Output | Time to acceptance s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Claude | A | 1 | 1 | 1.0328 | 0.5889 | 607099 | 441213 | 165868 | 3643 | 67.6 |
| Claude | B | 1 | 1 | 1.4871 | 0.8367 | 672109 | 509494 | 162595 | 4217 | 79.5 |
| Claude | G | 1 | 1 | 1.4758 | 0.8248 | 674423 | 511647 | 162756 | 3568 | 77.3 |
| Claude | C | 1 | 1 | 0.7695 | 0.4448 | 603675 | 441272 | 162385 | 3170 | 50.2 |
| Claude | E | 1 | 1 | 0.5287 | 0.3021 | 271551 | 214883 | 56660 | 1621 | 32.0 |
| Codex | A | 1 | 1 | 0.3025 | 0.3025 | 550145 | 457600 | 0 | 3195 | 139.0 |
| Codex | B | 1 | 1 | 0.2716 | 0.2716 | 526778 | 425984 | 0 | 2739 | 236.0 |
| Codex | G | 1 | 1 | 0.2432 | 0.2432 | 418748 | 321536 | 0 | 1663 | 143.4 |
| Codex | C | 0 | 1 | 0.2974 | 0.2974 | 424883 | 329728 | 0 | 3432 | 111.5 |
| Codex | E | 1 | 1 | 0.1087 | 0.1087 | 230275 | 194176 | 0 | 1711 | 113.4 |

Codex C is rejected raw and accepted corrected (pycache). Its list cost per accepted task is undefined raw (0 accepted) and 0.2974 USD corrected. Block total: Claude 5.2940 USD, Codex 1.2235 USD (calibration log). Projection printed by `calibrate` for the 300 sealed runs: Claude 158.82 USD, Codex 36.70 USD. For repetition 1 only, ten times the block: about 53 USD and 12 USD. One task per arm gives no rate: this table checks the pipeline and the cost order of magnitude, nothing more.

### Fixed host overhead

One read-only `Reply with OK` call per model and host config, in a clone of the base repository. Source: `runs/2026-09-30-pilot/results/overhead/*.json`.

| Provider | Model | Host config | Input tokens | Cached input | Cache writes | Output | List USD |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Claude | claude-opus-5-5 | user-config | 66328 | 0 | 66326 | 247 | 0.5356 |
| Claude | claude-opus-5-5 | isolated | 20462 | 12407 | 8053 | 4 | 0.0670 |
| Claude | claude-sonnet-5-5 | user-config | 65167 | 0 | 65165 | 15 | 0.2608 |
| Claude | claude-sonnet-5-5 | isolated | 20483 | 12407 | 8074 | 4 | 0.0348 |
| Codex | gpt-6.1-sol | user-config | 32215 | 7168 | 0 | 5 | 0.0509 |
| Codex | gpt-6.1-sol | isolated | 21235 | 0 | 0 | 5 | 0.0425 |
| Codex | gpt-5.6-terra | user-config | 64579 | 37376 | 0 | 192 | 0.0642 |
| Codex | gpt-5.6-terra | isolated | 41948 | 25088 | 0 | 227 | 0.0415 |

The campaign ran with `user-config`. These probes paid a cold cache write, while a run reuses a warm cache for the same prefix, so multiplying a probe by a run's process count overstates the fixed cost: for the Claude arms of the calibration block the product exceeds the whole run cost. The true share of the fixed prefix is not identified by these data; the number of turns per process is not recorded.

### Full run, repetition 1

Status: in progress. When it ends, export with `python3 export_results.py <pilot-folder> --label final` from the campaign folder, update `SHA256SUMS`, then paste the output of `python3 summarize.py` here and update the campaign log.

| Provider | Arm | Runs | Invalid | Accepted raw | Rate raw | List USD per accepted raw | Accepted corrected | Rate corrected | List USD per accepted corrected | List cost USD | p50 time to acceptance s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Claude | A to E | in progress | | | | | | | | | |
| Codex | A to E | in progress | | | | | | | | | |

Conclusions for this run, written on both columns once it ends: in progress.

Sizing inputs for N (discordance and log cost ratio standard deviation per pair) are in the `sizing` section of the final report, `results/reports/final-report.json`.

## Next campaign: harness fixes

These fixes change sealed inputs, so they need a new campaign folder and a new manifest; the 2026-09-30 manifest stays as recorded.

- Out-of-scope check: either add a `.gitignore` with `__pycache__/` to `corpus/base/`, or make the scope check in `run_pilot.py` ignore paths with a `__pycache__` segment. Either way the corrected column becomes the protocol's verdict, stated before the first run.
- Record the number of turns per host process, so that the share of the fixed host prefix can be estimated instead of bounded.

## Limits of the 2026-09-30 pilot

- Host-direct: the host CLIs run outside the Agent Router runtime. `prepare`, `run --approve`, the native sandbox, `verify`, `artifacts apply` and the feedback store are not exercised; costs are computed by the harness with `price_usage` semantics, not read from `feedback report --pricing`.
- Arm A applies one constant per-phase recipe on every task (frontier model with high effort for plan, cheaper model with medium effort for execute and verify). The pilot compares that recipe with the fixed arms; it does not test a per-task choice.
- 10 small Python tasks in one synthetic repository, standard library only. They are not the protocol's corpus of merged pull requests.
- One repetition: variance between repetitions is not measured, and the sizing inputs mix task variance with run-to-run noise.
- Costs are catalog list prices, not invoices. The runs used the maintainer's subscriptions.
- OpenAI prices were read through a summarizing fetch of the pricing page, verbatim rows in `pilot/pricing-sources.md`; they need a human reread. The Codex credit rate card was not read (HTTP 403), so no credit figure exists.
- The harness counts `tests/__pycache__/` as an out-of-scope write. The corrected column is a deviation decided after observation; see the pycache correction above.
- 10 tasks give a direction and variance estimates to fix N. No adoption claim for H1 or H2 follows from this pilot.
