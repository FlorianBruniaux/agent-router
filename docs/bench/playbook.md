# Bench playbook

The [M4 stop report](measurement-m4-result-2026-10-10/README.md) records 279,563 host tokens consumed by coordination, with none of the three worker assignments started. The [M4 v2 proposal](measurement-m4-v2/README.md) replaces the extra model coordinator with an offline admission ledger and three single-response patch attempts, then local tests and human review. Its proposed 300,000-token envelope awaits a new approval. No v2 participant has been launched. The [v1 seal](measurement-m4/APPROVAL.md) remains historical evidence.

The [M1–M3 measurement preparation](measurement-m123/README.md) records the 10 October continuation after #157: a campaign catalog, offline trace accounting and a simulated comparison rehearsal. Paid trials and measured savings remain separate acceptance gates.

The [current finalization plan](finalization-plan-2026-10-07.md) (FR), 2026-10-07, orders the useful trial, measurement and release work. It separates completed local checks from the remaining trial and admission criteria.

How to rerun a campaign of the [router value protocol](router-value-protocol.md), check a recorded one, and log its figures. Each campaign lives in `docs/bench/runs/<date>-<name>/` with its sealed inputs, its harness, its results exported as metrics only, and a `SHA256SUMS` file.

## Current delivery status, 8 October 2026

