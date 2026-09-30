---
adr: "0021"
title: Host capability evidence from supervised canaries
status: proposed
created: 2026-09-30
last_modified: 2026-09-30
scope: "Batches 1 to 3 are implemented (Claude model identity, typed `prepare` errors, `assets seal`). Batch 4 ran on 2026-09-30: Codex cannot reach its provider under `network: false`, so this ADR stops at batch 4 for Codex until a provider-egress ADR; Claude Code is inconclusive at startup. Batches 5 to 9 are design only; every real host capability stays UNKNOWN."
amended_by: []
supersedes: []
superseded_by: []
related: ["0002", "0003", "0014", "0016"]
---

# ADR 0021: Host capability evidence from supervised canaries

## Context

A phase that writes to the workspace cannot be routed on a real Claude Code or
Codex host. Facts read on `main` at `765a2f6`:

- The Claude probe marks every capability `unknown`
  (`crates/adapter-claude/src/probe.rs:112-123`). The Codex probe does the same
  (`crates/adapter-codex/src/lib.rs:138-149`).
- The policy rejects a phase whose required capability is not `supported`
  with `capability.unknown_or_unsupported`
  (`crates/router-policy/src/phases.rs:638-642`). The runtime opens the network
  in the sandbox only when `Network` is required
  (`crates/router-runtime/src/lib.rs:1521`), so a phase needing the network
  meets the same rejection. With `network: true` the Seatbelt profile adds
  `(allow network*)` without any destination limit
  (`crates/router-runtime/src/sandbox.rs:447-448`).
- A snapshot may claim `supported` only when a source of kind `host_runtime`
  backs it (`crates/router-adapter-api/src/lib.rs:251-262`). No production code
  emits that kind: it appears only in tests, examples and a documentation
  fixture (`git grep -n HostRuntime -- 'crates/*/src/*'` returns the enum and
  the check).
- The kind is provenance, not authentication
  (`crates/router-adapter-api/src/lib.rs:32-33`). `run` reads the snapshot from
  a caller file (`crates/agent-router-cli/src/execution.rs:33`, `:340-343`,
  `crates/agent-router-cli/src/phases.rs:64`,
  `crates/agent-router-cli/src/inspect.rs:123`), and the snapshot hash is an
  unkeyed digest (`crates/router-adapter-api/src/lib.rs:302-306`). A
  hand-written snapshot that labels a source `host_runtime` passes validation
  today. An independent security review on 2026-09-30 confirmed this path. It is
  a weakness of evidence authenticity; it does not bypass the approval digest.
- Fresh execution accepts a snapshot and each of its sources for 300 seconds;
  historical reconstruction accepts 86 400 seconds, measured from a historical
  clock (`crates/agent-router-cli/src/execution.rs:298-309`, `:349`,
  `crates/router-adapter-api/src/lib.rs:310-320`). Neither path reads a store.
  Replay validates its snapshot through the same
  `ValidatedHostObservation::from_observation`
  (`crates/router-eval/src/replay.rs:248`).
- The approval digest binds the SHA-256 of every configured program, the
  sandbox helper and `/usr/bin/sandbox-exec`
  (`crates/agent-router-cli/src/execution.rs:706-718`). The supervisor hashes
  the program again when it builds the command
  (`crates/router-runtime/src/sandbox.rs:292`), then `sandbox-exec` executes it
  by path (`crates/router-runtime/src/sandbox.rs:371-372`). The file can change
  between that hash and the exec. The Seatbelt supervisor runs its own
  deny/allow canaries before any run
  (`crates/router-runtime/src/sandbox.rs:174-240`), including a loopback
  listener that `nc` must fail to reach under `network: false`
  (`crates/router-runtime/src/sandbox.rs:236-265`).
- Only the Copilot and OpenCode adapters check a fresh `--version` against the
  approved pin (`crates/adapter-copilot/src/lib.rs:130-133`,
  `crates/adapter-opencode/src/lib.rs:132-135`). Claude and Codex keep the
  default `false`.
