# Installation

English | [Français](install.fr.md)

Agent Router has two experimental installation channels: Cargo from a pinned
Git revision, and a generated Homebrew formula for a verified local archive.
Both install `agent-router` with its default Claude/Codex adapters and
`router-sandbox-helper`. Installation enables no host integration, hook or
permission (see [What installation does not do](#what-installation-does-not-do)).

The repository is private, so cloning requires access to
`FlorianBruniaux/agent-router`. No crates.io package, public Homebrew tap or
public release asset is announced here. Current qualification is recorded in
the [distribution qualification](bench/distribution-qualification-2026-10-07.md);
the earlier [core-only trial](bench/release-qualification-2026-10-07.md) remains historical.

## Prerequisites

| Requirement | Detail |
| --- | --- |
| Operating system | macOS or Linux for supervised execution. Windows native execution is not implemented ([host support](../README.md#host-support)). Routing, `explain` and the other read-only commands do not launch anything. |
| Rust | 1.85 or newer. `rust-toolchain.toml` pins channel `1.85` with `rustfmt` and `clippy`, so `rustup` selects it inside the checkout. |
| C toolchain | A native C compiler/linker and platform SDK for source builds, including Cargo installation, because SQLite is built from bundled source. |
| Git | To clone the repository. |
| Linux sandbox | Supervised execution needs a bubblewrap binary and working unprivileged namespaces ([runtime](runtime.md#native-sandbox-evidence)). |
| macOS sandbox | Supervised execution uses `/usr/bin/sandbox-exec` (Seatbelt). |
| Python 3.11 or newer | Only for the release scripts in `scripts/`. The CLI never invokes Python. |
| ffmpeg | Only to regenerate the README demo media ([media guide](media/README.md)). Not needed to build or run the CLI. |

Routing works offline once Cargo dependencies are available. No semantic
service or model download is required at runtime.

## Cargo from a pinned Git revision

Choose an absolute installation prefix owned by your user. Replace the example
path before running the command; Cargo writes binaries and installation
metadata there, and downloads source/dependencies into its cache. Use your
existing authenticated Git access while the repository is private.

```sh
install_prefix="/absolute/user-prefix"
cargo install --git https://github.com/FlorianBruniaux/agent-router \
  --rev f378afad4f584bac0efabbbb9dab37c48db4298f --locked \
  --root "$install_prefix" agent-router-cli router-sandbox-helper

"$install_prefix/bin/agent-router" version --json
```

The pinned revision identifies the product source used for this qualification;
it does not point to unpublished distribution-tooling edits. Expected CLI
features are `claude` and `codex`. The installed helper remains an internal
program: configure its absolute path and review its hash in the execution
approval, as described below. Adding binaries to your shell's `PATH` is a
separate user choice.

Workspace dependencies use local paths, so installing `agent-router-cli` by
name from crates.io is not a supported channel. The pinned Git command provides
the complete workspace. On macOS ARM64, separate installs of both packages from
a local `file://` Git clone at this exact revision passed, with offline vendored
dependencies. Route/explain passed outside the checkout with source and cached
checkouts hidden; repeat installation, same-pin `--force` replacement and
removal preserved the pre-existing marker. HTTPS Git acquisition was not
exercised. Replacement at one pin does not establish an upgrade between product
versions; that result remains `UNKNOWN`. See the linked qualification report.

## Build from a checkout

```sh
git clone https://github.com/FlorianBruniaux/agent-router.git
cd agent-router
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper
```

This produces two binaries in `target/release/`:

- `agent-router`: the CLI.
- `router-sandbox-helper`: the resource-limit helper that supervised execution
  needs. It is an internal binary and takes no user-facing arguments, so do not
  run it directly. You reference its absolute path in the sandbox file
  (`helper` field, see the [execution guide](cli-execution.md#sandbox-and-privacy-configuration)).
  Its SHA-256 is part of the execution approval and is rechecked before launch
  ([runtime](runtime.md#native-sandbox-evidence)); a missing or changed helper
  fails before it is executed.

If you only route, plan and explain, `agent-router` alone is enough.

### Without host adapters

The default build includes the Codex and Claude adapters. To build the routing
CLI without them:

```sh
cargo build --release --locked --no-default-features -p agent-router-cli
```

### With optional features

Features are compiled in explicitly. See [Optional features](#optional-features).

```sh
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper \
  --features agent-router-cli/local-api
```

## Verify the installation

The fixtures are synthetic, make no model call and launch no host. For a source
build, run the following version check from the repository root:

```sh
./target/release/agent-router --version
./target/release/agent-router --help
```

For Cargo or Homebrew, use the installed executable's absolute path instead.
Obtain an authorized checkout of the same revision for the two fixture files.
Replace both paths below. The commands copy only those inputs and then route
from a temporary directory outside the checkout:

```sh
source_checkout="/absolute/agent-router-checkout"
agent_router_bin="/absolute/user-prefix/bin/agent-router"
example_dir=$(mktemp -d)
cp "$source_checkout/tests/fixtures/requests/codex-implementation.json" "$example_dir/request.json"
cp "$source_checkout/assets/catalogs/codex-core-v1.json" "$example_dir/catalog.json"
(
  cd "$example_dir" || exit
  "$agent_router_bin" route --request request.json --catalog catalog.json \
    --json > decision.json
  "$agent_router_bin" explain --decision decision.json
)
rm "$example_dir/request.json" "$example_dir/catalog.json" "$example_dir/decision.json"
rmdir "$example_dir"
```

Expected output, apart from the decision hash line:

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

`Missing evidence: 0` describes the supplied fixture, not your installed host
or its credentials. For a source build, `agent_router_bin` can instead point
to the checkout's `target/release/agent-router` by absolute path.

`route` exits `0` for a selection, `3` for a valid no-route result, `2` for
invalid input, `4` for an internal error and `7` for an integrity error; see
[exit codes](cli-output.md#exit-codes-and-error-codes).

### Diagnostics

`doctor` is the read-only diagnostic command ([diagnostics](doctor.md)). It does
not discover home directories, launch a host, repair files or run migrations.
Two forms exist:

```sh
agent-router doctor --request manifest.json --json
agent-router doctor --runtime-root /absolute/private/runtime --json
```

The second form inspects an existing runtime directory and refuses a missing
one; it creates nothing and recovers nothing. It is not a check of a fresh
install, because a fresh install has no runtime directory. SQLite may still
update its WAL-index while reading, so it is not a zero-write guarantee
([doctor](doctor.md#passive-runtime-storage-diagnostics)).

A passing `doctor` run does not establish that a host works, that a sandbox
enforces anything or that a credential is valid.

## Local release build

For a reproducible local binary archive with a signed manifest, follow
[release-local.md](release-local.md). It builds on
[binary-distribution.md](binary-distribution.md) (the binary archive) and
[distribution.md](distribution.md) (the separate source archives). Nothing
there signs, publishes or installs for you.

The v2 binary archive contains both `agent-router` (exact features `claude` and
`codex`) and `router-sandbox-helper`. These are the same default product
components installed by the Cargo command. Optional features still require a
separate source build. Supplying the helper does not configure a sandbox or
authorize execution: its absolute `helper` path and hash remain explicit inputs.

### Homebrew, local formula

`scripts/release-local.py verify` can render a Homebrew formula from
`distribution/homebrew/agent-router.rb.in`, pointing at the local archive by
`file://` URL by default. The formula installs both binaries and the license
documents, and refuses a mismatched OS or CPU. It has a self-contained
route/explain test and a helper argument-refusal test; these do not execute a
host or prove native sandbox enforcement.

On macOS ARM64, the complete v2 archive passed local install, formula test,
same-version reinstall, a second test and uninstall. Both binary hashes matched
the archive, the helper refused missing arguments with exit `125`, and removal
preserved the unrelated marker. Homebrew's own sandbox stayed enabled, without
an outer sandbox or safety override. The custom prefix outside `/tmp` was
classified Tier 3 by Homebrew; this does not qualify a default prefix, public
tap, HTTPS download, different-version upgrade or another platform. See the
[current qualification](bench/distribution-qualification-2026-10-07.md).

Use a separately provisioned isolated prefix outside `/tmp`, with Homebrew's
ordinary core catalog available locally and its safety checks intact. The
trial copied a complete shallow pinned core catalog after two stopped automatic
deep-clone attempts. The exact source pins and environment controls are in the
qualification report. This guide does not provision a prefix or modify system
Homebrew; the older nested-sandbox failure remains historical.

A local formula requires a local tap. After placing the verified formula at
`Library/Taps/local/homebrew-agent-router/Formula/agent-router.rb` beneath that
Homebrew's repository, select its absolute executable path:

```sh
brew_bin="/absolute/isolated-homebrew/bin/brew"
"$brew_bin" install local/agent-router/agent-router
"$brew_bin" test local/agent-router/agent-router
```

The tap and archive must already exist. These commands refer to your local tap,
not a published upstream tap. An optional canonical HTTPS archive URL can be
rendered for future publication; it neither uploads an asset nor checks that
one exists ([release attestation](release-local.md#archive-url-and-publication)).

## Optional features

Read from `crates/agent-router-cli/Cargo.toml` and the linked guides. Pass them
as `--features agent-router-cli/<name>` when building with `-p` for several
packages.

| Feature | Default | What it enables |
| --- | --- | --- |
| `codex` | yes | Codex host adapter (`adapter-codex`). |
| `claude` | yes | Claude Code host adapter (`adapter-claude`). |
| `opencode` | no | OpenCode adapter and the optional `opencode` sandbox-manifest field ([other hosts](other-hosts.md)). |
| `copilot` | no | Copilot adapter and the optional `copilot` sandbox-manifest field ([other hosts](other-hosts.md)). |
| `local-api` | no | The `serve` subcommand: a read-only route API on a Unix socket with a bearer token ([local API](local-api.md)). Without the feature there is no `serve`. |
| `local-sidecar` | no | The outbound standard-sidecar client transport. The default build has none ([standard sidecar](standard-sidecar.md)). |
| `semantic-retrieval` | no | Validation of an externally supplied `--semantic-ranking` file together with `--context-query` (enables the `semantic-ranking` feature of `router-retrieval`). It bundles no embedding model, provider or network request ([retrieval](retrieval.md#optional-externally-supplied-semantic-ranking)). |

`semantic-ranking` is a feature of the `router-retrieval` library, not of the
CLI. The CLI exposes it as `semantic-retrieval`.

Confirmed on a build from this checkout: `agent-router serve --help` exists with
`--features local-api` (options `--state-dir`, `--max-connections`,
`--request-deadline-ms`). The other feature builds were not run for this page;
their effects come from the Cargo manifest and the guides above.

## Uninstall

Remove the packages through the channel that installed them. Keep unrelated
packages, files and Cargo caches. For the same prefix used above:

```sh
cargo uninstall --root "$install_prefix" agent-router-cli router-sandbox-helper
```

For a local Homebrew installation, use the same isolated Homebrew executable:

```sh
"$brew_bin" uninstall local/agent-router/agent-router
```

The current qualification reports which channel removals were observed. These
commands do not remove runtime state or a source checkout.

1. If you built from a checkout, remove only the build outputs or copied
   binaries you created. Inspect any unrelated checkout edits before deleting
   it. Cargo's cache is shared with other Rust projects.
2. Delete any private state directories you chose. Agent Router uses no default
   location: each is a path you passed explicitly.
   - Runtime and workflow stores: the `--store` and `--runtime-root` paths. These
     hold the SQLite database (WAL files included), lock file and retained
     artifacts ([runtime](runtime.md#durable-state-and-control)).
   - Local API state: the `--state-dir` path given to `serve` (`router.sock`,
     `token`, `lock`).
   - Feedback and catalog stores, and sandbox or approval files you wrote, at
     the paths you chose.

Before deleting a runtime store, check for interrupted runs or retained output
you still need. Deletion removes directory entries and makes no claim of secure
media erasure ([runtime](runtime.md#durable-state-and-control)).

## What installation does not do

- It does not install or enable hooks, plugins, skills or host integrations.
- It does not write global configuration, environment files or credentials.
- It does not grant any permission, approval or network access. Execution needs
  an explicit approval digest, fresh capability evidence and a working native
  sandbox.
- It does not launch a host or call a model.
- It does not create a runtime directory, database or token. Those appear only
  when you run a command that names a path for them.
- It does not download embeddings, a semantic backend or dependencies at
  runtime.
- It does not publish or sign a release. Cargo builds from source; Homebrew
  installs the explicitly selected archive.

Next: [preparation](preparation.md), then the
[single-run guide](cli-execution.md) or [workflow guide](coordinator.md).
