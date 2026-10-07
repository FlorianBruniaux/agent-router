# Homebrew and Cargo qualification, 2026-10-07

Cargo Git installation and a local Homebrew formula pass their installation,
functional smoke, same-version replacement and uninstall checks on macOS ARM64.
Both provide `agent-router` with `claude,codex` and `router-sandbox-helper`.
The repository remains private, with its FSL license unchanged. No public tap,
release asset or crates.io package was published. See [installation](../install.md)
and the [sanitized evidence](distribution-qualification-2026-10-07.json).

## Observed results

| Check | Result | Limit |
| --- | --- | --- |
| Cargo Git installation | Both packages installed from an exact local Git revision, with locked offline dependencies | Local `file://` clone; HTTPS acquisition was not tested |
| Installed commands | Version, route/explain and passive Claude/Codex inspection pass outside the source checkout | Synthetic fixtures; source and Cargo checkouts were hidden during Cargo smoke |
| Cargo lifecycle | Repeat install leaves binaries unchanged; same-pin forced replacement and uninstall pass | Preserves an unrelated marker; no distinct-version upgrade proof |
| Binary export | Diagnostic pair has identical CLI bytes, helper bytes and normalized archive | The first pair failed equality; its cause is still unknown |
| Attestation | Prepare, disposable-key signature, local formula and canonical HTTPS formula rendering pass | Key removed; publisher identity and public asset availability are not established |
| Homebrew lifecycle | Install, functional formula test, reinstall, second test and uninstall all exit 0 | Local archive and custom prefix; system-prefix/public-tap installation was not tested |
| Homebrew preservation | Both binary hashes match the archive; uninstall removes them and retains the marker | Homebrew source/entry-point hashes unchanged; own sandbox remains active |
| Required Rust checks | Formatting, warnings-denied Clippy, workspace tests and harness check pass | 1,594 passed executions, 0 failed, 42 ignored; local results only |
| Packaging checks | 26 source + 17 binary + 27 release tests pass | Includes malformed/omitted helper evidence, platform guards and public URL substitutions |
| Independent review | 36 additional Python probes pass; no confirmed findings in the reviewed diff | Rust review used supplied code/evidence; Rust executions were performed by the parent |

## Exact source and artifact boundary

The installed product source is `f378afad4f584bac0efabbbb9dab37c48db4298f`,
version `0.0.1`, target `aarch64-apple-darwin`. Environment: macOS 26.5.1,
Rust/Cargo 1.85.1, Python 3.11.15 and OpenSSL 3.6.5. Cargo installation uses
its default adapter features; the archive explicitly selects the same adapters.
The binaries from these two build paths are not claimed to be byte-identical.

The source archive is
`dd712f106495a10a04053b6723f63955d7d578e60084bf17c90f53264501e2f0`.
The successful binary archive is
`3f262b1eba3bc745051bb2756ae0025c7ccf496768e8aa40adeec3300d51e988`.
Individual binary, manifest, formula and reviewed-file hashes are in the JSON
record. The archive was built before the final private-diagnostics addition;
that addition changes evidence retention, not product source or build flags.
The real build predates that evidence-retention change.

The exporter reads selected worktree bytes and records HEAD; it does not prove
a clean tree by itself. This export was made before the test-only Rust fix and
contains the unchanged selected source at the baseline above. Distribution
scripts and documentation are not in the core source archive. The later test
cleanup fix and distribution tooling were validated separately. Regenerate
bindings from the accepted release commit before publication, choose a matching
version/tag, and qualify that exact artifact. Existing version text alone does
not authorize attaching new artifacts to an older source tag.

## Two defects exposed by the real checks

**Build equality failure remains unexplained.** The first two successful Cargo
builds produced unequal executable bytes, and the packager emitted no archive.
Its temporary binaries had already been cleaned up, so the differing executable
and byte offsets cannot now be identified. Normalized Cargo events matched.
One diagnostic rerun retained copies before cleanup; both builds in that pair
matched and the resulting archive passed installation. No build flag or equality
check was weakened. This is a successful artifact observation, not a fix for the
first mismatch or a general reproducibility guarantee.

The packager now preserves bounded mode-0600 `run-1.json` and `run-2.json`
reports, including both executable hashes, before comparison. A regression
verifies a helper-only difference leaves both reports but no archive; independent
probes also cover a CLI-only difference. Investigation and release acceptance
remain tracked in [issue #136](https://github.com/FlorianBruniaux/agent-router/issues/136).

**A test helper could hang after a client assertion.** The first full Rust run
stalled in the local API test helper. A stack sample showed scoped cleanup
waiting for an accept loop whose shutdown flag had not been set. The helper
now catches the client panic, signals shutdown, joins the server and propagates
the original panic. A bounded regression failed after two seconds before the
fix; afterward, 36 unit and 10 integration tests passed, followed by the full
workspace gate. No production Rust behavior changed. The original assertion
that triggered the hang remains unknown; ten isolated repetitions of that
older test passed, so no speculative slot-race change was made.

## Homebrew environment and replay

The earlier [core-only experiment](release-qualification-2026-10-07.md) remains
historical. This trial copied Homebrew source commit
`a57af195cf9d7addb48bdb1204c9151313cd0057` and portable Ruby 4.0.7 into a private
prefix outside `/tmp`. Homebrew's own sandbox ran without an outer Seatbelt
wrapper or any sandbox-disabling option. No Homebrew source was patched.
Homebrew identifies this custom-prefix setup as Tier 3; it is not validation
of the normal system prefix.

A prefix beneath the system temporary directory failed Homebrew's inherited
sandbox write check. Moving the prefix outside that directory allowed the
installation. Uninstall also required the ordinary Homebrew Core catalog:
automatic full-history fetches were stopped, then a shallow checkout of commit
`4f9eb5afc65d3af7f0205d77ec94fdba66c1e739` was placed in the isolated Core tap.
A complete final lifecycle then passed. Cargo dependency acquisition stayed
offline; Homebrew catalog provisioning used the network. A misspelled Codex
inspection flag in the test driver was corrected to `--model-cache`; the final
inspection passed without changing product code.

The test used a provisioned local tap named `qualification/local`, the rendered
hash-pinned `file://` formula and explicit paths to the isolated `brew` binary:

```sh
"$brew_bin" install --formula qualification/local/agent-router
"$brew_bin" test qualification/local/agent-router
"$brew_bin" reinstall --formula qualification/local/agent-router
"$brew_bin" test qualification/local/agent-router
"$brew_bin" uninstall --formula qualification/local/agent-router
```

Auto-update, API installation, analytics, install cleanup and autoremove were
disabled through Homebrew's documented environment options. Cache, logs and
temporary/config data used private test paths. The formula test exercises an
inline synthetic route/explain case and checks that an argument-free helper
returns 125 with `sandbox-helper.arguments`. These checks never launch a host
or grant native execution permissions.

The final observed install/reinstall/uninstall durations were 3.137, 2.706 and
0.715 seconds. They describe this local archive and warm environment, not a
fresh-download benchmark or an estimate of engineering work.

## Remaining publication acceptance

[Issue #8](https://github.com/FlorianBruniaux/agent-router/issues/8) remains open.
Resolve the first-build mismatch or explicitly accept its bounded release
limitation, select the publication license and publisher key, and regenerate
artifacts at the chosen commit/version. Public HTTPS retrieval, a public tap,
a distinct-version upgrade and other platforms each need their own evidence.
Automatic daily integration, native provider success and productivity/cost
benefits were not tested by this distribution work.