- The per-store approval key is a 0600 file owned by the user who runs the
  router (`crates/agent-router-cli/src/approval_key.rs:12`).
- Before batch 1, the Claude probe required the `init` model to appear among the models of
  root assistant messages (`crates/adapter-claude/src/probe.rs:96-103`). The
  fixtures use the same synthetic id on both sides
  (`crates/adapter-claude/tests/fixtures/claude.jsonl:1-2`). A real 2.1.285
  stream announced `claude-haiku-4-5` in `init` and a dated id in the assistant
  message, and `inspect` failed with `probe.cache.invalid` (reported by the
  pilot agent on 2026-09-30, not reproduced here).
- Before batches 2 and 3, `prepare` mapped every `router_prepare` error to
  `prepare.invalid_or_denied` (`crates/agent-router-cli/src/preparation.rs:219`).
  The only `assets` subcommand was `review` (`crates/agent-router-cli/src/command.rs:553-566`);
  `seal_contracts` and `seal_availability` were library functions called from
  tests only (`docs/asset-contracts.md:29-30`), while new execution approval
  requires a v3 asset authority (`docs/cli-execution.md:74-75`).

Not known: whether a native Claude Code or Codex run reaches its provider when
the profile denies the network. Every canary below is a native run, so this
decides whether canaries can run at all.

## Decision

### Evidence source

Only a supervised canary produces `supported`. A canary is a native run of the
host, under the verified OS sandbox, on a throwaway directory. The router
chooses a random nonce, gives the host a fixed instruction, then checks the
observable effect itself.

The canary directory and the private binary copies (see "Executed binary")
live under a dedicated root, created owner-only. That root is not the store
directory, not an ancestor of `approval.key`, and not a directory whose
listing or readable siblings expose the key. The child profile never includes
the store in its read or write roots.

- `WorkspaceWrite`: the host must create one named file whose content equals
  a second nonce. The router checks the exact bytes and that no other path in
  the directory changed. A missing file, wrong bytes or an extra path produce
  no proof.
- `Network`: the router serves one nonce on a loopback port it opened for the
  canary, reusing the listener pattern of the sandbox probe. The host must
  fetch it and write it to the named file. The command refuses to start a
  `Network` canary under a `network: false` profile.
- `Tools`, `Subagents` and `Vision` get no canary in this ADR.

A failed canary records the attempt and leaves the capability `unknown`. It
never produces `unsupported`, because one failed model turn does not show that
the host lacks the capability.

### Meaning of `Network` when supported

The loopback canary proves one thing: under the `network: true` profile, a tool
of the host opened a TCP connection and read the reply. It proves nothing
about external egress, DNS resolution, proxies or TLS. The proof records
`network_scope: loopback_fetch`, and `docs/reference/json-contracts.md` will
define `Network: supported` with exactly that meaning. A phase that needs a
named external destination cannot rely on it; a narrower capability for that
case needs its own ADR.

### Provider egress spike

Before any canary command is built, a spike measures, for Claude Code and for
Codex separately, whether a native run under the `network: false` profile
reaches its provider. The spike uses the approved binary, a one-line prompt,
and records exit code, termination and whether the stream contains a root
assistant message. It runs only after the owner authorizes the spend. The
result decides the path per host:

- **Provider unreachable** (expected, since the profile denies sockets): the
  canaries and every native run need provider egress. This ADR then stops at
  batch 4 for that host. A separate ADR must separate provider egress (only
  the provider endpoint, for example through the loopback transport of ADR
  0014) from the `Network` capability of tools. Running canaries with
  `network: true` in the meantime is refused, because it would open the network
  on every write run without the phase asking for it.
- **Provider reachable**: some channel escapes the socket denial. Canaries may
  proceed, and the channel is recorded as a sandbox finding in its own issue,
  since `network: false` no longer means what the runtime documents.
