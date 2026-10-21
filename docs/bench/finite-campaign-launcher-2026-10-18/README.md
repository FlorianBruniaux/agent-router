# One-cell Linux campaign launcher

`cargo build -p router-runtime --example finite-campaign --locked` builds the parent entry. It dispatches one sealed assignment through the owned finite Runtime. It contains no provider HTTP sender, account authentication reader or campaign ledger. `--inspect` prints the public 24-assignment manifest without dispatch. `--run` requires a fresh private parent pipe and exact approval before native execution.

## Input and approval

The first input line is a bounded strict JSON descriptor. It fixes the assignment, configuration/account/controller hashes, 60,000 ms deadline, 6,000 observed tokens, 48,000,000 equivalent microcredit stop, and root `/task/campaign/<assignment_id>`. The six program pins must use `/task/programs/{client,native,helper,guard,profile,namespace}`. Unknown fields, aliases, changed limits and existing assignment roots are refused.

The parent materializes only the sealed public packet. H0 and H1 retain their original task files; only H1 includes its prescribed feedback harness. The frozen acceptance hash is carried for the independent evaluator. No solution or hidden evaluator enters the packet.

Fresh local OS canaries run before preparation. Their broker accepts only the fixed diagnostic GET, has bounded reads and is joined before the subscription relay starts. It cannot forward a provider request. Discovery returns authenticated capability evidence, never worker execution authority. Runtime repeats its own canaries under its retained owners.

Stdout then contains `runtime_prepare` and `runtime_binding` JSON lines. The parent must answer `approve_runtime` with the complete exact nine-field binding and packet hash. A digest alone, a changed nonce or another packet is refused. Subsequent stdout is the existing raw Session JSONL protocol. The host may wait up to 300 seconds for preparation, before any trial or provider request. The pinned Session controller validates `runtime_ready`, persists trial start and resets its unchanged 60-second window before provider traffic. Diagnostics use stderr. The caller must establish private pipe provenance; a copied JSON approval does not establish it.

## Assignment and limits

This experiment chooses exactly one model and effort per cell. Its explicit `sealed-assignment-v1` policy assigns zero utility weights and offers no alternatives. It still enforces all eligibility, evidence, permission, scope and resource checks. It makes no measured quality, price or provider availability claim.

The single execution node owns the task's unchanged 60,000 ms / 6,000-token allowance. Independent evaluation is outside this worker plan. A successful worker remains `Verifying`, with no implicit acceptance or source integration. This avoids halving the trial allowance through the general purpose two-node plan builder.

The Linux launcher requires cgroup memory 8,589,934,592 bytes and swap 0. The existing finite guard, file policy, detached descendant cleanup and MCP limits are retained. Three fixed tools, at most 8 calls, 32 KiB files, 8 KiB output per call, 32 KiB total output, 5 s per execution, 15 s total execution and 16 MiB scratch remain enforced. The host ledger owns consumption and the 48-credit equivalent stop. This observed stop is not a provider billing cap.

## Output and qualification

After proven namespace termination, an exact terminal receipt and a stable single-link regular-file check, only the declared submission may be exported under `submission/`. Its bytes and hash must match the retained tool terminal. `runtime-result.json` records state, cleanup, admission release, attempts, events, original source preservation and export metadata. Human acceptance stays null. No private home, controller state or authentication directory is exported.

`qualify-offline.py` is a separate coded-response test driver using the published subscription controller fixtures. It cannot call the live sender. Each selected cell has a fresh test ledger; preceding rows are explicitly synthetic receipts used only to reach that cell in the ordered test ledger. Only the selected cell executes natively. These rows must never be imported into a real campaign ledger.

With an independently verified public SDK, Linux image/cache, six program pins and private candidate directory prepared, run one case as follows:

```sh
python3 -B docs/bench/finite-campaign-launcher-2026-10-18/qualify-offline.py t02-Lx-r1-H0 --output /private/candidate-directory --recipe /private/verified-argv-recipe.json
```

The driver checks the model and effort actually requested, coded tool returns, feedback execution for H1, exact submission bytes, unchanged source, namespace cleanup and admission release. Transport qualification does not establish task correctness or real provider consumption.

[The qualification record](qualification.json) records six passing native cases with zero provider calls and the exact artifact hashes. [The local quality record](local-quality.json) retains three failed workspace runs and targeted replays; the final sequential all-feature workspace run passed on 2026-10-19 after the Doctor test framing correction. Format, Clippy, harness parity and the repository secret scan also passed. The child still runs the same single lock probe with the same assertions; its explicit two-thread libtest setting prevents an inherited single-thread stdout prefix from corrupting the marker. A stale capability proof remains a refusal: the launcher does not extend its 300-second validity or refresh timestamps without running discovery again. No CI result from the preceding subscription relay commit qualifies this new launcher.

## Useful score correction

`--useful-score` is restricted to `t02-Lm-r1-H1`. It reuses the published authored correction, observes failing public feedback before the write and passing feedback afterwards, and exports the actual captured patch as well as the submission. It remains a fake-provider fixture, with no account or inference.

The [external acceptance replay](../native-useful-score-2026-10-21/README.md) checks twelve named cases against that exported artifact and reconstructs its exact Git tree. It does not turn Runtime's `Verifying` state into human acceptance or qualify the frozen campaign's hidden evaluator.
