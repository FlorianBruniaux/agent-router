# Local release attestation

`scripts/release-local.py` binds a core-source archive and one or more local
binary archives (see [binary-distribution.md](binary-distribution.md)) to a
canonical manifest, then verifies an externally produced signature over that
manifest and renders a Homebrew formula. It never generates a key, signs,
publishes, installs or touches a global Homebrew prefix.

A binary built before the latest fixes is not a final release. Nothing here
marks a candidate as final: the manifest records what was built from which
source, nothing more.

The v2 recipe binds `agent-router` with exactly `claude` and `codex` features
and `router-sandbox-helper` without features. Each artifact record binds both
binary hashes and both clean-build hash maps. Old v1 manifests and single-binary
archives are refused; regenerate the archive, manifest and external signature.
This format change is separate from the product version, which remains `0.0.1`.

The [current distribution qualification](bench/distribution-qualification-2026-10-07.md)
records the v2 build and installation observations. The
[earlier core-only qualification](bench/release-qualification-2026-10-07.md) remains
historical and does not qualify this complete archive.

## Signature tool

`openssl` 3.6.5 at `/opt/homebrew/bin/openssl`, Ed25519 raw signatures over
`agent-router-release-v2\0` followed by the manifest bytes. `ssh-keygen -Y` was
not chosen because it needs an allowed-signers file and namespace handling;
`minisign` is not installed here. The verifier requires the exact version token
`OpenSSL 3.6.5` (so `3.6.4`, `3.6.50` or `3.6` are refused).

OpenSSL's output decides the verdict: exit 0 with `Signature Verified
Successfully` is accepted, exit 1 with `Signature Verification Failure` is
`release.signature_invalid`, and any other output or exit code is
`release.openssl_failed`. This relies on OpenSSL 3.6.5's message text, which is
why the version is pinned exactly.

## Steps

1. Prepare the manifest and signing bytes. The output directory must not exist.

```sh
python3 scripts/release-local.py prepare \
  --source-archive "$TMPDIR/agent-router-core.tar.gz" \
  --source-sha256 <independently-verified-sha256> \
  --source-commit <40-hex-commit> \
  --candidate "$TMPDIR/agent-router-aarch64-apple-darwin.tar.gz" \
  --output-dir "$TMPDIR/release-prepared"
```

2. Sign `SIGNING-MESSAGE.bin` outside this repository with a key you hold:

```sh
openssl pkeyutl -sign -rawin -inkey <private-key.pem> \
  -in "$TMPDIR/release-prepared/SIGNING-MESSAGE.bin" -out "$TMPDIR/MANIFEST.sig"
openssl pkey -in <private-key.pem> -pubout -outform DER -out "$TMPDIR/release-public.der"
```

3. Verify with pins obtained through a separate trusted channel: manifest
   SHA-256, public key SHA-256, source commit, source archive SHA-256 and the
   candidate archive SHA-256. The formula output must not exist.

If the manifest pin and the public key pin come from the same channel as the
manifest and key themselves, the signature adds only the link between that
manifest and that key. Independent channels are what make the pins mean
something.

`verify` requires every candidate listed in the manifest, no more and no fewer
(`release.candidate_set_mismatch` otherwise), even though it renders one formula.

```sh
python3 scripts/release-local.py verify \
  --source-archive "$TMPDIR/agent-router-core.tar.gz" --source-sha256 <sha256> \
  --expected-source-commit <40-hex-commit> \
  --manifest "$TMPDIR/release-prepared/MANIFEST.json" --signature "$TMPDIR/MANIFEST.sig" \
  --public-key "$TMPDIR/release-public.der" --public-key-sha256 <sha256> \
  --expected-manifest-sha256 <sha256> --expected-candidate-sha256 <sha256> \
  --candidate "$TMPDIR/agent-router-aarch64-apple-darwin.tar.gz" \
  --target aarch64-apple-darwin --features-json '["claude","codex"]' \
  --formula-output "$TMPDIR/agent-router.rb"