- **Inconclusive** (crash, timeout, ambiguous stream): the host stays UNKNOWN
  and batches 7 and 8 stay blocked for it.

### Binding and freshness

A proof is one canonical record. Its MAC covers every field below, and the
shared validation compares each one with the `host_runtime` source that cites
the proof:

- record schema version, proof id and a per-store sequence number;
- store id, a random value created with the store;
- capability, tier (`proven` or `attested`) and, for `Network`, the scope;
- harness id and host snapshot id;
- host binary SHA-256 and the output of a supervised `--version`;
- adapter id and the digest of the argument template it prepares, which
  carries the host permission mode;
- sandbox helper and wrapper digests, profile shape (write roots relative to
  the workspace, network flag) and OS;
- the model id observed in the canary stream, the nonce digests, and the
  canary start and end times.

A source matches a proof only when its `id` equals the proof id, its
`content_sha256` equals the proof digest, its `observed_at_unix_seconds` equals
the proof end time and its `observed_version` equals the bound version. The
capability whose `source_ids` list it must equal the proof capability, and the
snapshot harness must equal the proof harness. A valid `Network` proof
therefore cannot back `WorkspaceWrite`, and a proof from another host, another
store or another tier backs nothing.

The MAC key is HKDF-SHA256 (RFC 5869) with the per-store secret as input
keying material, the store id as salt and `agent-router/capability-proof/v1`
as info. The approval binding keeps its own use of the secret.

A proof expires after `DEFAULT_MAX_AGE_SECONDS` (86 400) unless the operator
sets a shorter bound. Any change to a bound field voids it before expiry. A
proof whose start or end time lies after the verifier's clock is refused.

A proof is replayable by design during its lifetime: any snapshot built in
those 86 400 seconds with the same binding may cite it. The snapshot itself
keeps its own bound, 300 seconds for fresh execution and 86 400 seconds for
historical reconstruction. On the historical path, the proof is checked
against the historical clock of the run, not against the current time, and the
result feeds only `Runtime::reopen`, never fresh execution.
Claude and Codex adapters gain a native `--version` check so that the version
bound at canary time is compared again at dispatch.

### Revocation

For one capability and one binding digest, the row with the highest sequence
number decides. A failed canary that is newer than a valid proof makes the
capability `unknown` again, until a newer canary succeeds. The sequence number
is inside the MAC, so rows cannot be reordered without detection. Deleting
rows is not detected; see the threat model.

### Executed binary

Every native run, canaries included, executes a private copy. The runtime
copies the approved binary into a run-scoped, owner-only directory under the
dedicated root, hashes the copy after it is closed, compares that hash with the
approval, makes the file read-only and executes the copy. The child profile
grants no write access to that directory. This replaces the path-based exec of
the original file. Using an open file descriptor instead (`fexecve` or
`execveat`) is allowed on Linux if bubblewrap supports it; macOS has no such
call, so the copy is the only route there.

The copy binds the entry file only. If a host is a script or loads modules
from its install directory, the interpreter and those modules stay unbound,
and a host that locates resources relative to its own path may fail from a
copy. The spike records both for Claude Code and Codex.

### Threat model

The MAC stops a snapshot written by hand, a proof copied from another store and
a proof reused for another capability, host or tier. It does not stop a process
running as the same user: that process can read the 0600 key, forge a proof,
delete rows or replace the private copy. The sandbox keeps the host child
itself away from the key only as long as the store stays outside the child's
read roots. Protection against another process of the same user needs a
separate identity (another user, a keychain item with an access list, or
hardware), which this ADR does not provide.

### Storage and use

The record format and its validation live in `router-adapter-api`, with the
other host evidence. Records are append-only rows in the runtime private store.

`inspect` gains an explicit store argument. It folds the deciding row of each
capability, when it is a valid and unexpired `proven` proof, into the snapshot
as a `host_runtime` source. Snapshot validation checks those sources against
the proof lifetime instead of the 300 second snapshot bound.

