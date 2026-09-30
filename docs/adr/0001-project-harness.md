---
adr: "0001"
title: One project harness, two host projections
status: accepted
created: 2026-09-26
last_modified: 2026-09-26
scope: "Issue #12. Product integration is outside this decision."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0001: One project harness, two host projections

## Decision

`harness/manifest.json`, its explicitly named Markdown sources, and the thin
launcher in `harness/session-start.sh` are the canonical project configuration.
The internal, unpublished `xtask` Rust crate renders both hosts. It has no
dependency on any product crate, and product crates must not depend on it.

Generated files are `AGENTS.md`, `CLAUDE.md`, `.codex/config.toml`,
`.codex/agents/*.toml`, `.codex/hooks.json`, `.claude/agents/*.md`,
`.claude/settings.json`, and matching skill trees in `.agents/skills/` and
`.claude/skills/`. `.harness-generated.json` records their SHA-256 digests.
Generated trees belong to this harness; add project agents and skills through
the canonical manifest. No home directory, credential, environment file,
machine-local allowlist or MCP configuration is an input.

Codex reads flat project instructions, native agent TOML and `.agents/skills/`.
Claude Code reads project instructions, agent Markdown and `.claude/skills/`.
Native models are configured independently, never translated as aliases between
providers. Codex uses supported platform IDs. Claude agents inherit the active
model; effort is omitted because support depends on that inherited model.
Codex sandbox settings and Claude tool restrictions are distinct mechanisms,
not equivalent OS isolation. Instructions never grant host permissions.

`cargo run -p xtask --locked -- check` compares exact bytes, detects missing,
changed and stale projections, and rejects unsafe paths and symlinks.
Installation and update use `generate`: preflight every owned preimage before
writing, save a project-local recovery journal, then replace each file by
same-directory rename. The operation is idempotent. `remove` deletes only
verified owned files. `rollback` restores the previous transaction after
checking that no file changed since it. No recursive deletion or global install
is supported. Atomicity is per file, not across the full bundle; stop hosts
while updating and use the journal after an interrupted transaction.

## Hook boundary

Both hosts receive a `SessionStart` advisory parity check. A thin shell launcher
locates an already-built `target/debug/xtask`; it never compiles, installs,
downloads or grants a permission. Rust owns bounded typed input parsing,
event filtering and the projection decision. A missing binary, malformed
input, unsupported event or inaccessible project produces an advisory or a
no-op, with exit zero. A drift report asks the agent to stop editing generated
files and repair the source. This is fail-open information, not a security
enforcer. `PreToolUse` permission hooks are rejected: this issue has no new
security policy that would justify interception.

Fixtures establish protocol behavior only. Fresh-session canaries must
separately establish discovery, trust, invocation and delivery of hook context.
No canary uses bypass-trust or bypass-permission flags. Codex `/hooks` review
and explicit trust of the exact current definition remain native host actions.
Unknown activation is recorded as `UNKNOWN`, including when a host is installed
but unavailable to a fresh authenticated session.

## Alternatives and sources

- Two manually maintained host trees are rejected because drift has no single
  resolution authority.
- A Python or shell generator is rejected in favor of a tested internal Rust
  tool using the workspace's existing dependencies. Shell only launches it.
- Reusing product routing crates would couple development configuration to
  runtime capabilities, so no product source is changed.
- Global installation and automatic trust are excluded by issue #12.
- BM25 implementation and live global index rebuilds are excluded by the task.
  Skill trigger scenarios are retained for later evaluation; they do not prove
  automatic routing or install an executable extension.

Reference repositories were read only: CCBoard contributed crate/error/ADR
principles; CC Copilot Bridge contributed generated parity and independent
review roles; RTK contributed thin adapters, atomic file replacement, integrity
and separate evidence levels. No source code or thresholds were copied.

Host formats were checked against [Codex subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents),
[Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Codex skills](https://learn.chatgpt.com/docs/build-skills), and
[Claude subagents](https://code.claude.com/docs/en/sub-agents).
The local CLIs observed at implementation were Codex 0.153.4 and Claude Code
2.1.283; model availability and runtime behavior need separate evidence.
