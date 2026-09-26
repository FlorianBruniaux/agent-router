<!-- Generated from harness/instructions.md; do not edit. -->
# Agent Router development harness

Agent Router currently computes and explains deterministic dry-run plans. It
executes no model, host, subagent or worktree. This development harness is
separate tooling. Issue #12 must not implement HostProbe or product adapters.

## Architecture and Rust

- `router-types` owns serialized contracts; `router-catalog` validates catalogs;
  `router-policy` applies constraints before scoring; `agent-router-cli` owns
  application I/O. `xtask` owns only the development harness.
- Preserve deterministic output, bounded input and work limits, explicit
  UNKNOWN evidence, and the trust rules in `docs/reference/json-contracts.md`.
- Use typed `thiserror` errors in libraries. Add operation context at CLI
  boundaries; use `anyhow` only if already needed there. No implicit panics in
  fallible library paths, raw provider responses or secret values in logs.
- Record structural decisions in `docs/adr/`. Add behavior tests with
  nontrivial changes and keep harness fixtures separate from product fixtures.

## Work boundaries

Inspect status, branch and HEAD before editing. Own explicit paths, preserve
unrelated changes and never stage the whole repository. No global config,
permission grants, MCP credentials, machine-local allowlists or environment
files belong in generated outputs. Skills give instructions, not executable
capabilities or authority. Do not enable a hook or claim trust by its presence.

Use read-only exploration, bounded implementation and independent review when
requested. The five project agents have separate output contracts. Assign file
ownership before allowing the implementer or test engineer to edit. Delegate
only when the user or an applicable instruction requests it; an available agent
is not a request to launch one.

## Verification and delivery

Run from the repository root, through `rtk` where available:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo run -p xtask --locked -- check
```

Use Gitleaks if installed; CI scans secrets. Do not require or install
`cargo audit` or `cargo deny` locally without checking availability. A failing
check blocks a passing verdict. Keep source checks, host discovery, hook trust,
invocation, interception and observed effect as separate evidence fields.

Edit `harness/`, regenerate with `cargo run -p xtask --locked -- generate`,
and inspect every generated path. See `docs/harness.md` for installation,
rollback and fresh-session canaries. Commits, pushes and PRs require the task's
authorization. Use a signed conventional commit and a scoped branch when
requested. Never infer merge authority from green checks.
