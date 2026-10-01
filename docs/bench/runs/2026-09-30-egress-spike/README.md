# Provider egress spike of 2026-09-30 (ADR 0021, batch 4)

This folder records batch 4 of [ADR 0021](../../../adr/0021-host-capability-evidence.md): does a native Codex or Claude Code run, launched the way `router-runtime` launches it under the `network: false` Seatbelt profile, reach its provider? Three runs, on macOS, by the maintainer, on 2026-09-30. Total list cost: 1094 microUSD, all on Codex `gpt-5.6-luna`.

## Contents

| Path | What it is |
| --- | --- |
| `harness/v1/`, `harness/v2/`, `harness/v3/` | `spike_egress.py` and its fake-host test `selftest.py`, byte-identical to the files that ran. Each `spike_egress.py` hashes to the `script_sha256` of the matching manifest |
| `results/v1-20260930T170911Z/` | Manifest and `codex.json` of run v1 |
| `results/v2-20260930T173101Z/` | Manifest and `codex.json` of run v2 |
| `results/v3-20260930T184024Z/` | Manifest, `codex.json` and `claude.json` of run v3 |
| `results/v4-startup/` | Allowlist projections of the three v4 Claude startup records. Raw v4 files remain outside this repository |
| `SHA256SUMS` | SHA-256 of every file above except itself, redacted copies included |
| `REDACTIONS` | Original and redacted SHA-256 of each redacted file, including the v4 allowlist rule |

| Harness | `spike_egress.py` SHA-256 | Fake-host test |
| --- | --- | --- |
| v1 | `01d09d3397fc99a7c06aab2e1f93de4124590b2bc759a3c88a8f5ed51b4c61cf` | 21 of 21 |
| v2 | `09026180ad2e9e88b06a4d645fa23aa1c0331a6423fc5cd1e0f85768d36c2aba` | 23 of 23 |
| v3 | `c7934c0774634240f3022e37a088dcef93cce375d6efde693b1116074a785cbf` | 31 of 31 |

## Verify

```sh
cd docs/bench/runs/2026-09-30-egress-spike
shasum -a 256 -c SHA256SUMS
```

To rerun a fake-host test, copy one harness folder elsewhere and put next to it `pricing.json` and `pricing-cache-writes.json` from `../2026-09-30-pilot/pilot/`, and `crates/router-runtime/src/sandbox.rs` at `f263e7d` renamed `sandbox.rs.f263e7d`. The three files are byte-identical to the ones the runs used. Then run `python3 selftest.py`. It spends no token and launches no real host.

## Method

The harness replicates, from `f263e7d`, the Seatbelt profile text (`crates/router-runtime/src/sandbox.rs:428-451`, compared with the source at start), the command construction (`sandbox.rs:284-322`: reads are the read roots, the working directory and the program; the environment is cleared, then `PATH=/usr/bin:/bin` and named variables), the network flag (`crates/router-runtime/src/lib.rs:1521`) and the argv of the Codex and Claude adapters (`crates/adapter-codex/src/execution.rs:31-50`, `crates/adapter-claude/src/execution.rs:34-58`). It does not replicate the resource helper, the approval digest or the router prompt template.

Per host, with no retry:

- `S0` and `S1` (v2 and later): `--version`, then `codex login status` or `claude auth status`, under `network: false`, no model call;
- `C1`: one model call under `network: false`, the measurement;
- `C2`: the same call under the same profile with `network: true`, the control;
- `C3`: the same call from a private copy of the binary, `network: true`, only when `C2` completed.

A preflight repeats the runtime's own probe before any host (loopback `nc` denied under `network: false`, allowed under `network: true`, write outside the roots denied). It passed in all three runs (`preflight.sandbox_effective`).

## Runs

The command lines are reconstructed from the manifest fields (`claude_keychain_read_root`, `codex_allow_preferences`); the harness does not record its argv.

| Run | Command (from the spike folder) | Verdict | What it shows |
| --- | --- | --- | --- |
| v1 `20260930T170911Z` | `python3 spike_egress.py --confirm-spend` | Codex `inconclusive`, run stopped after Codex; Claude not run | Under the runtime's read-only profile, which has no writable root, Codex exits 1 with `fs_denied` in `C1` (127 ms) and `C2` (73 ms). The failure does not depend on the network. The launcher `codex.js` exits 127 (`launcher_missing`, `env: node`) because `PATH` is `/usr/bin:/bin`. v1 kept no stderr text, so the denied path is not recorded |
| v2 `20260930T173101Z` | `python3 spike_egress.py --confirm-spend --claude-keychain-read-root` | Codex `inconclusive`, cause `model_call` under the v2 rule, run stopped; Claude not run | With a private 0700 `CODEX_HOME` as the only writable root, `S0` and `S1` pass (`Logged in using ChatGPT`). `C1` (178 ms) and `C2` (128 ms) both exit 1 with `failed to load configuration: Failed to synchronize managed preferences (code -32600)`: a configuration failure, not a network one |
| v3 `20260930T184024Z` | `python3 spike_egress.py --confirm-spend --codex-allow-preferences domain --claude-keychain-read-root` | Codex `provider_unreachable_under_network_false`, copy `runs_from_copy`; Claude `inconclusive`, cause `startup` | See below |

