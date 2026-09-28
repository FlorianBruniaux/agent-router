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

Workflow syntax validation does not establish the advisory status of a lockfile.
An actual audit must identify the tool, unchanged lockfile digest and advisory
revision. A local audit is separate evidence from a successful remote workflow.
A network or database-fetch failure fails the job and is not a clean audit.

## Local integration audit, September 28

The integration candidate's two lockfiles were audited with `cargo-audit` 0.22.2,
installed using Rust 1.88.0 under an isolated temporary directory. The workspace
audit fetched a fresh database at `ef03605143a913024f864d2edf476adad5720c93`
(1,273 advisories); the fuzz audit reused that exact checkout without fetching.
Both used `--deny warnings`, with no ignored advisories, and exited successfully:

| Lockfile | SHA-256 | Dependencies | Vulnerabilities / warnings |
| --- | --- | ---: | --- |
| `Cargo.lock` | `965bfffa58676c98f141fa2c84b247936b5389423a2d5b5d6f1aa2e6de09ef8a` | 169 | 0 / 0 |
| `fuzz/Cargo.lock` | `0aeb8d93d4744037b84157efc19788d46ee2b7fe39be921231f06888d50ea0e9` | 67 | 0 / 0 |

Both file digests were unchanged afterward. This establishes the local audit
result for those locks and that advisory revision, not an absence of future
vulnerabilities or remote CI success. No global toolchain configuration changed.

References:

- [RustSec cargo-audit 0.22.2 release](https://github.com/RustSec/rustsec/releases/tag/cargo-audit/v0.22.2)
- [cargo-audit 0.22.2 manifest](https://github.com/RustSec/rustsec/blob/cargo-audit/v0.22.2/cargo-audit/Cargo.toml)
- [RustSec Advisory Database](https://github.com/RustSec/advisory-db)
- [GitHub Actions secure use reference](https://docs.github.com/en/actions/reference/security/secure-use)
