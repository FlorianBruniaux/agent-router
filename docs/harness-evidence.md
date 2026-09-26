# Issue #12 validation evidence

Observed 2026-09-26. Base: `739051c502f5cd20ec5df9fce8cfedd178ce52e2`.
Branch: `codex/issue-12-project-harness`. Clean baseline, 80 tests in 14
suites, formatting and warnings-denied Clippy passed before edits.

## Local evidence

- Workspace formatting and warnings-denied Clippy passed after the Rust change.
- Final workspace tests: 93 passed in 18 suites, including 13 new harness tests.
- Generation produced 40 owned files. Runtime activation is not inferred.
- Gitleaks 8.30.1 found no leaks in the directory scan or the 36-commit history.
- Native TOML/JSON syntax passed with Python 3.14.7. The default shell's Python
  lacks `tomllib`; CI explicitly selects Python 3.12. Skill local links resolve
  in canonical and both generated trees.
- Canonical `ctxharness` absent. Static configuration health: `UNKNOWN`.
  Behavioral evidence from that auditor: `UNKNOWN`. No substitute audit verdict.
- `cargo audit` and `cargo deny` absent locally; not installed or claimed passing.

Reference files in CCBoard, CC Copilot Bridge and RTK were read only. Dirty or
untracked reference configuration was not used as canonical input. Tracked
versions of CC Copilot Bridge files were read from HEAD. No files from these
repositories were copied into the implementation.

## Host evidence

Local binaries: Codex CLI 0.153.4; Claude Code 2.1.283.
Both canaries initially encountered the outer execution sandbox. They were
rerun with execution authorization, preserving the hosts' native restrictions.
Neither canary used a permission or hook-trust bypass flag, read tools, wrote
project files, or launched subagents.

| Observation | Codex | Claude Code |
| --- | --- | --- |
| Fresh session | `exec --ephemeral --json`, read-only, approvals never | `--no-session-persistence`, plan mode, project settings only |
| Five project agents | All five named from loaded context in native-settings canary | All five listed in native `system.init.agents` |
| Six project skills | All six named from loaded context | All six listed in native `system.init.skills` |
| Hook invocation | Native trusted startup delivered the project marker | Native `SessionStart:startup` hook response, exit 0, project marker in stdout |
| Context delivery | Model returned `agent-router-harness:codex:parity-ok` without being given its value | Model returned `agent-router-harness:claude:parity-ok` without being given its value |
| Trust of exact current definition | Native `/hooks` review followed by `hooks/list`: trusted and enabled | Execution observed; no separate persisted-trust claim |
| Tool interception | Not applicable | Not applicable |

Codex's first isolated canary used `--ignore-user-config`: it discovered skills
but did not expose agents. A second canary retained native user settings and
exposed all project agents; neither delivered the project hook marker. The follow-up native `/hooks` review identified the project hook as new. Only
that project hook was trusted through the native interface; other pending hooks
were left unchanged. No global hook definitions or permission configuration were
edited. Native trust metadata was saved by Codex. The read-only `hooks/list` API
then confirmed the exact command, enabled state and native definition hash
`sha256:9061124d55c803672933c148d0b707da2e32d3670463b8d2b03cafe9b4b02d94`.
This native hash differs from the file hash below by design. A fresh ephemeral
canary delivered `agent-router-harness:codex:parity-ok`. Unrelated MCP startup
failures were present and did not prevent this observed effect. Native agent
invocation and OS sandbox enforcement were not established by these discovery
canaries.

## Independent review

A separate reviewer read the diff and exercised isolated temporary fixtures.
It found one P2 defect: a valid large source could produce a recovery journal
over the reader's 1 MiB limit, making rollback unavailable after installation.
The fix rejects that transaction before projection writes and preserves the
previous journal. The reviewer independently confirmed the regression and all
13 xtask tests. No other concrete defect was found. Generation and final gates
followed the fix because it changes the hook's compiled-source digest.

BM25 automatic routing is excluded by the issue task boundary. Six sets of
eight positive and two negative prompts exist for future evaluation; scenario
presence is not routing accuracy.

## Final local gates

All commands below passed after correction and regeneration:

```sh
rtk cargo fmt --all -- --check
CARGO_TARGET_DIR=/private/tmp/agent-router-harness-target rtk cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
CARGO_TARGET_DIR=/private/tmp/agent-router-harness-target rtk cargo test --workspace --locked
CARGO_TARGET_DIR=/private/tmp/agent-router-harness-target rtk cargo run -p xtask --locked -- check
rtk cargo build -p xtask --locked --target-dir target
python3.14 scripts/check-host-syntax.py
gitleaks dir . --redact --no-banner
gitleaks git . --redact --no-banner
rtk git diff --check
```

The syntax command was run with the installed Python 3.14 executable, not the
older default `python3`. Generated inventory: 40 files including the inventory.
Final definition SHA-256:

- `.codex/hooks.json`: `6ab4648dfa4dbdd4ae1b649006c1ea28e1a984de06597f92599d93d30c4da7b0`
- `.claude/settings.json`: `3cc3d6927a28d6b8a89555fee39b9dc4317c44220375ce15465da1a4019592f6`

The Codex native-trust and fresh-session criteria were verified on 2026-09-26
UTC. CI passed on the implementation commit `c9f7f23a2bc701577b3c9d6639b2c8309628fafb`
for Linux, macOS and secrets (push run 36261774490 and PR run 36261791523).
This evidence update must also pass the PR checks before merge. The project
harness is complete within issue #12; this does not implement product routing
or establish runtime capabilities for future HostProbe snapshots.