Verification lives in the shared validation path, not in `run` alone.
`ValidatedHostObservation` construction, which the CLI loader, the historical
path and replay all use, takes a proof verifier. With a verifier, every
`host_runtime` source behind a non-`unknown` capability must match a proof as
described under binding. Without one, a non-`unknown` capability is refused
with `capability.evidence.unauthenticated`. `route-phase`, `run`, historical
reconstruction and `inspect` take the store explicitly. `run` also checks that
the proof binary digest and version equal those pinned by the approval.

Replay and evaluation read proofs from the store of the run when it is given.
A replay exported without that store cannot check the MAC; its capability
evidence is then reported as `unverified` in the replay result, which grants
nothing and is never execution authority.

A new receipt schema version lists, per required capability, the proof
digest, its tier, its observation time and its expiry. Earlier receipt
versions keep their meaning. The run event log records the same digests.

### Operator attestation

An operator may record an attestation with the same binding, an identity label
and a free-text reason, under tier `attested`. An attestation never enters
`host.capabilities` and never sets `support: supported`; the capability stays
`unknown` in the snapshot. Attestations travel in a separate, versioned
snapshot field. Keeping them out of the capability map matters because the
policy gate reads that map directly (`snapshot.capabilities()`,
`crates/router-policy/src/phases.rs:638`), and validation already ties any
non-`unknown` value to a `host_runtime` source
(`crates/router-adapter-api/src/lib.rs:258-262`). The policy ignores
attestations unless the phase policy names the capability in an explicit
acceptance list, and the receipt then shows the tier `attested`. The same rule
holds in `route-phase`, `run`, replay and evaluation. This tier is the last
batch and may be dropped without changing the others.

### Claude model identity

The model a stream proves is the `message.model` of root assistant messages.
The `init` model is a configured label. The probe stops requiring the `init`
label among observed models, and never adds the label to `supported_models`.
It still fails with `model not observed` when no root assistant message names a
model. Nothing strips a date suffix or guesses that two ids name one model.
An alias can map to a dated id only through an explicit table in the signed
catalog. Until that table exists, a candidate must name the dated id.

### Asset authority and `prepare`

Sealing asset documents and typing the `prepare` failure follow ADR 0003 and
need no new decision. They are separate batches, listed below because a real
write phase needs them. A seal command computes and prints the integrity hash
of a contracts or availability document. It establishes no authority: the
operator still pins the expected hash in the manifest.

## Implementation batches

Each batch lands with the test named as its acceptance criterion. No batch
runs a paid host call in CI.

1. Claude model identity. A stream with `init` model `claude-haiku-4-5` and an
   assistant message with a dated id yields a snapshot listing only the dated
   id. A stream without a root assistant model still fails.
2. Typed `prepare` errors. Each `router_prepare` gate returns its own fixed
   code; a test per gate asserts the code and the absence of input text.
3. `assets seal`. Sealing then validating with the printed hash succeeds; a
   one-byte change fails; the command never writes a manifest.
4. Provider egress spike, per host, after the owner authorizes the spend. The
   deliverable is a measured record per host (exit, termination, root
   assistant message present or not, script or single binary, behaviour when
   run from a copy) and the path chosen above. No code lands in this batch.
5. Private-copy execution. A test replaces the binary between the approval
   hash and the exec; the run either executes the verified bytes or refuses,
   and never executes the new bytes.
6. Proof record, store and shared verification, with a fake host binary. A
   proof folded by `inspect` lets a write phase pass the policy gate. The
   shared validation refuses, in `route-phase`, `run` and the historical path:
   a hand-written snapshot with a `host_runtime` source; a proof cited for
   another capability (a `Network` proof for `WorkspaceWrite`) or another
   host; a tier relabelled from `attested` to `proven`; a MAC computed with
   another store's key or with the approval label; a proof whose source
   timestamps or version differ; a proof with a future timestamp; an expired
   proof; a proof overridden by a newer failed canary. Changing one byte of the
   binary, the version output or the sandbox profile voids the proof. A
   historical reopen accepts a proof valid at the run's clock and expired now.
   A replay without the store reports `unverified`.
