# Local native binary export

`scripts/package-binary.py` builds the native `agent-router` CLI and
`router-sandbox-helper` from a verified
core-source archive. It requires an externally supplied SHA-256, a complete
source archive license inventory and locally cached `.crate` archives matching
the projected `Cargo.lock`. It makes two clean release builds with Rust 1.85.1,
`--locked --offline --no-default-features --features agent-router-cli/claude,agent-router-cli/codex`,
and an explicit target matching the native Rust host. It writes
the local archive only when both executable hash maps and normalized archive bytes match
across both builds. Passing this check establishes reproducibility for that
toolchain, target and input archive, not a general reproducibility guarantee.

The v2 build recipe, observations and run report carry both binary identities
and hashes. The CLI's default feature set is exactly `claude` and `codex`; the
helper has no features. The v2 release attestation refuses old single-binary
archives. See the [current distribution qualification](bench/distribution-qualification-2026-10-07.md)
for the final artifact and installation results. The
[earlier core-only qualification](bench/release-qualification-2026-10-07.md) is
historical evidence for a different archive contract.

In the current qualification, the first real pair differed and archive creation
was refused. A subsequent diagnostic pair matched for both executables and
produced the candidate installed locally. The first mismatch's cause remains
`UNKNOWN`; the passing pair does not establish that it was fixed. The artifact
binds source revision `f378afad4f584bac0efabbbb9dab37c48db4298f`, before the
distribution-tooling and test-only corrections; accepted revisions require fresh
build and release bindings.

## Prepare and build

Create a source archive from the chosen core checkout. Keep its digest separate
from the script invocation and compare it through the trusted channel used to
approve that source snapshot:

```sh
python3.11 scripts/package-core.py \
  --strict-licenses \
  --output /private/tmp/agent-router-core.tar.gz
shasum -a 256 /private/tmp/agent-router-core.tar.gz
```

Copy the verified digest as the required value below. The example uses the
native Apple Silicon target. Cross-compilation is refused: another target
requires running this procedure on a matching native host with its Rust and
C/SDK toolchain installed.

```sh
python3.11 scripts/test-package-binary.py
python3.11 scripts/package-binary.py \
  --source-archive /private/tmp/agent-router-core.tar.gz \
  --source-sha256 <independently-verified-sha256> \
  --target aarch64-apple-darwin \
  --output /private/tmp/agent-router-aarch64-apple-darwin.tar.gz \
  --diagnostics-dir /private/tmp/agent-router-binary-diagnostics
```

The output archive and diagnostics directory must not already exist. The script
does not download dependencies. It creates a fresh private vendor tree from
the exact checksum-matching `.crate` archives in Cargo's local cache, with
Cargo checksum manifests, plus a fresh target directory and Cargo home for each
build. Cargo's JSON event stream and stderr are kept in mode-0700 diagnostics
storage; logs are streamed with combined byte, line-length, event-count and
time bounds. The process group is stopped and reaped immediately when a bound
is crossed. Rust source and
vendor paths are remapped to stable virtual paths for both builds. Do not publish
those raw logs because they can contain local paths. The JSON run report binds
their hashes to the source, build observations, both binaries and archive.
Each completed build also writes a mode-`0600` `run-1.json` or `run-2.json` in
that directory before the reproducibility comparison, retaining both binary
hashes even when a mismatch refuses the archive. Retain these reports for diagnosis.

Before invoking Cargo, the script refuses `.cargo/config` or
`.cargo/config.toml` in the current directory ancestry or output ancestry,
including symlinked files and directories. It uses a private Cargo home with a
generated vendor configuration and does not inherit Cargo environment flags.

Build scripts run offline with Cargo, but this does not sandbox them from the
network or other host resources. No claim of network isolation is made. The
script does not sign, publish, install, register a host integration or alter
global configuration. It exports the same default product components as
the [pinned Cargo installation](install.md#cargo-from-a-pinned-git-revision).
Optional adapter/API/sidecar features are separate source-build choices.

After verifying both archives, extract them to separate temporary
directories, then invoke the packaged executable with the bundled example
fixtures from the source archive:

```sh
extract_dir=$(mktemp -d)
mkdir "$extract_dir/binary" "$extract_dir/source"
tar -xzf /private/tmp/agent-router-aarch64-apple-darwin.tar.gz -C "$extract_dir/binary"
tar -xzf /private/tmp/agent-router-core.tar.gz -C "$extract_dir/source"
"$extract_dir/binary/agent-router-binary/bin/agent-router" route \
  --request "$extract_dir/source/agent-router-core/tests/fixtures/requests/codex-implementation.json" \
  --catalog "$extract_dir/source/agent-router-core/assets/catalogs/codex-core-v1.json" \
  --json > "$extract_dir/decision.json"
"$extract_dir/binary/agent-router-binary/bin/agent-router" explain \
  --decision "$extract_dir/decision.json"
```

This prints a routing decision from the extracted native binary against synthetic
local fixtures. It does not exercise a provider, host integration, sandbox, or
live service. Compare its plan with the [installation smoke example](install.md#verify-the-installation).
The extracted files remain in `extract_dir` for inspection; remove only this
owned directory after checking for changes or files you want to keep.
The bundled `fixtures/examples/phase-snapshot.json` is a library
fixture with unauthenticated capability declarations; using it with `route-phase`
is refused by the CLI with `capability.evidence.unauthenticated`. Do not turn
those declarations into trusted evidence to make an example pass.

## Included evidence and limits

The binary archive contains both executables (`0755`), notices and all verified
third-party license texts (`0644`), the source license inventory and source
SBOM, a stable build recipe, an integrity manifest, normalized build
observations and `BUILD-SBOM.json`. The source SBOM and build SBOM remain
separate. The build SBOM lists
packages seen in Cargo `compiler-artifact` events and maps each to the exact
lockfile checksum and source-archive license record, including paths and hashes
for the included license texts. Archive verification checks each such link. It
does not claim an exact
compilation-unit graph or prove static inclusion. License conclusions remain
`NOASSERTION`.

The report records `binaries_sha256` for both executables, hashes and versions
for Cargo and rustc, plus available
Clang and Apple SDK version output. On Apple targets it also records the names
reported by `otool -L`. That lists declared dynamic dependencies only; it does
not prove future loads or statically included libraries.

The source snapshot determines the bundled SQLite version. A WAL-reset issue
was flagged in an earlier SQLite version; a dependency update alone does not
constitute a general security clearance. This export marks security review
`UNKNOWN`; assess the exact source archive and resulting binary separately.

Archive verification rejects links, path traversal, extra members, unexpected
modes and changed contents. Archive creation is atomic and refuses to replace
an existing path. Failures leave no binary archive; bounded diagnostics may
remain in the explicitly named private directory for investigation.

To bind such an archive to a signed manifest, see
[release-local.md](release-local.md).
