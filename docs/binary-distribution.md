# Local core binary export

`scripts/package-binary.py` builds one native `agent-router` CLI from a verified
core-source archive. It requires an externally supplied SHA-256, a complete
source archive license inventory and locally cached `.crate` archives matching
the projected `Cargo.lock`. It makes two clean release builds with Rust 1.85.1,
`--locked --offline --no-default-features`, and an explicit target. It writes
the local archive only when the binary bytes and normalized archive bytes match
across both builds. This is a measured result for that toolchain, target and
input archive, not a general reproducibility guarantee.

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
native Apple Silicon target; set another explicit target only when its Rust
target and native C/SDK toolchain are already installed locally.

```sh
python3.11 scripts/test-package-binary.py
python3.11 scripts/package-binary.py \
  --source-archive /private/tmp/agent-router-core.tar.gz \
  --source-sha256 <independently-verified-sha256> \
  --target aarch64-apple-darwin \
  --output /private/tmp/agent-router-binary.tar.gz \
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
their hashes to the source, build observations, binary and archive.

Before invoking Cargo, the script refuses `.cargo/config` or
`.cargo/config.toml` in the current directory ancestry or output ancestry,
including symlinked files and directories. It uses a private Cargo home with a
generated vendor configuration and does not inherit Cargo environment flags.

Build scripts run offline with Cargo, but this does not sandbox them from the
network or other host resources. No claim of network isolation is made. The
script does not sign, publish, install, register a host adapter or alter global
configuration. It exports the provider-neutral CLI without default adapter
features.

To exercise the extracted binary, extract both archives to separate temporary
directories, then invoke the packaged executable with the public example
fixtures from the source archive:

```sh
tar -xzf /private/tmp/agent-router-binary.tar.gz -C /private/tmp
tar -xzf /private/tmp/agent-router-core.tar.gz -C /private/tmp
/private/tmp/agent-router-binary/bin/agent-router route-phase \
  --context /private/tmp/agent-router-core/fixtures/examples/phase-request.json \
  --host-snapshot /private/tmp/agent-router-core/fixtures/examples/phase-snapshot.json \
  --catalog /private/tmp/agent-router-core/fixtures/examples/phase-catalog.json \
  --now 100 --json
```

This verifies routing from the extracted native binary against synthetic local
fixtures. It does not exercise a provider, host integration, sandbox, or live
service.

## Included evidence and limits

The binary archive contains the executable (`0755`), notices and all verified
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

The report records hashes and versions for Cargo and rustc, plus available
Clang and Apple SDK version output. On Apple targets it also records the names
reported by `otool -L`. That lists declared dynamic dependencies only; it does
not prove future loads or statically included libraries.

The source snapshot determines the bundled SQLite version. A WAL-reset issue
was flagged in an earlier SQLite version; a dependency update alone does not
constitute a general security clearance. This export marks security review
`UNKNOWN`; assess the exact source archive and resulting binary separately.

Archive extraction rejects links, path traversal, extra members, unexpected
modes and changed contents. Archive creation is atomic and refuses to replace
an existing path. Failures leave no binary archive; bounded diagnostics may
remain in the explicitly named private directory for investigation.

To bind such an archive to a signed manifest, see
[release-local.md](release-local.md).
