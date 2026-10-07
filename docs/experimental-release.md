# Experimental onboarding

Status: experimental source candidate, 2026-10-07. Publication, a signed release
and a package-manager installation are separate delivery steps.

Agent Router computes a local routing decision before coding work starts. Try
the offline CLI first. Supervised execution is a separate, approved workflow;
automatic daily Claude/Codex integration and general productivity or cost gains
are not established.

The source is available under [FSL-1.1-ALv2](../LICENSE.md), which restricts
competing commercial use. It is not an OSI open source license. Each version
converts to Apache 2.0 two years after it is made available, under the license's
terms. This guide does not change the license or announce repository publication.

## Choose an installation channel

[Pinned Cargo installation](install.md#cargo-from-a-pinned-git-revision) builds
from a complete Git workspace. The [local Homebrew formula](install.md#homebrew-local-formula)
installs a verified native archive. Both provide `agent-router` with exactly
`claude` and `codex` features and `router-sandbox-helper`; optional features need
a separate source build. The helper's absolute configuration path and approval
hash remain explicit, and no hook, credential or permission is installed.

The repository remains private. Local Cargo installation and removal passed on
macOS ARM64 using a `file://` Git source and offline dependencies; remote HTTPS
acquisition was not exercised. The complete v2 archive passed Homebrew install,
test, same-version reinstall, retest and uninstall with both hashes checked,
in an isolated macOS ARM64 prefix classified Tier 3, with Homebrew's sandbox
enabled. This does not qualify a default prefix, public tap or version upgrade.
See the [current distribution qualification](bench/distribution-qualification-2026-10-07.md)
for exact artifact and lifecycle observations. No public tap, crates.io package
or release asset is announced.

The first two-build pair mismatched and was refused; the second pair matched.
The initial cause remains `UNKNOWN`. Successful local installation is not a
general reproducibility, security, publisher-identity or host-execution verdict.

## Try one offline decision

Build from an authorized checkout with the [installation guide](install.md).
Rust 1.85 or newer is required. Cargo may need network access to fetch build
dependencies; routing itself needs no provider, model download or credentials.
Run from the repository root after the build:

```sh
example_dir=$(mktemp -d)
./target/release/agent-router route \
  --request tests/fixtures/requests/codex-implementation.json \
  --catalog assets/catalogs/codex-core-v1.json \
  --json > "$example_dir/decision.json"

./target/release/agent-router explain \
  --decision "$example_dir/decision.json"

rm "$example_dir/decision.json"
rmdir "$example_dir"
```

Both product commands should exit `0`. The explanation contains a decision hash
followed by these lines:

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

This demonstrates constraint filtering and phase selection on bundled synthetic
inputs. The model names, capabilities and scores are fixture data; they do not
prove model availability, current prices, authentication or native execution.
`Missing evidence: 0` applies to this supplied request. No host is launched,
no task is implemented and no global hook or configuration is installed.

For a different result, check that the command uses these exact fixture paths
and the binary built from the same checkout. Consult the [exit-code reference](cli-output.md#exit-codes-and-error-codes)
before interpreting a nonzero status as a successful selection.

## Read support at the demonstrated surface

| Surface | Evidence and limit |
| --- | --- |
| Offline routing and explanation | The example above returns the documented plan. The source CLI also provides versioned JSON contracts. This does not establish live model support. |
| Codex / Claude Code adapters | Included in the default build. Adapters prepare separate host processes; the current product rejects changing a parent conversation's model. [Host support](../README.md#host-support), [execution contract](../crates/router-adapter-api/src/execution.rs). |
| Native Codex app-server diagnostic | Recorded on macOS in October 2026: an explicit client turn transmitted model/effort parameters and received scripted provider output. Automatic prompt interception and an installed Agent Router integration remain unvalidated; Codex App and Claude surfaces were not tested by that diagnostic. [U02 evidence](bench/native-integration-u02/README.md). |
| Supervised runtime on macOS / Linux | Seatbelt / Bubblewrap implementations and fake-host native canary evidence are documented separately. A host process exit is transport evidence; task verification is another step. Windows native execution is not implemented. [Runtime evidence and limits](runtime.md#reproducible-checks). |
| OpenCode / Copilot | Optional adapter features with protocol fixtures and fake-host canaries. Installed native hosts and providers remain unverified. [Other-host setup and limits](other-hosts.md). |
| Distribution | Cargo and v2 archives carry the same two default product components. Local lifecycle qualification is separate from HTTPS acquisition, version upgrades, host execution and publication. See the [current qualification](bench/distribution-qualification-2026-10-07.md), [installation](install.md) and [local release](release-local.md). The [old core-only report](bench/release-qualification-2026-10-07.md) remains historical. |

The table summarizes checked source contracts and recorded evidence. It does
not claim those native canaries were rerun for a future release artifact.

## Move from a decision to approved execution

Follow [preparation](preparation.md) and [CLI execution](cli-execution.md) only
after supplying host binaries, credentials, fresh capability evidence, current
asset authority, explicit storage and a working native sandbox. Review the
exact execution digest before launching. Review the verified patch separately
before applying it. Routing or installation grants none of these permissions.

The [development harness](harness.md) configures contributor tooling. Its hooks
are separate from product routing and supply no runtime authority or evidence
of automatic host integration.

## Interpret measurements within their scope

The [host-direct pilot](bench/playbook.md#conclusions-of-the-full-run) records 100
runs: 10 small Python tasks, five arms, two providers and one repetition, out of
300 runs in the sealed order. Arm A uses the same per-phase recipe on every task;
the pilot does not test adaptive selection per task. Prices are catalog list
prices, not invoices. A correction decided after observation changes eight
Codex acceptance verdicts affected by `__pycache__` scope checks; both raw and
corrected results are retained. Corrected acceptance does not separate the arms
on quality, and cost comparisons vary by provider and comparison arm.

This host-direct campaign does not exercise Agent Router's
approval, sandbox, verification or patch-application lifecycle. It establishes
no general cost, quality or daily productivity gain.

Use the [completion ledger](completion.md) for implementation acceptance and
the [documentation index](README.md) to select the next guide.
