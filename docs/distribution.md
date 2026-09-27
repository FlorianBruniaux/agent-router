# Core source distribution

`scripts/package-core.py` exports the Rust workspace sources, a file integrity
manifest, an SPDX 2.3 SBOM, a license inventory and dependency notices. It uses
Python 3.11 or newer as release tooling. The Rust library and CLI do not invoke
Python, download a semantic backend or install a host integration.

## Build and verify an archive

Run from the repository root, with Cargo dependencies already available locally:

```sh
python3 scripts/test-package-core.py
python3 scripts/package-core.py --output /tmp/agent-router-core.tar.gz
python3 scripts/package-core.py --verify /tmp/agent-router-core.tar.gz
```

The output path must not exist. The exporter never replaces an earlier artifact.
It invokes `cargo metadata --no-deps --locked --offline`, reads the lockfile and
local Cargo cache, and runs Gitleaks when installed. It performs no network call.
Use `--strict-licenses --require-gitleaks` to require all dependency declarations
and license texts to be available and the additional secret scan to pass.

Verification checks every archived file against `DISTRIBUTION.json`, rejects
unlisted files, duplicates, links, traversal and sensitive path/content patterns,
and prints the archive SHA-256. A digest proves byte integrity against the
included inventory. It is not a publisher signature or independent trust proof.

For identical source files, Git commit and verified Cargo cache evidence, archive
bytes are identical. Member order, archive times, owners, permissions and gzip
headers are fixed. The SPDX creation date comes from the source commit.
Source files are checked again before publication; a concurrent source change
aborts the export. A verified temporary archive is linked into place atomically.

## Workspace boundaries

Cargo metadata discovers product members under `crates/`; no fixed crate list is
maintained. `xtask` and members declaring one of the following metadata values
are excluded from the core source archive:

```toml
[package.metadata.agent-router]
distribution = "integration" # also: development, private
```

`distribution = "core"` is accepted for a product crate. Unclassified members
outside `crates/` fail packaging. A core package depending on an excluded local
member also fails, even when that dependency is optional: Cargo still needs its
manifest. Integration packages should depend on the public core. They need their
own export and conformance checks before their distribution can be claimed.

The archive contains product manifests, Rust source/test/example/benchmark files,
public catalogs and shared test fixtures. The root manifest is projected to the
included members, retaining workspace dependency and package settings. Host
configuration, the development harness, global instructions, integration assets,
private policy packs, telemetry, databases and environment files are excluded.
Unexpected file types, symlinks, known credential formats or machine home paths
in selected inputs fail packaging. Only paths and rule identifiers appear in
diagnostics; matching sensitive values are not printed.

These conservative detectors do not prove the absence of secrets or establish
whether arbitrary prose is private. Public catalog and fixture contents still
require review. An organization keeps its catalogs, policies and telemetry out
of the public workspace and supplies them through explicit local inputs.

Rust adapter crates under `crates/` remain independent packages in this source
archive, alongside the provider-neutral core. The CLI's host features are
recorded in the manifest. Build its core mode explicitly without host features:

```sh
cargo test -p agent-router-cli --no-default-features --locked --offline
cargo build -p agent-router-cli --no-default-features --locked --offline
```

This source layout preserves optional dependency manifests required by Cargo.
It does not bundle host executables or automatically activate an integration.

## License evidence and SBOM scope

`LICENSE-INVENTORY.json` covers all external entries in `Cargo.lock`, including
development, optional and other-platform dependencies. This is a conservative
inventory, not a resolved graph for one target or one feature set.
When offline full metadata is available, each record also distinguishes normal,
build (including procedural macro), and development dependency paths. This scope
is explicitly all core packages, all features and all targets. A package reachable
only through development paths is not thereby claimed as shipped runtime code;
binary inclusion requires checking the actual build target and feature set.

For each cached `.crate` archive, the exporter checks its SHA-256 against the
lockfile before reading its package manifest and license/notice files. Original
license texts and attribution are included under `licenses/dependencies/` with
their individual hashes. A missing archive, changed checksum, missing license
text or absent declaration remains an explicit incomplete evidence record.
Missing Windows-only cache entries do not disappear from the inventory and do
not imply that the current platform uses those packages.

Some upstreams keep license terms in `AUTHORS`; these notices are collected too.
For texts omitted from a crate archive, `licenses/third-party/evidence.json`
records a pinned upstream URL, exact crate checksum, source correspondence and
text hash. The exporter validates those bindings before including the supplement.
For `difflib` 0.4.0, every packaged Rust file and the original Cargo manifest match
Git blobs at `f035fb8e656f27119e23eca9d5b996df14f3885e`; its upstream MIT text is
preserved unchanged, including the upstream copyright attribution.

The SBOM records upstream declarations as `licenseDeclared`; `licenseConcluded`
stays `NOASSERTION`. A checksum establishes provenance of the inspected bytes,
not license compatibility. The generated notices state the same boundary.

The core is declared `MIT OR Apache-2.0` by the workspace. Corresponding texts
are supplied in `licenses/`. Dependency texts retain their own terms. Legal
compatibility review, security review, host runtime verification and publication
are separate fields; this exporter cannot mark them successful.

## Acceptance evidence for issue #8

| Criterion | Evidence required |
| --- | --- |
| Reproducible core source package | Two identical input exports have the same hash; inventory verification passes |
| Offline core independent of release tooling | Build and test the extracted Rust workspace offline; run a routing fixture without Python |
| Optional host integrations | Cargo feature/default checks plus independent adapter/package conformance, maintained by integration owners |
| Public-only content | Boundary tests, reviewed catalog/fixtures, artifact Gitleaks scan and manifest inspection |
| Provenance and notices | Complete lockfile inventory; archive checksums; license/notice hashes; separate compatibility verdict |
| Publication | Explicit release action after license and security gates; never performed by this script |

No success of the source exporter alone closes the host integration or public
release requirements.
