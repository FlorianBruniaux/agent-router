# Experimental release qualification, 2026-10-07

Local qualification for [issue #8](https://github.com/FlorianBruniaux/agent-router/issues/8).
The core archive installs by extraction, computes the documented offline decision
and can be removed while preserving an existing file. Homebrew installation is
**not qualified**. Publication and publisher identity are **not approved** by
these checks. Start with the [offline onboarding](../experimental-release.md).

## Exact scope

The source baseline is `882a246236de5864bee5a2f0f9771e03438c6a46`.
This qualification ran in a dedicated worktree; no change was made to the primary
checkout, global configuration, repository visibility or the FSL license.
Local script and documentation corrections are reviewed separately from that
baseline. Their hashes and the measured artifacts are in the
[sanitized evidence](release-qualification-2026-10-07.json).

The core exporter reads selected **worktree bytes**, not a Git archive. It
records HEAD but does not establish that the tree is clean. Here, selected core
inputs were unchanged, and three source exports, including one after the
tooling correction, were byte-identical. The core archive excludes `scripts/`
and `docs/` and generates its own README. The corrected release script and new
onboarding are therefore not contained in that archive. Regenerate and verify
release bindings after accepting a new revision or changing the license.

Environment: macOS 26.5.1 (25F80), Apple Silicon; Rust and Cargo 1.85.1;
Python 3.11.15; OpenSSL 3.6.5; Gitleaks 8.30.1. The binary target is
`aarch64-apple-darwin`, version `0.0.1`, with no adapter features enabled.
This is not a fresh Linux, Windows or native-provider qualification.

## Results

| Check | Observed result | Boundary |
| --- | --- | --- |
| Rust formatting, warnings-denied Clippy, workspace tests, harness check | All four commands exit 0; 1,593 passed test executions, 0 failed, 42 ignored | Local workspace/all-features; ignored tests were not run in this qualification |
| Source, binary and release Python suites | 26 + 12 + 19 = 57 tests passed | Release regression was first reproduced failing |
| Independent release-fix review | 19 tests rerun plus 3 independent test methods passed | Includes 20 refusal subcases over both version fields, exact byte bound and full-observation hash binding |
| Source export | 810 files; three byte-identical exports | Strict license evidence and configured secret scan passed; legal compatibility remains separate |
| Native core binary | Two clean builds produce identical executable and normalized archive bytes | One toolchain, target and source archive; no general reproducibility guarantee |
| Extracted binary | `route` and `explain` both exit 0 under an external network-denying sandbox | Bundled synthetic request/catalog; no provider or native host execution |
| Removal | 290 archive files removed after hash/mode comparison; existing marker retained; executable absent | Extraction prefix only, not a Homebrew uninstall or secure erasure claim |
| Manifest attestation | Real archive prepare, fixture signature and verify exit 0 | Disposable test key removed; publisher trust is not established |
| Homebrew | Local tap recognized; exact `file://` archive fetched and hash matched | Install failed in the constrained environment; test and uninstall remain unqualified |

The required commands, run from the repository root through RTK, were:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo run -p xtask --locked -- check
python3.11 scripts/test-package-core.py
python3.11 scripts/test-package-binary.py
python3.11 scripts/test-release-local.py
```

Socket tests needed execution outside the agent command sandbox. This changes
where the checks ran, not native product permissions. No Rust source changed
after these gates; the Python correction was checked by its targeted suite and
an independent reviewer. No remote CI result is implied.

## Defects reproduced and corrected

1. **Real toolchain observations refused.** The binary packager records full
   `cargo -Vv` and `rustc -Vv` output. The release verifier rejected their line
   feeds as `release.toolchain_version_invalid`. It now validates the entire
   bounded output, retains the first line as the manifest label, and continues
   to bind the complete observations by SHA-256. Empty first lines, NUL, CR,
   private path prefixes and oversized input remain refused. The new roundtrip
   failed before the fix and passes after it.
2. **Published binary example refused.** The previous `route-phase` example
   supplied an unauthenticated library snapshot and exits 2 with
   `capability.evidence.unauthenticated`. The guide now uses the shipped legacy
   `route` request/catalog that was actually tested. Capability authentication
   was not weakened.
3. **Current document privacy.** One document's actual owner worktree path was
   replaced with a placeholder. Eight historical blob versions still contain
   the path. It is an identifying local path, not a credential; a current-file
   edit does not remove historical exposure.

## Homebrew experiment and limit

A read-only archive of the installed Homebrew source at
`a57af195cf9d7addb48bdb1204c9151313cd0057` and its portable Ruby 4.0.7 were copied
into a temporary prefix. No remote tap or package was installed globally.
The formula used the hash-pinned local core binary archive.

Homebrew requires a tap, so the generated formula was placed in a local tap.
The constrained trial then exposed a denied pseudo-terminal and a denied Unix
socket. Only terminal devices and sockets inside the temporary test prefix
were added to the outer sandbox rules. IP connections and writes outside that
prefix still failed their negative controls.

The remaining failure was `sandbox_apply: Operation not permitted` from a
nested sandbox. Homebrew's documented `HOMEBREW_AVOID_NESTED_SANDBOXING` option
also refused the attempt because the account belongs to the `admin` group.
No Homebrew source or privilege check was patched. The installed command and
versioned keg are absent; the usual Homebrew entry point and portable Ruby
retain their original hashes. The formula's install and test blocks remain
unqualified. A supported unprivileged isolated environment is the next check;
this result does not prove normal-prefix installation is broken.

## Publication inventory

The repository was still private when inspected. Read-only enumeration covered
26 remote branch heads, tag `v0.0.1`, and 109 retained PR heads. All enumerated PR
heads were reachable from the scanned branch/tag union: 338 commits and 2,650
unique blobs, with no size skips. Main alone had 337 commits. Gitleaks' patch
counts, 195 on main and 196 on the union, are a different measure.

Configured Gitleaks scans of tracked HEAD and those histories returned zero
findings. Default-only scans with inline ignores disabled returned exit 1 and
five findings: four exact synthetic self-test token occurrences and one
private-key-header marker in the credential scanner, without key material.
Those failures and classifications were retained. No ignore rule was expanded.
The core archive contained neither the status document nor an owner/home path
match across its 810 regular member bodies.

The initial GitHub text scan covered 134 issue/PR bodies, five conversation
comments and zero inline review comments: 139 records, 390,098 bytes. Pagination
was exhausted. Gitleaks returned zero findings; the sole path-pattern match was
a synthetic traversal example in PR #46. A subsequent read-only inventory queried formal reviews for all 109 PRs and
returned zero reviews. It also found zero releases, zero release assets, zero
Actions artifacts and 1,003 Actions runs. Discussions are disabled; the wiki is
enabled. Actions logs, wiki content and attachment/media payloads were not
inspected. These remaining surfaces need review before changing repository
visibility; absence of release assets does not clear historical job logs.

The lockfile contains 145 external dependency records. Strict packaging checks
bind license texts and inventory entries. These checks do not decide dependency
license compatibility or third-party-data ownership. Two supplemental license
records matched their locally pinned hashes; their upstream contents were not
refetched during this audit. Secret detection is heuristic, not proof that all
credentials or private data are absent.

## Agent Router used during this work

Three actual task descriptions were submitted to the CLI: publication audit,
installation qualification and documentation review. With the bundled synthetic
host and legacy catalog, all three produced plans. That catalog contains older
5.6 model priors; no skill selection was requested in those inputs.

The same tasks were then routed against live `inspect --host codex` evidence.
The local model cache listed models but left tools, subagents, vision, network
and workspace-write capability support `UNKNOWN`. All three requests returned
exit 3, `no_route`, including `host.capability.unknown_high_risk` for the audit.
Repeating each request with the same snapshot produced identical bytes.

Agent Router supplied advice and abstained on insufficient live evidence. It
did not launch these agents, switch their models, or establish savings. The
operator explicitly launched Codex subagents with the available platform roles:
Astra/high for test engineering and independent code/security review, Sol 6.1/high
for inventory and documentation, with the applicable quality, contract,
permissions and documentation skills. The security specialist reviewed supplied
evidence; the inventory worker performed the actual filesystem/GitHub scans.
No paid provider experiment was launched by the router; Codex subagent cost and
productivity gains were not measured.

## Remaining acceptance

- Choose publication terms: retain FSL and describe the project as source
  available, or approve a compatible open-source license change. No choice is
  inferred from this report. See [FSL](https://fsl.software/) and the
  [Open Source Definition](https://opensource.org/osd).
- Resolve the historical identifying-path publication choice and remaining
  GitHub surface inventory, then recheck the exact visibility-changing scope.
- Decide whether experimental delivery is source/manual archive only. If
  Homebrew is advertised as supported, pass install, a useful command and
  uninstall in a supported isolated environment first.
- Accept the corrected revision, regenerate the release bindings, and establish
  publisher key trust before claiming a signed public release.
- Keep daily Claude/Codex automation and comparative gains tracked separately
  in [#131](https://github.com/FlorianBruniaux/agent-router/issues/131) and
  [#5](https://github.com/FlorianBruniaux/agent-router/issues/5). This packaging
  qualification does not complete those experiments.
