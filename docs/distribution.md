# Core and integration source distributions

The separate local native binary export, including its build proof, limits and
reproduction steps, is documented in [binary-distribution.md](binary-distribution.md).

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
python3 scripts/package-core.py --distribution integrations --output /tmp/agent-router-integrations.tar.gz
python3 scripts/package-core.py --verify /tmp/agent-router-integrations.tar.gz
```

The output path must not exist. The exporter never replaces an earlier artifact.
It invokes `cargo metadata --no-deps --locked --offline`, reads the lockfile and
local Cargo cache, and runs Gitleaks when installed. It performs no network call.
It also runs offline Cargo metadata in a temporary copy to project `Cargo.lock`
onto the selected workspace. That projection can only remove dependency nodes
or edges: package versions, sources and registry checksums must remain identical
to the original lockfile. Dependency names are resolved before comparison so
Cargo's unambiguous name abbreviations do not appear as version changes.
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

## Qualify the extracted sources

Archive integrity alone does not prove that its tests compile or pass. Extract
into a fresh directory, then run the checks there with the pinned Rust toolchain
and dependencies already installed locally:

```sh
tar -xzf /tmp/agent-router-core.tar.gz -C /tmp/empty-export-check
cd /tmp/empty-export-check/agent-router-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
```

Create `/tmp/empty-export-check` first. Do not substitute the machine's default
compiler or install a toolchain as part of this offline check. Record the archive
hash, compiler version, commands and exit codes. Ignored platform or native tests
remain unqualified; passing the portable suite grants no native host authority.
The development harness is excluded, so its `xtask` check runs in the source
repository separately.

## Continuous integration license evidence gate

The Ubuntu quality job exports the actual repository with `--strict-licenses`
and independently runs `--verify` on the resulting core source archive. Before
export, `cargo fetch --locked` fills the Cargo cache for all locked dependencies,
including optional and other-platform entries. The fetch step has a five-minute
CI timeout; export and verification together have a separate five-minute timeout.
The exporter itself remains offline. The archive stays in `$RUNNER_TEMP` and is
not uploaded or published.

The gate fails when an external dependency lacks a cached archive, a matching
lockfile checksum, a license declaration or verified license text. Supplemental
texts must match their pinned archive and text hashes. Archive verification
also rejects changed or missing inventory members. The synthetic packaging tests
exercise these failures through the exporter CLI and check that a refused export
creates no output archive.

This checks evidence completeness and byte integrity. License compatibility and
vulnerability review remain separate requirements. The generated SPDX document
still inventories source dependencies across the lockfile; it is not a target-
and feature-specific SBOM for a compiled binary. The separate CI secret scan
does not change those evidence boundaries.

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
own export and conformance checks. `--distribution integrations` creates that
separate source archive; the default remains `--distribution core`.

The archive contains product manifests, Rust source/test/example/benchmark files,
public catalogs and shared test fixtures. It preserves the exact root
`rust-toolchain.toml`; a missing pin refuses the export. External inputs of
selected tests are listed explicitly: synthetic capability support, two inert
adapter descriptors, the handoff prompt and the Linux feedback fixture. This
does not select neighboring integration, prompt or bench files. Missing inputs,
symlinks and sensitive content fail the same source checks.

The root manifest is projected to the included members, retaining workspace
dependency and package settings. Host configuration, the development harness,
global instructions, other integration assets,
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

## Independent integration archive

The integration mode selects members marked `distribution = "integration"`
and public adapter crates under `crates/adapter-*`. It recursively includes all
their local dependency manifests and sources, including optional and test
dependencies needed by Cargo. A dependency on a private or development package
fails the export. Unrelated core members, the CLI and the development harness
are omitted unless a selected integration actually depends on a public member.
The manifest records the selected and excluded packages explicitly.

Public files under `integrations/` include inert host descriptors, protocol
source references, documentation and synthetic conformance fixtures. Entire
directories of excluded workspace members are omitted. Global configuration,
credentials and activation scripts are not selected. Unexpected file types,
symlinks, sensitive paths and content fail the same checks as the core archive.
Shared public fixtures, including `fixtures/examples/`, remain available to
both distributions. An adapter descriptor describes a package; it grants no
permission and installs no hook.

The archive root is `agent-router-integrations/` and its manifest kind is
`integrations-source`. Verification binds the archive prefix to that kind and
rejects mixed roots. License inventory, notices, SBOM, source rechecks and
reproducible archive properties are identical to the core export. To check the
independent workspace after extraction:

```sh
cd agent-router-integrations
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
```

These commands exercise translation and conformance against synthetic streams.
They make no host or provider call. Copilot, OpenCode and HTTP provider runtime
behavior remains unverified until a deployment supplies the corresponding
native executable or authenticated HTTP transport and records a supervised
canary. No credentials, provider installation or global activation is part of
the archive. See `integrations/additional-hosts.md` inside it for exact protocol
sources and remaining runtime evidence.

## License evidence and SBOM scope

`LICENSE-INVENTORY.json` covers all external entries in `Cargo.lock`, including
development, optional and other-platform dependencies. This is a conservative
inventory, not a resolved graph for one target or one feature set.
When offline full metadata is available, each record also distinguishes normal,
build (including procedural macro), and development dependency paths. This scope
is explicitly all selected packages, all features and all targets. A package reachable
only through development paths is not thereby claimed as shipped runtime code;
binary inclusion requires checking the actual build target and feature set.
Original lockfile entries outside the selected workspace are marked
`outside_selected_workspace`; they remain in the conservative license inventory
even when the projected archive lockfile no longer needs them.

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

The core is declared `FSL-1.1-ALv2` by the workspace. `FSL-1.1-ALv2` is on the
SPDX license list but is not OSI approved. The texts are `LICENSE.md` and
`NOTICE` at the repository root, exported in the source archive and in the
binary archive.
Dependency texts retain their own terms. Legal compatibility review, security
review, host runtime verification and publication are separate fields; this
exporter cannot mark them successful.

## Acceptance evidence for issue #8

| Criterion | Evidence required |
| --- | --- |
| Reproducible core source package | Two identical input exports have the same hash; inventory verification passes |
| Offline core independent of release tooling | Build and test the extracted Rust workspace offline; run a routing fixture without Python |
| Independent integration source package | Separate archive kind and workspace dependency closure; extracted conformance tests pass |
| Optional host integrations | Cargo feature/default checks plus independent adapter conformance; live runtime remains a separate gate |
| Public-only content | Boundary tests, reviewed catalog/fixtures, artifact Gitleaks scan and manifest inspection |
| Provenance and notices | Complete lockfile inventory; archive checksums; license/notice hashes; separate compatibility verdict |
| Publication | Explicit release action after license and security gates; never performed by this script |

No success of the source exporter alone closes the host integration or public
release requirements.