7. `WorkspaceWrite` canary command, with a digest-bound approval of its own,
   only for a host whose spike allows it. With a scripted fake host: correct
   write yields a proof; wrong bytes, an extra file or no file yield none. A
   test checks that the canary root is outside the store and that the child
   cannot read `approval.key`. One real canary per host runs only after the
   owner authorizes the spend.
8. `Network` loopback canary, same acceptance shape as batch 7, plus two
   tests: the proof carries `network_scope: loopback_fetch`, and the command
   refuses a `Network` canary under a `network: false` profile.
9. Operator attestation tier. Tests, in `route-phase`, `run`, replay and
   evaluation: an attestation never changes `host.capabilities` or `support`;
   a phase policy without an acceptance list rejects it; with the capability
   listed, the receipt shows `attested`.

## Batch 4 result

The spike ran three times on 2026-09-30, on macOS, for a total list cost of
1094 microUSD. Harness, results and redactions are in
`docs/bench/runs/2026-09-30-egress-spike/` (below, "the run folder"). The
harness replicates the profile, command and adapter argv of `f263e7d`, not the
resource helper, the approval digest or the router prompt.

**Codex: provider unreachable.** In run v3, Codex `0.159.2` with
`gpt-5.6-luna` exited 1 after 16 872 ms under `network: false`, with
`Connection failed: error sending request` and no root assistant message. Under
the same profile with `network: true` it completed with a root assistant
message, and so did a private copy of its executable
(`results/v3-20260930T184024Z/codex.json`, launches `C1`, `C2`, `C3`). For
Codex this ADR therefore stops at batch 4: canaries and every native run need
provider egress, and a separate ADR must separate provider egress from the
`Network` capability of tools, as decided under "Provider egress spike". The
v3 profile carried two deviations from the runtime's, listed below; the only
difference between `C1` and `C2` was the network line.

**Claude Code: inconclusive at startup.** In run v3, `claude auth status`
(`2.1.285`) exited 1 under a private `CLAUDE_CONFIG_DIR`, even with
`~/Library/Keychains` as a read root; its output reports `"loggedIn": false`
(`results/v3-20260930T184024Z/claude.json`, launch `S1`). No model call was
made, so Claude egress stays UNKNOWN. The binary names its keychain entry with
a suffix derived from `CLAUDE_CONFIG_DIR` when that variable is set (strings of
the `2.1.285` binary); that explains the result but was not observed in the
run.

**Product findings.** Each one blocks a native Codex run under the runtime as
it stands at `f263e7d`:

- The read-only profile grants no writable root for host state. Codex exited 1
  with `fs_denied` in `C1` and `C2` alike (run v1,
  `results/v1-20260930T170911Z/codex.json`). The runs gave each host a private
  0700 directory named by `CODEX_HOME` or `CLAUDE_CONFIG_DIR`, both names the
  runtime already accepts (`crates/router-runtime/src/sandbox.rs:466-476`).
- Codex needs `user-preference-read` on the domain `com.openai.codex`. Without
  it, `CFPreferencesAppSynchronize("com.openai.codex")` fails and Codex stops
  with `Failed to synchronize managed preferences`, identically with and
  without network (run v2, `results/v2-20260930T173101Z/codex.json`; call site
  `codex-rs/config/src/loader/macos.rs:136-148` at tag `rust-v0.159.2`). The
  profile allows every `mach-lookup` but no preference read
  (`crates/router-runtime/src/sandbox.rs:430`). Run v3 added
  `(allow user-preference-read (preference-domain "com.openai.codex"))` and
  shared-memory reads named `apple.cfprefs.*`; whether the shared-memory rule
  is required was not isolated.
