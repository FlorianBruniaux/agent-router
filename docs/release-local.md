# Local release attestation

`scripts/release-local.py` binds a core-source archive and one or more local
binary archives (see [binary-distribution.md](binary-distribution.md)) to a
canonical manifest, then verifies an externally produced signature over that
manifest and renders a Homebrew formula. It never generates a key, signs,
publishes, installs or touches a global Homebrew prefix.

A binary built before the latest fixes is not a final release. Nothing here
marks a candidate as final: the manifest records what was built from which
source, nothing more.

The [2026-10-07 qualification](bench/release-qualification-2026-10-07.md)
exercises real core archives with a disposable signing key and records the
remaining Homebrew, publication and publisher-trust limits.

## Signature tool

`openssl` 3.6.5 at `/opt/homebrew/bin/openssl`, Ed25519 raw signatures over
`agent-router-release-v1\0` followed by the manifest bytes. `ssh-keygen -Y` was
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
  --candidate "$TMPDIR/agent-router-aarch64-apple-darwin-core.tar.gz" \
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
  --candidate "$TMPDIR/agent-router-aarch64-apple-darwin-core.tar.gz" \
  --target aarch64-apple-darwin --features-json '[]' \
  --formula-output "$TMPDIR/agent-router.rb"
```

The signed formula hash covers a formula rendered with a neutral archive URL, so
moving a candidate between `prepare` and `verify` is allowed. The real `file://`
URL of the candidate is injected only into the written formula.

Output is one JSON line. Failures print `{"error": "<code>", "ok": false}`
with no key material and write no formula.

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
five of them fail.

## Limits

- Homebrew installation remains unqualified. On 2026-10-07, an isolated copy
  recognized the local tap and fetched the exact hash-pinned `file://` archive.
  Installation then failed on nested sandboxing; the supported opt-out refused
  the account's administrator group. No Homebrew safety check was changed.
  The install and test blocks remain unverified; see the qualification report.
  The manual archive's route/explain and controlled removal did pass.
- Symlinks: `prepare` and `verify` reject a symlinked candidate, template,
  manifest or output leaf, and a symlinked output parent. They do not reject
  symlinked ancestors further up. Rejecting every component would refuse
  `/tmp` and `$TMPDIR` on macOS, which are symlinks. Paths are resolved once for
  the formula URL and then used as given, so a directory swapped by a
  concurrent local writer between checks and use is not defended against.
- `scripts/package-binary.py` checks the source archive path and size before
  reading it (check then use). That is existing code outside this tool and is
  unchanged.
- The signature proves only that the holder of the pinned key signed these
  bytes. Key custody and rotation are outside this tool.
- Unit-test archives are synthetic. The separate 2026-10-07 qualification
  verifies two identical real builds and a fixture-key attestation; it does
  not establish publisher identity or release authority.