```

The signed formula hash covers a formula rendered with a neutral archive URL, so
moving a candidate between `prepare` and `verify` is allowed. The selected
archive URL is injected only into the written formula.

Output is one JSON line. Failures print `{"error": "<code>", "ok": false}`
with no key material and write no formula.

## Archive URL and publication

Without `--archive-url`, `verify` uses the local candidate's canonical `file://`
URL. An opt-in public URL must match this exact shape, using the source version
and the local candidate's basename:

```text
https://github.com/FlorianBruniaux/agent-router/releases/download/v{version}/{archivebasename}
```

For the `0.0.1` source and candidate filename in the command above, append this
argument to `verify`, with a new formula output path:

```sh
--archive-url https://github.com/FlorianBruniaux/agent-router/releases/download/v0.0.1/agent-router-aarch64-apple-darwin.tar.gz
```

This is a rendering example, not a claim that this asset exists. The command
makes no HTTP request, uploads nothing and changes no repository visibility.
The signature binds archive content and the neutral formula, not availability
or transport origin. A published asset and a public tap require their own
authorized delivery and verification.

The formula installs both binaries and refuses an OS/CPU mismatch. Its test
creates synthetic request/catalog files and exercises route/explain plus the
helper's argument refusal. It proves neither host authentication nor native
provider execution. Configure the helper's absolute path and review its hash
separately before any supervised run ([installation](install.md)).

## Tests

```sh
python3 scripts/test-release-local.py
```

Each test generates a throwaway Ed25519 key in the temporary directory and
asserts the exact error code. Covered rejections: altered manifest with an
external pin, altered manifest with a recomputed pin (signature check), altered
archive, changed formula template, substituted public key with a matching pin,
wrong pin, signature over another message, existing formula output, wrong
candidate or source pin, truncated and malformed candidate archives (JSON error,
no traceback, no path), inexact OpenSSL version, unrecognised OpenSSL verdict
a missing candidate, and multiline toolchain observations with bounded full-output
validation. The manifest retains each first line and the hash of the complete
observations. Replacing the signature verdict with acceptance makes
five of them fail. V2 coverage additionally refuses missing/substituted binary
evidence, single-binary archives, wrong features and platform targets, and
noncanonical archive URLs; it checks the complete local/public formula shapes.

## Limits

- The complete v2 archive passed Homebrew install, test, same-version reinstall,
  retest and uninstall on macOS ARM64 with both binary hashes verified and the
  unrelated marker preserved. Its custom non-`/tmp` prefix was classified Tier 3;
  Homebrew's own sandbox remained enabled, without an outer sandbox or override.
  Ordinary core catalog files were supplied from a shallow pinned copy. This
  qualifies only the exact local artifact and environment in the current report,
  not a default prefix, public tap, HTTPS retrieval or different-version upgrade.
- The first real two-build pair mismatched and the archive was refused. A
  subsequent pair matched for both executables and produced the qualified
  archive. The initial mismatch's cause remains `UNKNOWN`; retained per-build
  hashes improve diagnosis without establishing a fix or general reproducibility.
  Source bindings identify `f378afad4f584bac0efabbbb9dab37c48db4298f`, before the
  distribution-tooling and test-only edits. Regenerate release bindings after
  accepting the changed revision.
- Symlinks: `prepare` and `verify` reject a symlinked candidate, template,
  manifest or output leaf, and a symlinked output parent. They do not reject
  symlinked ancestors further up. Rejecting every component would refuse
  `/tmp` and `$TMPDIR` on macOS, which are symlinks. Paths are resolved once for
  the formula URL and then used as given, so a directory swapped by a
  concurrent local writer between checks and use is not defended against.
- `scripts/package-binary.py` checks the source archive path and size before
  reading it (check then use). This guard is separate from signature verification.
- The signature proves only that the holder of the pinned key signed these
  bytes. Key custody and rotation are outside this tool.
- Unit-test archives are synthetic. Real build and installation observations
  apply to the exact artifacts in the current qualification report; a fixture
  signature establishes neither publisher identity nor release authority.
