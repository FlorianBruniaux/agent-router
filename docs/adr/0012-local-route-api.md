---
adr: "0012"
title: Route-only local API on an owner-private Unix socket
status: accepted
created: 2026-09-29
last_modified: 2026-09-29
scope: "Opt-in `local-api` feature."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0012: Route-only local API on an owner-private Unix socket

## Context

`docs/specs/technical-specification.md` (lines 586-590) asks for a local API on
a Unix socket by default with an owner-only token file, and section 21 (line
712) says not to start with a daemon. `docs/standard-sidecar.md` (lines
153-156) keeps this surface out of the sidecar. Routing and preparation execute
nothing; any command family with authority stays closed without explicit
approval, as for R06 (`docs/catalog-active-cli.md`).

## Decision

Add `agent-router serve` behind the default-off `local-api` feature. It serves
three endpoints and nothing else: `GET /v1/health`, `GET /v1/capabilities` and
`POST /v1/route`.

- Transport is `std::os::unix::net::UnixListener` with one bounded thread per
  connection. No async runtime, HTTP framework or TCP listener is added. The
  CLI's tokio dependency keeps its features (`rt-multi-thread`, `time`,
  `signal`; `git diff 6f36042 -- crates/agent-router-cli/Cargo.toml` adds only
  the `local-api` feature and the optional crate) and is used only to catch
  SIGINT and SIGTERM.
- The `--state-dir` must be an absolute, existing-or-creatable directory owned
  by the caller with mode 0700, and not a symlink. It holds the socket (0600)
  and a bearer token file (0600, generated from `/dev/urandom` when absent),
  plus a `lock` file held with `flock` for the server's life. Unsafe entries
  are refused, never repaired. Parents of the directory are not checked.
- Every endpoint requires the token. Comparison hashes both sides with SHA-256
  and compares in constant time with `subtle`. Authentication precedes routing
  and method checks, so unauthenticated callers learn nothing else.
- The route body carries `request`, `catalog` and an optional `host_snapshot` as
  inline JSON. The server never opens a caller-supplied path, the catalog store
  or any other file.
- The handler is the same code path as `agent-router route --json` after file
  reading (`decide` in `main.rs`), so a decision is byte-identical.
- Framing is closed: HTTP/1.1, one request per connection, `Content-Length`
  only, no `Transfer-Encoding`, no pipelining, headers capped at 16 KiB,
  a whole-request deadline, fixed budgets for the response write and the
  final drain, and a cap on simultaneous connections. A panicking handler
  yields a 500 and frees its slot.
- An envelope scanner rejects duplicate keys at every depth and unknown
  top-level fields before any typed parse, because `serde_json::Value` silently
  keeps the last duplicate.

## Consequences

No execute, feedback, outcome, activation, grant or catalog-store surface
exists in the API, and adding one needs a new decision. Nested unknown fields
follow the typed parsers: `RouteRequest` does not deny them today, so the API
accepts what the CLI accepts. There is no TLS, multi-user access or streaming.
The socket peer is not checked with `SO_PEERCRED`, and the socket mode is set
after `bind` rather than with `fchmod` before `listen`; the 0700 directory is
the access boundary. The API refuses JSON nested deeper than 64 levels, the
CLI's limit being serde_json's default. A crash leaves a stale socket, which the next start replaces
only if nothing accepts connections on it.
