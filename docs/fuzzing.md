# Bounded parser fuzzing

The isolated [`fuzz/`](../fuzz/) Cargo workspace fuzzes three public trust boundaries without contacting providers, models, or other services:

- `host_snapshot` sends arbitrary bytes through `ValidatedHostObservation::from_json`, then exercises sealing and validation of decoded observations. Repeated calls must produce the same result.
- `provider_sse` sends the same bytes through the OpenAI Responses and Anthropic Messages stream adapters using a fixed synthetic transport outcome. Repeated results must match, and inputs over the transport byte limit must be rejected.
- `phase_scope` parses a request/scope pair and calls `asset_authority_scope_digest`. Accepted digests must be deterministic and independent of candidate/scope ordering.

Checked-in seeds cover valid and inconsistent snapshots, duplicate or truncated stream frames, invalid UTF-8, and safe, Unicode, absolute, traversal, separator, empty, and dot path components. All data is synthetic. The fuzz workspace has its own `Cargo.lock`; the product lock is not part of the fuzz dependency graph.

## Reproduce locally

The tool versions used by CI are Rust `nightly-2026-09-12` and `cargo-fuzz 0.13.2`. `cargo-fuzz` needs nightly and LLVM sanitizer support. Keep its installation isolated:

```sh
cargo +nightly-2026-09-12 install cargo-fuzz --version 0.13.2 --locked --root /tmp/agent-router-fuzz-tools
export PATH="/tmp/agent-router-fuzz-tools/bin:$PATH"
cargo +nightly-2026-09-12 metadata --manifest-path fuzz/Cargo.toml --locked --no-deps
cargo +nightly-2026-09-12 fuzz build --fuzz-dir fuzz
cargo +nightly-2026-09-12 fuzz run --fuzz-dir fuzz host_snapshot -- -max_total_time=30 -timeout=3 -max_len=4194305 -rss_limit_mb=1024
cargo +nightly-2026-09-12 fuzz run --fuzz-dir fuzz provider_sse -- -max_total_time=30 -timeout=3 -max_len=4194305 -rss_limit_mb=1024
cargo +nightly-2026-09-12 fuzz run --fuzz-dir fuzz phase_scope -- -max_total_time=30 -timeout=3 -max_len=262144 -rss_limit_mb=1024
```

To replay a saved crash or regression input, pass its file as the corpus directory, for example:

```sh
cargo +nightly-2026-09-12 fuzz run --fuzz-dir fuzz provider_sse fuzz/artifacts/provider_sse/crash-<hash> -- -runs=1
```

The workflow uses three separate 10-second smoke runs and caps each process at 1 GiB RSS. On failure, synthetic crash inputs from `fuzz/artifacts/` are retained as a CI artifact for seven days, accessible according to repository permissions. Both lockfiles are checked even after a failed smoke run. These are bounded CI regression checks, not exhaustive campaigns or evidence about a production runtime. The Rust Fuzz Book describes the [cargo-fuzz CI workflow](https://rust-fuzz.github.io/book/cargo-fuzz/ci.html); pinned tool versions are recorded in [parser-fuzz.yml](../.github/workflows/parser-fuzz.yml).