The [dedicated-client qualification](owned-entry-qualification-2026-10-08/README.md)
records three final-source native scenarios against a simulated provider, strict
binding and cleanup, and retained initial failures. [ADR 0026](../adr/0026-owned-client-simulated-entry.md)
selects this entry only for F02a simulated patch work. ACP component comparison is
partial; real-provider and complete OS admission remain unqualified. The passive
observation now has a JSON Schema. The [verification report](verification-owned-client-2026-10-08.json)
records local gates and independent reviews. [PR #144](https://github.com/FlorianBruniaux/agent-router/pull/144)
subsequently added a useful simulated correction, verified on its published Git
revision with user files preserved. [ADR 0027](../adr/0027-verified-patch-publication.md)
and the [CLI example](../cli-execution.md) define this separate local proof. The
competing PR #145 is not the retained API; its applicable protections are carried
onto the merged implementation. Native composition and measured savings remain open.

## Inspect durable attempts with observation v2

Use `agent-router observe --store ROOT --run-id ID --schema-version 2 [--json]` with an existing authorised store/run, replacing the format placeholders. Default v1 is unchanged. The [observation guide](../run-observation.md#read-v2-attempts) maps each attempt field to its evidence: requested selection, supervisor termination/exit code and source hashes. Starts without an outcome remain visible; incomplete capture is explicit. Legacy protocol markers and native usage/timing/objective stay unknown.

The reconstructed candidate passed 80 targeted tests, including 21 independent hostile scenarios, with 4 native opt-in tests ignored. Its independent review corrected two overbroad ADR claims. These counts do not establish full CI or real-provider execution. The remaining measurement work is qualified native provenance, trusted timing (#117), external correlation (#120) and complete collection for #5. This slice does not close #116 or #118.

The following paragraphs retain the 7 October baseline and its original limits.

## Delivery baseline, 7 October 2026

The [broker-outage cancellation follow-up](physical-send-feasibility-2026-10-07/outage-cancellation/README.md) closes the broker listener before native launch. A correlated retry notification triggers one interrupt; its acknowledgement and terminal `interrupted` match the thread/turn. Physical process, output, handler and socket cleanup are checked separately, with zero provider requests or connections. The initial v2 and global `NO_GO` remain unchanged. Broker loss during an active session is untested; no production or financial admission follows.

The local [F04a observation implementation](../run-observation.md) is tested and independently reviewed. It reads one coherent, redacted snapshot of an existing store and shows at most 20 human state/sequence references, not a complete native timeline. Twelve runtime and three CLI focused tests passed; 19 independent probes found no confirmed blocker. The [source-bound verification record](verification-observation-2026-10-07.json) records all 21 selected quality checks passed: 1,633 successful workspace Rust results plus one core check, and 290 Python tests. Rust totals sum result lines, including subprocess invocations, rather than unique definitions; the focused tests are included. The 42 ignored opt-in native tests are not successes, and this selection is not full CI equivalence. F04b and complete F05a remain open.

The [F03a diagnostic](physical-send-feasibility-2026-10-07/README.md) retains an overall NO-GO: six of seven native cases pass, but absent broker means zero HTTP requests and a 15-second timeout, with native terminal behavior UNKNOWN. The corrected verdict classifier passed 12 tests and four independent probes; 84 verdict mutations were checked. These results do not admit real provider execution or close #131/#132/#133.

The six issue bodies #5, #11, #131, #132, #133 and #136 were reconciled and read back on 2026-10-07; 17 issues remain open. The [personal trial candidate](calibration-v2/personal-trial.md) now names a concrete useful task for preparation; it has not been implemented or executed by this pass.

[Exact-revision accounting](../accepted-changes.md) now has an offline CLI, synthetic example and independent review. The [local verification record](verification-owned-entry-2026-10-07.json) binds the selected quality checks to source hashes. Supplied costs are not collected provider usage or observed savings.

The [bounded build investigation](reproducibility-investigation-2026-10-07.md) records four matching native builds from the historical f378 export; it neither reproduces the original mismatch nor qualifies the current product.

[Owned-entry decision, 2026-10-07](owned-entry-2026-10-07/README.md) records
an early NO-GO for both unmodified app-server and ACP transports under the
required physical-send admission contract. The [local broker/send diagnostic](physical-send-feasibility-2026-10-07/README.md)
(#133/F03a) retains its original overall NO-GO. Its separate controlled-cancellation
follow-up now permits F01r to start on a pinned dedicated app-server client,
HTTP broker and controlled interruption, without waiting for #132/F03b.
First qualify task/thread/turn/model/effort binding and refusal of an unadmitted
choice before provider contact. A positive entry decision precedes the useful
#132 path and final F03b composition; it is not inferred from the local probe.
The remaining measurement frontier is F04b, F05a/F05b and F06a.
The behavioral comparison remains incomplete. No automatic route is activated.

[Homebrew and Cargo qualification, 2026-10-07](distribution-qualification-2026-10-07.md)
records both local installation lifecycles, 1,594 passing Rust test executions
(42 ignored), 70 packaging/release Python tests and a matching diagnostic build
pair. The first build mismatch remains unexplained in
[issue #136](https://github.com/FlorianBruniaux/agent-router/issues/136).
Public distribution and distinct-version upgrades remain unqualified.
The [earlier core-only trial](release-qualification-2026-10-07.md) retains its
original results and failed Homebrew experiment. These delivery checks are
separate from the value campaigns below and establish no productivity savings.


The [current status and next acceptance criteria](status-and-action-plan-2026-10-05.md)
are maintained with issues [#5](https://github.com/FlorianBruniaux/agent-router/issues/5)
and [#11](https://github.com/FlorianBruniaux/agent-router/issues/11) after each
validated delivery or new blocker. Campaign results also get a log entry below;
sealed results and earlier failed checks are preserved.

PRs [#125](https://github.com/FlorianBruniaux/agent-router/pull/125) and
[#126](https://github.com/FlorianBruniaux/agent-router/pull/126) are merged through
`8983d23a2e8937e244cf64ca0f2ba422073b20ae`. The 11 checks on PR #126's final head
`2f50bcb5c069b12e35c45842c69eb6ae43d23da2` passed before merge, including
Linux/macOS workspace tests and the 42 Python fixture tests (29 + 13).
The four post-merge workflows did not execute any steps: GitHub reported a
payment or Actions spending-limit problem
([post-merge run](https://github.com/FlorianBruniaux/agent-router/actions/runs/37416315096)).
GitHub post-merge validation remains unverified. Development checks now run
locally using the [local verification guide](../local-ci.md); no billing change
was made. This does not supply the missing Linux evidence.

[Local verification without Actions, 2026-10-06](verification-local-ci-2026-10-06.json)
records 1,575 workspace Rust results, 1 core-only test, all 42 ignored tests and
191 Python tests passed across the original run and corrected tooling rerun.
The first run's zero-test Python discovery failure remains recorded; the fix
executes each hyphenated test file directly. Rust sources did not change.
Fresh RustSec audit, both secret scans, strict core packaging and verification,
and three isolated parser fuzz smokes also passed. Three Linux-specific
behaviors remain unverified. This record captures validation before merging
[PR #127](https://github.com/FlorianBruniaux/agent-router/pull/127); the PR shows
its current merge status.

[Native routing and measurement preparation, 2026-10-06](verification-native-measurement-2026-10-06.json)
records 1,636 Rust results and 257 Python tests passed across the full run and
one test-only repair. The full run's Git-fixture cleanup failure remains recorded;
the repaired 28-test suite passes, with no intervening Rust changes. Formatting,
both Clippy modes, native tests, fresh dependency audit, secret scans, strict core
packaging and three isolated fuzz smokes pass. Three Linux checks remain unverified.

The [U02a roundtrip](native-integration-u02/README-v5.md),
[U02b native hook observations](native-integration-u02/README-u02b.md) and
[U06 explicit decision handoff](native-integration-u06/README.md) now have separate
records. U06 observes the real CLI-selected `gpt-5.6-terra / medium` pair in the
separate Codex session's request. A fresh disabled session retains its baseline.
The provider is scripted; only the selected execute stage runs, without selected
skills. These diagnostics do not constitute a value campaign or AR runtime receipts.

[U07 failure observations](native-integration-u07/observations.md) now record
8 fresh sessions on Codex 0.159.2. Five unexpected hook failures each allow one
parent request; valid block and exit 2 with a reason allow none. Native UI
disabling is effective in a fresh session. Every request is rejected locally,
with no child dispatch or inference. Child dispatch through this hook is not
admitted under the single-effective-dispatch requirement.

Next: qualify an entry point that owns submission without an already running
parent turn; seal the [personal trial](calibration-v2/personal-trial.md) with an
approved cap and admitted provider path, then run calibration followed by a
held-out comparison. CLI, desktop and Claude
remain separate surfaces. No automatic daily routing or user benefit is claimed.

## Earlier preparation and local evidence

Next preparation, 2026-10-05: [calibration v2 candidate](calibration-v2/README.md)
(French), a descriptive collection protocol with an incomplete manifest template.
It does not authorize a run or amend the recorded pilot results.

Local integration evidence, 2026-10-05: [native Codex diagnostic](native-integration-u02/README.md).
Parameter transmission and output correlation passed against a scripted provider;
the canary absent from the direct tools projection keeps that historical probe
red. The [later diagnostic](native-integration-u02/canary-diagnosis/README.md)
does not establish absence from the whole request. Automatic CLI integration
is not admitted by either result.

[Local verification, 2026-10-05](verification-2026-10-05.json) records U03 source
hashes and checks, including six parallel runtime timeouts followed by a passing
serial workspace suite. These are development checks, not a new value campaign.

## Campaign log

[Attempt-accounting verification, 2026-10-06](verification-2026-10-06.json):
1,575 active workspace tests passed sequentially after one interactive-input
test timed out in parallel and then passed in isolation. The report now retains
distinct recorded attempts and exposes incomplete cost coverage. Native
[diagnostic variants](native-integration-u02/canary-diagnosis/README.md) refine
the canary observation without proving a code-mode callback or automatic routing.

Development verification, 2026-10-06: the unchanged Rust sources at `f637d0a`
passed all 1,563 active workspace tests in parallel locally. GitHub also passed
Linux/macOS quality, secret scans, dependency audit and native Linux checks on
that exact SHA. The U02 HTTP counter defect found during independent review was
reproduced, corrected and checked with 13 fixture tests. Its
[new native result](native-integration-u02/result-v4.json) still lacks the tool
canary and does not admit automatic integration. This is not a value campaign.

One line per campaign. The commit is the repository commit the `agent-router` binary was built from.

| Date | Commit | Manifest | Scope | Status | Results |
| --- | --- | --- | --- | --- | --- |
| 2026-09-30 | `c43d289` | `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856` | host-direct pilot; 10 tasks, arms A, B, G, C, E, Claude and Codex, repetition 1 (100 runs) | done: 100 of 100 runs, 0 invalid, one infrastructure retry; exported 2026-09-30 after the run ended | [runs/2026-09-30-pilot](runs/2026-09-30-pilot/README.md) |

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
- Runs concerned, final export (100 runs): `codex-A-t06-r1`, `codex-A-t07-r1`, `codex-A-t09-r1`, `codex-C-t01-r1`, `codex-C-t02-r1`, `codex-C-t05-r1`, `codex-C-t07-r1`, `codex-G-t04-r1`. All eight passed their hidden test and had no out-of-scope path other than `tests/__pycache__/`. No Claude run is concerned.
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

Status: done. 100 of 100 runs, exported with `export_results.py --label final` after the run ended. Source: `runs/2026-09-30-pilot/results/`; the tables below are the unedited output of `python3 summarize.py` in the campaign folder. Total list cost: Claude 53.6719 USD, Codex 12.6600 USD, plus one Codex attempt set aside after an infrastructure failure, whose cost is UNKNOWN (no usage recorded).

Manifest `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856`, host config `user-config`, 100 run(s), repetitions present: [1]. Variance between repetitions: not measured.
Attempts set aside (retried or interrupted, excluded from the tables but paid): ['codex-B-t07-r1.retried'], list cost UNKNOWN USD.
Runs whose verdict the pycache correction changes: ['codex-A-t06-r1', 'codex-A-t07-r1', 'codex-A-t09-r1', 'codex-C-t01-r1', 'codex-C-t02-r1', 'codex-C-t05-r1', 'codex-C-t07-r1', 'codex-G-t04-r1'].

| Provider | Arm | Runs | Invalid | Accepted raw | Rate raw | List USD per accepted raw | Accepted corrected | Rate corrected | List USD per accepted corrected | List cost USD | Product cost USD | Input tokens | Cached input | Cache writes | Output | p50 time to acceptance s (corrected) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| claude | A | 10 | 0 | 10 | 100% | 1.0271 | 10 | 100% | 1.0271 | 10.2710 | 5.8078 | 5930031 | 4258389 | 1671468 | 34792 | 83.9 |
| claude | B | 10 | 0 | 10 | 100% | 1.5261 | 10 | 100% | 1.5261 | 15.2613 | 8.5743 | 6744617 | 5072672 | 1671749 | 43601 | 89.2 |
| claude | G | 10 | 0 | 10 | 100% | 1.5037 | 10 | 100% | 1.5037 | 15.0368 | 8.3612 | 6393196 | 4724121 | 1668889 | 37003 | 92.3 |
| claude | C | 10 | 0 | 10 | 100% | 0.7696 | 10 | 100% | 0.7696 | 7.6960 | 4.4139 | 5695789 | 4054544 | 1641077 | 32046 | 66.0 |
| claude | E | 10 | 0 | 10 | 100% | 0.5407 | 10 | 100% | 0.5407 | 5.4068 | 3.1032 | 2822016 | 2246034 | 575900 | 17504 | 35.9 |
| codex | A | 10 | 0 | 7 | 70% | 0.4356 | 10 | 100% | 0.3049 | 3.0492 | 3.0492 | 5449053 | 4538880 | 0 | 35504 | 146.8 |
| codex | B | 10 | 0 | 10 | 100% | 0.2807 | 10 | 100% | 0.2807 | 2.8075 | 2.8075 | 5443356 | 4429696 | 0 | 33719 | 251.1 |
| codex | G | 10 | 0 | 9 | 90% | 0.2909 | 10 | 100% | 0.2618 | 2.6178 | 2.6178 | 4360162 | 3337856 | 0 | 23939 | 194.9 |
| codex | C | 10 | 0 | 6 | 60% | 0.5117 | 10 | 100% | 0.3070 | 3.0704 | 3.0704 | 5469411 | 4618240 | 0 | 37030 | 130.1 |
| codex | E | 10 | 0 | 10 | 100% | 0.1115 | 10 | 100% | 0.1115 | 1.1151 | 1.1151 | 2462501 | 2103936 | 0 | 18757 | 127.8 |

Paired comparisons per task (descriptive, no significance test). Discordance: share of paired tasks where exactly one of the two arms is accepted. Cost ratio: list cost of X summed over the paired tasks, divided by that of Y. The sizing section of the final report computes discordance on the raw column only.

| Provider | X vs Y | Paired tasks | Discordance raw | Discordance corrected | Tasks where X costs less | Cost ratio X/Y |
| --- | --- | --- | --- | --- | --- | --- |
| claude | A vs B | 10 | 0.00 | 0.00 | 10/10 | 0.673 |
| claude | A vs G | 10 | 0.00 | 0.00 | 10/10 | 0.683 |
| claude | A vs C | 10 | 0.00 | 0.00 | 0/10 | 1.335 |
| claude | A vs E | 10 | 0.00 | 0.00 | 0/10 | 1.900 |
| claude | B vs E | 10 | 0.00 | 0.00 | 0/10 | 2.823 |
| claude | B vs G | 10 | 0.00 | 0.00 | 2/10 | 1.015 |
| claude | G vs C | 10 | 0.00 | 0.00 | 0/10 | 1.954 |
| codex | A vs B | 10 | 0.30 | 0.00 | 2/10 | 1.086 |
| codex | A vs G | 10 | 0.40 | 0.00 | 0/10 | 1.165 |
| codex | A vs C | 10 | 0.50 | 0.00 | 5/10 | 0.993 |
| codex | A vs E | 10 | 0.30 | 0.00 | 0/10 | 2.734 |
| codex | B vs E | 10 | 0.00 | 0.00 | 0/10 | 2.518 |
| codex | B vs G | 10 | 0.10 | 0.00 | 2/10 | 1.072 |
| codex | G vs C | 10 | 0.50 | 0.00 | 8/10 | 0.853 |

Fixed host overhead, one `Reply with OK` call per model and host config:

| Provider | Model | Host config | Input tokens | Cached input | Cache writes | Output | List USD |
| --- | --- | --- | --- | --- | --- | --- | --- |
| claude | claude-opus-5-5 | isolated | 20462 | 12407 | 8053 | 4 | 0.0670 |
| claude | claude-opus-5-5 | user-config | 66328 | 0 | 66326 | 247 | 0.5356 |
| claude | claude-sonnet-5-5 | isolated | 20483 | 12407 | 8074 | 4 | 0.0348 |
| claude | claude-sonnet-5-5 | user-config | 65167 | 0 | 65165 | 15 | 0.2608 |
| codex | gpt-5.6-terra | isolated | 41948 | 25088 | 0 | 227 | 0.0415 |
| codex | gpt-5.6-terra | user-config | 64579 | 37376 | 0 | 192 | 0.0642 |
| codex | gpt-6.1-sol | isolated | 21235 | 0 | 0 | 5 | 0.0425 |
| codex | gpt-6.1-sol | user-config | 32215 | 7168 | 0 | 5 | 0.0509 |

Cold-probe bound per arm: the probe cost of each phase model under `user-config`, summed over the arm's processes, divided by the arm list cost. The probe pays a cold cache write; a run that reuses a warm cache pays less for the same prefix, so the ratio can exceed 100%. It bounds the fixed prefix cost from above; it is not the true share, which these data do not identify (the number of turns per process is not recorded).

| Provider | Arm | Cold-probe bound USD | Arm list cost USD | Bound / cost |
| --- | --- | --- | --- | --- |
| claude | A | 10.5718 | 10.2710 | 102.9% |
| claude | B | 16.0667 | 15.2613 | 105.3% |
| claude | G | 16.0667 | 15.0368 | 106.8% |
| claude | C | 7.8244 | 7.6960 | 101.7% |
| claude | E | 5.3556 | 5.4068 | 99.1% |
| codex | A | 1.7923 | 3.0492 | 58.8% |
| codex | B | 1.5258 | 2.8075 | 54.3% |
| codex | G | 1.5258 | 2.6178 | 58.3% |
| codex | C | 1.9256 | 3.0704 | 62.7% |
| codex | E | 0.5086 | 1.1151 | 45.6% |
#### Conclusions of the full run

Scope: 10 small Python tasks, one repetition, catalog list prices, host-direct execution. Neither `summarize.py` nor the final report computes a significance test; every statement below describes this pilot only.

Claude:

- Acceptance: every arm accepted 10 of 10 tasks, raw and corrected. On this pilot the corpus does not separate the arms on quality.
- Cost per accepted task, identical in both columns: A 1.0271 USD, B 1.5261, G 1.5037, C 0.7696, E 0.5407. On this pilot, A cost 0.673 times B and 0.683 times G and was cheaper than B on 10 of 10 tasks; A cost 1.335 times C and 1.900 times E and was more expensive than both on 10 of 10 tasks.
- Median time to acceptance: A 83.9 s, B 89.2 s, G 92.3 s, C 66.0 s, E 35.9 s.

Codex:

- Acceptance raw: A 7/10, B 10/10, G 9/10, C 6/10, E 10/10. Corrected: 10/10 for every arm. Every raw rejection is one of the eight pycache runs above, so the raw differences between arms measure the harness defect, not the models.
- Cost per accepted task, raw: A 0.4356 USD, B 0.2807, G 0.2909, C 0.5117, E 0.1115. Corrected: A 0.3049, B 0.2807, G 0.2618, C 0.3070, E 0.1115.
- Total list cost, which does not depend on acceptance: A cost 1.086 times B (cheaper on 2 of 10 tasks), 1.165 times G (0 of 10), 0.993 times C (5 of 10) and 2.734 times E (0 of 10).
- The pool's "cheaper" model is not cheaper per token. In `pilot/pricing.json`, `gpt-5.6-terra` and `gpt-6.1-sol` share the input price (2 USD per million tokens), and `gpt-5.6-terra` costs more for cached input (0.20 against 0.10) and output (12 against 10). On this pilot, G (`gpt-6.1-sol`, medium) cost less than C (`gpt-5.6-terra`, medium) on 8 of 10 tasks, ratio 0.853. Arm A sends execute and verify to `gpt-5.6-terra`, so its recipe could not save list cost here.
- Median time to acceptance (corrected): A 146.8 s, B 251.1 s, G 194.9 s, C 130.1 s, E 127.8 s.

Both providers:

- E, a single session with the frontier model at high effort, had the lowest cost per accepted task of every arm on both providers, in both columns, with the same acceptance as B. B cost 2.823 times E on Claude and 2.518 times E on Codex. This is the protocol's E1 contrast; the pilot does not isolate its cause, and the loss of the prompt cache between separate phase processes is one candidate the protocol names.

The README tagline at the time of this pilot, "Frontier models for the hard steps, cheaper ones for the rest", was a product claim, not a result of this pilot. Against it:

| Provider | Criterion | A vs B | A vs G | A vs C | A vs E |
| --- | --- | --- | --- | --- | --- |
| Claude | Cost per accepted task | supports: A cheaper (0.673) | supports: A cheaper (0.683) | contradicts: A dearer (1.335) at equal acceptance | contradicts: A dearer (1.900) at equal acceptance |
| Claude | Acceptance rate | does not settle: 10/10 both | does not settle: 10/10 both | does not settle: 10/10 both | does not settle: 10/10 both |
| Codex | Cost per accepted task | contradicts: A dearer, raw 0.4356 against 0.2807, corrected 0.3049 against 0.2807 | contradicts: A dearer in both columns | does not settle: corrected 0.3049 against 0.3070, total cost ratio 0.993; raw 0.4356 against 0.5117 favours A only through the defect | contradicts: A dearer in both columns |
| Codex | Acceptance rate | does not settle: corrected 10/10 both; raw 7/10 against 10/10 is the defect | does not settle: same | does not settle: same | does not settle: same |

On this pilot the tagline holds only for Claude, on cost, against the frontier-only arms B and G. It does not hold against C or E on either provider, and quality is not separated anywhere, because every arm reached the ceiling once the defect is corrected.

#### Sizing inputs for N

From the `sizing` section of `results/reports/final-report.json`, which the sealed harness computed on the raw acceptance column. The protocol's formulas give N for a 10-point success difference and for a cost ratio of 0.8, at alpha 1.25 % (first Holm step), power 80 %.

| Pair | Paired tasks | Discordance (raw) | SD of log cost ratio per task | N, success 10 points | N, cost ratio 0.8 |
| --- | --- | --- | --- | --- | --- |
| Claude A vs B | 10 | 0.0 | 0.0225 | none (discordance 0) | 1 |
| Claude A vs C | 10 | 0.0 | 0.0188 | none | 1 |
| Claude B vs E | 10 | 0.0 | 0.0447 | none | 1 |
| Claude B vs G | 10 | 0.0 | 0.0188 | none | 1 |
| Claude G vs C | 10 | 0.0 | 0.0130 | none | 1 |
| Codex A vs B | 10 | 0.3 | 0.1473 | 286 | 5 |
| Codex A vs C | 10 | 0.5 | 0.2001 | 476 | 8 |
| Codex B vs E | 10 | 0.0 | 0.1704 | none | 6 |
| Codex B vs G | 10 | 0.1 | 0.0963 | 96 | 2 |
| Codex G vs C | 10 | 0.5 | 0.2054 | 476 | 9 |

Reading limits:

- Every nonzero discordance above comes from the pycache defect. With the corrected column the discordance is 0.00 on every pair (paired comparisons table), so this pilot gives no usable discordance and no success N. The raw success N values must not size the main bench.
- The cost N values come from one repetition of 10 tasks that every arm solved; they contain no run-to-run variance, which was not measured. They are the protocol formula applied to these standard deviations, not a recommendation.

## Next campaign: harness fixes

These fixes change sealed inputs, so they need a new campaign folder and a new manifest; the 2026-09-30 manifest stays as recorded. The review candidate is in [`campaign-next/`](campaign-next/README.md). It has no manifest, no campaign outputs and no real-provider results.

- The candidate places `__pycache__/` in the copied base repository, records completed turns per host process, and validates each price class before manifest creation. All three behavior changes have local Python tests.
- Its three synthetic challenge fixtures are candidate material only. They still need merged-PR provenance, frozen source commits and independent leak review before they can enter a protocol campaign.
- Use one global budget across providers and attempts. The current authorization is at most 5 USD total, including prior spend. A real `calibrate` or `run` needs explicit spend confirmation, a global cap no higher than the remaining authorization and a finite remaining-cost estimate; UNKNOWN cost or a projection above the cap fails closed. The candidate currently disables real host-direct work because it has no verified per-process hard cost ceiling. Do not treat the cap check as a spend guarantee until that ceiling is implemented.
- Refresh and human-check the catalog prices before sealing. The copied snapshot is historical and does not pass the cheaper-role comparison for the current candidate pools.

## Limits of the 2026-09-30 pilot

- Host-direct: the host CLIs run outside the Agent Router runtime. `prepare`, `run --approve`, the native sandbox, `verify`, `artifacts apply` and the feedback store are not exercised; costs are computed by the harness with `price_usage` semantics, not read from `feedback report --pricing`.
- Arm A applies one constant per-phase recipe on every task (frontier model with high effort for plan, cheaper model with medium effort for execute and verify). The pilot compares that recipe with the fixed arms; it does not test a per-task choice.
- 10 small Python tasks in one synthetic repository, standard library only. They are not the protocol's corpus of merged pull requests.
- One repetition: variance between repetitions is not measured, and the sizing inputs mix task variance with run-to-run noise.
- Costs are catalog list prices, not invoices. The runs used the maintainer's subscriptions.
- OpenAI prices were read through a summarizing fetch of the pricing page, verbatim rows in `pilot/pricing-sources.md`; they need a human reread. The Codex credit rate card was not read (HTTP 403), so no credit figure exists.
- The harness counts `tests/__pycache__/` as an out-of-scope write. The corrected column is a deviation decided after observation; see the pycache correction above.
- 10 tasks give a direction and variance estimates to fix N. No adoption claim for H1 or H2 follows from this pilot.