- The npm launcher `codex.js` is a `#!/usr/bin/env node` script and exits 127
  under `PATH=/usr/bin:/bin` (`crates/router-runtime/src/sandbox.rs:318`, all
  three runs, launch `V0`). A program entry must name the native executable
  that `codex.js` spawns.

These findings belong to the provider-egress ADR or to a runtime profile
change; this ADR does not decide them.

## Runtime startup amendment, 2026-09-30

Native Codex and Claude processes receive a fresh owner-only host directory
inside the runtime store for each attempt, including read-only attempts. Only
that directory is added to child read/write roots. A read or write root covering
the store, its approval key, or an unrelated store child is refused. The state
is removed on preparation failure and after supervised completion; a killed
router can leave a private orphan, never reused by another attempt.

The runtime owns `CODEX_HOME` and `CLAUDE_CONFIG_DIR`: caller values for these
names are refused, not silently replaced. It does not copy personal host config,
OAuth credentials, keychains or refresh tokens. Explicitly approved
`OPENAI_API_KEY` is written as the sole field of a private 0600 Codex `auth.json`;
Claude receives the approved `ANTHROPIC_API_KEY` through its environment. Without
an API key, state stays empty and authenticated provider access is unverified.
The Codex file shape follows [AuthDotJson at rust-v0.159.2](https://github.com/openai/codex/blob/rust-v0.159.2/codex-rs/login/src/auth/storage.rs).
Copying OAuth credentials is rejected because a refresh in a disposable copy
could invalidate the original session. OAuth support requires a separate
credential ownership and refresh design.

Approval domain v5 binds this state policy in addition to all v4 inputs. Existing
approvals must be previewed again. The random directory is runtime-owned output,
not an extra caller-controlled grant. macOS and Linux use the same validation
and environment construction; only their enforcement primitives differ. These
changes do not open network access or establish any host capability.


If host-state cleanup fails (for example, the child removes directory
permissions), a completed attempt is recorded as failed and the CLI reports
`runtime.cleanup.unknown` (exit 6). A version probe cannot succeed with failed
state cleanup. Inspect the private orphan before retrying; no automatic OAuth
recovery or credential write-back is attempted.

## Consequences

A write phase becomes routable on a host only after a canary succeeded on that
exact binary, version, adapter template and sandbox profile, and after the
spike showed that the host can run under the chosen profile. If both hosts need
provider egress, nothing here routes a real write phase until the egress ADR is
accepted. Updating Claude Code or Codex voids the proofs and puts writes back
to UNKNOWN until a new canary runs. Each canary costs one real model turn; that
cost is not measured.

After batch 8, these stay UNKNOWN: `Tools`, `Subagents` and `Vision`; external
egress, DNS and TLS under `Network`; account or plan changes and server-side
feature flags, which no bound field observes; Linux parity if the canaries only
run on macOS; modules loaded by a script host; capability evidence in a replay
exported without its store; and any claim that a model other
than the canary model writes as reliably. A proof shows that the host wrote
once under these conditions, not that every run will.

Tests and fixtures that build `host_runtime` sources by hand (listed by
`git grep -n HostRuntime -- crates`) need a test verifier once batch 6 lands.

## Rejected options

- **Catalog capability manifest per host version.** A signature shows who wrote
  the claim, not that this binary, with these flags and this sandbox, performs
  it. ADR 0016 already refuses `supported` claims in descriptors. A manifest may
  only suggest which canaries to run.
- **Documentation or `--help` parsing.** Presence is not support (ADR 0002,
  "Consequences"), help text is untrusted input and changes between releases.
- **Operator attestation as `supported`.** It turns a declaration into the
  same value as an observation and hides the difference in receipts.
- **Reusing the 300 second snapshot bound for proofs.** It would require a paid
  canary before every run.
- **Canaries with `network: true` while provider egress is unresolved.** A
  write proof would then only match network-open runs, so every routed write
  would open the network without the phase requiring it.
- **Latest valid proof wins over a newer failure.** It keeps a capability
  `supported` after the host was seen failing under the same binding.