### Run v3

Codex (`results/v3-20260930T184024Z/codex.json`), with the profile deviations `write root: private configuration directory` and `codex preferences: domain`:

- `C1`, `network: false`: exit 1 after 16 872 ms, categories `net_connect` and `fs_denied`, no root assistant message. The excerpt contains `failed to refresh available models: Connection failed: error sending request`;
- `C2`, `network: true`: exit 0 after 5 565 ms with a root assistant message; 11 586 input tokens (9 984 cached), 5 output tokens, 526 microUSD;
- `C3`, private copy, `network: true`: exit 0 after 3 435 ms with a root assistant message, 568 microUSD. The copy hashes like the original;
- the residual `fs_denied` in `C1`, `C2` and `C3` is a skill scan under `$HOME/.config` that the profile does not grant; the calls still completed;
- `auth.json`: copied 0600 into the private directory, deleted at the end, `signature_changed_by_host: false`.

Claude (`results/v3-20260930T184024Z/claude.json`), with the deviations `write root: private configuration directory` and `read root: ~/Library/Keychains`:

- `S0`: `2.1.285 (Claude Code)`;
- `S1` `auth status`: exit 1, empty stderr; the stdout excerpt reports `"loggedIn": false` and `"authMethod": "none"`. The private directory received `.claude.json`, `.claude.json.lock` and `backups`. No model call.

### v4 startup projections

The three projections under `results/v4-startup/` correspond only to `20260930T190002Z`, `20260930T190228Z` and `20260930T190433Z`. Each records the source schema, fake flag, start marker, host, source-file SHA-256 values, calls, catalog cost completeness, startup booleans and a reduced verdict. All three record zero calls, zero list cost and `startup.ok=false`; they are startup diagnostics, not evidence of a completed host run or a ready campaign.

The projection omits argv, executable and configuration values, filesystem roots, environment names, keychain data, profile rules, stderr, launch details and free text. `REDACTIONS` records the combined source-pair hash, projected-file hash and allowlist rule. Verify the stored projections with `shasum -a 256 -c SHA256SUMS`.

## What the runs establish

- Codex `0.159.2` does not reach its provider under the `network: false` profile, and does under the same profile with `network: true` (v3 `C1` against `C2`). This holds for a profile with two deviations from the runtime's: a writable private configuration directory and `user-preference-read` on `com.openai.codex` with shared-memory reads named `apple.cfprefs.*`.
- Codex does not start at all under the runtime's read-only profile (v1), and its configuration loader fails without preference reads (v2).
- Codex runs from a private copy of its native executable (v3 `C3`).
- Nothing about Claude Code egress: it did not pass its startup check.

## Not established

- That no channel other than the provider request escapes the socket denial: the harness observes one call, not every channel. DNS resolution under `network: false` was not probed.
- Behaviour with the resource helper, the approval path or the router prompt, none of which the harness replicates.
- Which Seatbelt operation v2 lacked: the `domain` rules made `C2` complete, but the Seatbelt log was not read.
- Why Claude reported no login. The Claude Code `2.1.285` binary names its keychain entry with a suffix derived from `CLAUDE_CONFIG_DIR` when that variable is set (strings of the binary, read on 2026-09-30); this is a reading of the code, not an observation of the run.
- Linux and bubblewrap.

## What is not stored

No model text, no `auth.json` content, no raw stderr. Each launch keeps exit code, termination, timings, byte counts and SHA-256 of stdout and stderr, stderr categories, and (v2 and later) a stderr excerpt of at most 300 bytes with home paths, run paths and token-like strings redacted by the harness. The private configuration directories, workspaces and binary copies stayed out of this folder.

## Redactions

The prefix rule of the pilot: the maintainer's home directory prefix is replaced by the literal `$SPIKE_HOME` in the four result files listed in `REDACTIONS`, with their original and redacted SHA-256. The harness files are not redacted, so their hashes still match the manifests. They contain no home path: in v2 and v3 the home-directory regular expression of the harness redactor (`spike_egress.py`, function `make_redactor`) and, in the v2 and v3 `selftest.py`, the maintainer's user name as the string the redaction test must not find.
