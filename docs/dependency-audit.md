# Rust dependency security audit

The dedicated `Rust dependency security audit` workflow checks the repository's
committed `Cargo.lock` against the RustSec Advisory Database on pull requests,
pushes to `main` and `codex/**`, a weekly schedule, and manual dispatch. The
weekly run detects advisories added after the last code change.

It installs `cargo-audit` 0.22.2 using Rust 1.88.0 in the CI job only. That
version requires Rust 1.88, while the product toolchain remains pinned to Rust
1.85. The tool is installed under the runner's temporary directory and invoked
by its explicit path. A fresh `CARGO_HOME` and explicit `--db` path prevent
reuse of a previously cached advisory database. The workflow passes
`--file Cargo.lock` so the audit reads the checked-in lockfile directly, and
`--deny warnings` makes all RustSec warnings fail the job. No advisory
exceptions or alternate audit policy are configured.

The job prints the installed tool version, the audited lockfile SHA-256, and
the advisory database Git commit after a successful audit. It checks that the
lockfile hash did not change during the run. The job has read-only repository
permissions, uses a full commit SHA for `actions/checkout`, and has a 20-minute
timeout. It does not create issues, checks through a write-capable token, or
modify dependency files.

## Evidence boundary

Local workflow validation does not establish the advisory status of this lock.
Only a successful run of this workflow against the intended commit can provide
that result. The advisory database is fetched by `cargo-audit` during CI, so a
network or database-fetch failure also fails the job and is not a clean audit.

References:

- [RustSec cargo-audit 0.22.2 release](https://github.com/RustSec/rustsec/releases/tag/cargo-audit/v0.22.2)
- [cargo-audit 0.22.2 manifest](https://github.com/RustSec/rustsec/blob/cargo-audit/v0.22.2/cargo-audit/Cargo.toml)
- [RustSec Advisory Database](https://github.com/RustSec/advisory-db)
- [GitHub Actions secure use reference](https://docs.github.com/en/actions/reference/security/secure-use)
