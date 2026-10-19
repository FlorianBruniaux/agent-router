# Local verification without GitHub Actions

Run the development checks on the current checkout and retain the command logs,
exit codes and source identity outside the repository. This does not require an
Actions budget or a model-provider account. Results cover the selected checks on
the machine that ran them; macOS does not establish Linux sandbox behavior.

## Run on this Mac

Use a dedicated worktree and Python 3.11 or newer. On Florian's Mac the installed
Homebrew interpreter is `/opt/homebrew/bin/python3`; the system Python is too old
for the TOML checks. Rust uses the version pinned in `rust-toolchain.toml`.

From the repository root:

```sh
/opt/homebrew/bin/python3 -B scripts/check-local.py --list
CARGO_TARGET_DIR=/private/tmp/agent-router-local-target \
  /opt/homebrew/bin/python3 -B scripts/check-local.py \
  --group quality --group native --group audit --group packaging
```

The runner creates a new temporary evidence directory and prints the path to
`summary.json`. Use `--output-dir /absolute/new/directory` to choose a location;
an existing directory or a location inside the checkout is refused. Reuse
`CARGO_TARGET_DIR` to avoid rebuilding the same dependencies on every run.
Do not edit the checkout during verification.

Tests run sequentially to reduce contention between native process tests.
The wrapper uses `rtk proxy` when available, retaining command output and exit
status in each log. Socket and Seatbelt tests need the host's normal local
permissions; run outside an agent sandbox that denies these operations. The
runner does not grant permissions or change global configuration.

For a development change that does not need the native stress suites, select
`--group quality`. Before a delivery, use the four-group command above and the
separate fuzz recipe. In the [6 October observation](bench/verification-local-ci-2026-10-06.json),
the workspace tests took 477 seconds and the ignored/native suites took
883 seconds. These are observations on this Mac, not a duration guarantee.

## Coverage and prerequisites

| Group | Checks | Prerequisites and limits |
| --- | --- | --- |
| `quality` | Format, all-feature and core Clippy, workspace tests, core-only local API test, harness parity, host syntax, Python tooling/bench fixtures, secret scans of files and history | Rust with Clippy/rustfmt, Python 3.11+, Gitleaks 8.30.1, Git; release tests require exact OpenSSL 3.5.9 at `/opt/homebrew/bin/openssl` |
| `native` | Build CLI/helper, then all tests explicitly ignored in the standard workspace suite | Seatbelt on macOS or Bubblewrap on Linux, Git, `nc`, compiler, local sockets; includes two bounded CPU/disk stress tests and synthetic host programs, no real inference |
| `audit` | Audit `Cargo.lock` against a newly fetched advisory database; record its Git commit and preserve lockfile hashes | Existing cargo-audit 0.22.2 and network access to RustSec; an unavailable database does not count as a clean audit |
| `packaging` | Fetch locked dependency archives, export the core with complete license evidence, verify the source archive | Cargo registry access or existing cache, Python and Gitleaks; the archive is written in the evidence directory |
| `fuzz` | Explicit coverage gap in this runner | Instrumented fuzzing requires the separate isolated recipe in [fuzzing](fuzzing.md); the runner does not install tools or mutate checked-in corpora |

Without `--group`, every group is selected, including the unsupported `fuzz`
group. That selection exits nonzero and records the gap. The four-group command
above is the supported local gate; a separate fuzz result must be retained if
parser fuzzing is required for the delivery.

Three Linux behaviors need their own Linux evidence: Bubblewrap file/network
isolation, killing an escaped descendant after its Bubblewrap parent dies, and
refusal of an allocation exceeding `RLIMIT_AS`. Running their names on macOS
can produce zero tests; that is not validation. A Docker CLI without a running
Linux engine cannot supply this evidence.

The bench tests use synthetic fixtures and loopback providers. Do not replace
them with `run_pilot.py calibrate/run` or a direct host probe: those have separate
approval and budget requirements. Source archive verification also does not
establish a signed release, installation, automatic routing or measured savings.

M3 harness fixtures use `scripts/check-m3-baseline.py`, which exports only the
required offline inputs from the Git base declared by the frozen candidate and
checks their pinned hashes. It runs the current M3 tests using their existing
`M3_REPO` override. Missing Git history or mismatched input fails the check.
This preserves historical failure criteria after #164 solved T1; the campaign
suite separately checks the corrected current report. Passing these historical
fixtures does not make the old tasks eligible for a new trial. No sealed M3
files or product permissions change.

The quality group discovers every `test_*.py` native-probe suite, including
versioned probes and the separate-session decision fixture under
`native-integration-u06`, and the offline manifest tests under `calibration-v2/tests`.
It also includes the campaign catalog, trace accounting and offline rehearsal
suites under [measurement-m123](bench/measurement-m123/README.md), with failures
propagated through the same runner.

It also includes the physical-send broker, startup-outage cancellation and
[dedicated-client qualification](bench/owned-entry-qualification-2026-10-08/README.md)
unit suites. Discovery runs their scripted/loopback fixtures only; native probes
remain explicit. A runner regression checks execution and nonzero failure
propagation for all three registrations. These checks do not turn a structurally
valid manifest into an execution approval.

## Read and retain results

`summary.json` records the selected commands, statuses, exit codes, log digests,
source fingerprints, tool evidence and coverage gaps. A failure, missing tool,
timeout, interruption or zero executed tests blocks a passing selected-checks
verdict. Tests marked not applicable remain explicit. `full_ci_equivalent` stays
false: one operating system cannot cover the two-system GitHub matrix.

The [first local verification record](bench/verification-local-ci-2026-10-06.json)
retains a failed discovery command and its corrected rerun: Python discovery
does not import the hyphenated `scripts/test-*.py` names. The runner now invokes
each of those files directly, retaining a separate verdict for each suite.
That record combines unchanged Rust/bench checks with the corrected tooling
run; it does not relabel the first full run as green.

Logs remain local. Publish only a reviewed summary with the tested commit,
source fingerprint, counts, failures and coverage limits in
`docs/bench/`, linked from the [playbook](bench/playbook.md) and the relevant
issue. Do not publish credentials, raw model conversations or private artifacts.
Retain the first failing run when a correction is made; a later pass does not
erase it. A new GitHub run is a separate observation when Actions becomes
available again.

The output directory is disposable after its needed evidence is retained.
The runner never deletes it automatically or installs a global hook, daemon or
self-hosted Actions runner.
