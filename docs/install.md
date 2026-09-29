# Installation

Agent Router is built from source. There is no published package-manager
install and no automatic global configuration step. Installing it never enables
a host integration, a hook or a permission (see [What installation does not
do](#what-installation-does-not-do)).

The repository is private, so cloning requires access to
`FlorianBruniaux/agent-router`.

## Prerequisites

| Requirement | Detail |
| --- | --- |
| Operating system | macOS or Linux for supervised execution. Windows native execution is not implemented ([host support](../README.md#host-support)). Routing, `explain` and the other read-only commands do not launch anything. |
| Rust | 1.85 or newer. `rust-toolchain.toml` pins channel `1.85` with `rustfmt` and `clippy`, so `rustup` selects it inside the checkout. |
| Git | To clone the repository. |
| Linux sandbox | Supervised execution needs a bubblewrap binary and working unprivileged namespaces ([runtime](runtime.md#native-sandbox-evidence)). |
| macOS sandbox | Supervised execution uses `/usr/bin/sandbox-exec` (Seatbelt). |
| Python 3.11 or newer | Only for the release scripts in `scripts/`. The CLI never invokes Python. |
| ffmpeg | Only to regenerate the README demo media ([media guide](media/README.md)). Not needed to build or run the CLI. |

Routing works offline once Cargo dependencies are available. No semantic
service or model download is required at runtime.

## Build from source

```sh
git clone git@github.com:FlorianBruniaux/agent-router.git
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

Run these from the repository root. They use bundled synthetic fixtures, make no
model call and launch no host.

```sh
./target/release/agent-router --version
./target/release/agent-router --help
```

Then route a fixture and read the explanation:

```sh
./target/release/agent-router route \
  --request tests/fixtures/requests/codex-implementation.json \
  --catalog assets/catalogs/codex-core-v1.json \
  --json > "${TMPDIR:-/tmp}/agent-router-decision.json"

./target/release/agent-router explain \
  --decision "${TMPDIR:-/tmp}/agent-router-decision.json"
```

Expected output, apart from the decision hash line:

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

`route` exits `0` for a selection, `3` for a valid no-route result, `2` for
invalid input and `4` for an internal error.

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

The binary archive is built with `--no-default-features` and contains
`agent-router` only. It does not contain `router-sandbox-helper`, so supervised
execution from that archive needs the helper built separately with the build
command above.

### Homebrew, private prefix

`scripts/release-local.py verify` can render a Homebrew formula from
`distribution/homebrew/agent-router.rb.in`, pointing at the local archive by
`file://` URL. Installing it is UNKNOWN: it has not been exercised. Only
`ruby -c` syntax was checked on a rendered formula. The `install` block, the
`file://` handling and the formula's `test` block are untested
([limits](release-local.md#limits)).

If you try it, use a private Homebrew prefix. Installing with the system `brew`
has global effects. The formula installs `bin/agent-router` and the license
documents only.

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

The build writes to the checkout (`target/`) and to Cargo's own cache
(`~/.cargo`, shared with your other Rust projects; leave it unless you want to
reclaim space). Removal is deleting what you created.

1. Delete the checkout, which contains `target/` and both binaries. If you copied
   `agent-router` or `router-sandbox-helper` elsewhere, delete those copies.
2. Delete any private state directories you chose. Agent Router uses no default
   location: each is a path you passed explicitly.
   - Runtime and workflow stores: the `--store` and `--runtime-root` paths. These
     hold the SQLite database (WAL files included), lock file and retained
     artifacts ([runtime](runtime.md#durable-state-and-control)).
   - Local API state: the `--state-dir` path given to `serve` (`router.sock`,
     `token`, `lock`).
   - Feedback and catalog stores, and sandbox or approval files you wrote, at
     the paths you chose.
3. If you rendered a Homebrew formula and installed it, remove it with Homebrew
   in the same prefix you used. This path is UNKNOWN as described above.

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
- It does not publish, sign or install a release archive.

Next: [preparation](preparation.md), then the
[single-run guide](cli-execution.md) or [workflow guide](coordinator.md).
