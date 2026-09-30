---
adr: "0021"
title: Host capability evidence from supervised canaries
status: proposed
created: 2026-09-30
last_modified: 2026-09-30
scope: "Design only. No code implements it; every real host capability stays UNKNOWN until batch 4 lands."
amended_by: []
supersedes: []
superseded_by: []
related: ["0002", "0003", "0016"]
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
  meets the same rejection.
- A snapshot may claim `supported` only when a source of kind `host_runtime`
  backs it (`crates/router-adapter-api/src/lib.rs:251-262`). No production code
  emits that kind: it appears only in tests, examples and a documentation
  fixture (`git grep -n HostRuntime -- 'crates/*/src/*'` returns the enum and
  the check).
- The kind is provenance, not authentication
  (`crates/router-adapter-api/src/lib.rs:32-33`). `run` reads the snapshot from
  a caller file (`crates/agent-router-cli/src/execution.rs:33`). A hand-written
  snapshot that labels a source `host_runtime` would therefore pass the
  structural check.
- Fresh execution accepts a snapshot and each of its sources for 300 seconds
  (`crates/agent-router-cli/src/execution.rs:349`,
  `crates/router-adapter-api/src/lib.rs:310-320`).
- The approval digest already binds the SHA-256 of every configured program,
  the sandbox helper and `/usr/bin/sandbox-exec`
  (`crates/agent-router-cli/src/execution.rs:706-718`). The Seatbelt supervisor
  runs its own deny/allow canaries before any run
  (`crates/router-runtime/src/sandbox.rs:174-240`).
- Only the Copilot and OpenCode adapters check a fresh `--version` against the
  approved pin (`crates/adapter-copilot/src/lib.rs:130-133`,
  `crates/adapter-opencode/src/lib.rs:132-135`). Claude and Codex keep the
  default `false`.
- The Claude probe requires the `init` model to appear among the models of
  root assistant messages (`crates/adapter-claude/src/probe.rs:96-103`). The
  fixtures use the same synthetic id on both sides
  (`crates/adapter-claude/tests/fixtures/claude.jsonl:1-2`). A real 2.1.285
  stream announced `claude-haiku-4-5` in `init` and a dated id in the assistant
  message, and `inspect` failed with `probe.cache.invalid` (reported by the
  pilot agent on 2026-09-30, not reproduced here).
- `prepare` maps every `router_prepare` error to `prepare.invalid_or_denied`
  (`crates/agent-router-cli/src/preparation.rs:219`). The only `assets`
  subcommand is `review` (`crates/agent-router-cli/src/command.rs:553-566`);
  `seal_contracts` and `seal_availability` are library functions called from
  tests only (`docs/asset-contracts.md:29-30`), while new execution approval
  requires a v3 asset authority (`docs/cli-execution.md:74-75`).

## Decision

### Evidence source

Only a supervised canary produces `supported`. A canary is a native run of the
host, under the verified OS sandbox, on a throwaway directory created by the
router inside its private store. The router chooses a random nonce, gives the
host a fixed instruction, then checks the observable effect itself.

- `WorkspaceWrite`: the host must create one named file whose content equals
  a second nonce. The router checks the exact bytes and that no other path in
  the directory changed. A missing file, wrong bytes or an extra path produce
  no proof.
- `Network`: the router serves one nonce on a loopback port it opened for the
  canary. The host must fetch it and write it to the named file. No external
  domain is contacted by the canary.
- `Tools`, `Subagents` and `Vision` get no canary in this ADR.

A failed canary records the attempt and leaves the capability `unknown`. It
never produces `unsupported`, because one failed model turn does not show that
the host lacks the capability.

### Binding and freshness

A proof binds, in one canonical record:

- the harness id, the host binary SHA-256 and the output of a supervised
  `--version`;
- the adapter id and the digest of the argument template it prepares, which
  carries the host permission mode;
- the sandbox helper and wrapper digests, the profile shape (write roots
  relative to the workspace, network flag) and the OS;
- the model id observed in the canary stream, the capability, the nonce
  digests and the canary start and end times.

The router authenticates the record with an HMAC under a key derived from the
per-store secret (`crates/agent-router-cli/src/approval_key.rs:12`), with a
label distinct from the approval binding. A proof expires after
`DEFAULT_MAX_AGE_SECONDS` (86 400) unless the operator sets a shorter bound.
Any change to a bound field voids it before expiry. Claude and Codex adapters
gain a native `--version` check so that the version bound at canary time is
compared again at dispatch.

### Storage and use

The record format and its validation live in `router-adapter-api`, with the
other host evidence. Records are append-only rows in the runtime private store.

`inspect` gains an explicit store argument. It folds each valid, unexpired
proof into the snapshot as a `host_runtime` source whose `content_sha256` is
the proof digest. Snapshot validation checks those sources against the proof
lifetime instead of the 300 second snapshot bound.

`run` re-verifies every `host_runtime` source behind a `supported`
capability: the proof must exist in the same store, carry a valid HMAC, be
unexpired and match the binary digests and version pinned by the approval.
Otherwise it refuses with `capability.evidence.unauthenticated`. `route-phase`
keeps accepting caller snapshots, because routing executes nothing.

A new receipt schema version lists, per required capability, the proof
digest, its tier, its observation time and its expiry. Earlier receipt
versions keep their meaning. The run event log records the same digests.

### Operator attestation

An operator may record an attestation with the same binding, an identity label
and a free-text reason. It never becomes `supported`. It has its own tier,
`attested`, which the policy refuses by default. A phase policy can accept it
only by naming each capability explicitly, and the receipt shows the tier.
This tier is the last batch and may be dropped without changing the others.

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
4. Proof record, store and verification. With a fake host binary: a proof
   folded by `inspect` lets a write phase pass the policy gate; a hand-written
   snapshot with a `host_runtime` source is refused by `run`; changing one byte
   of the binary, the version output or the sandbox profile voids the proof; an
   expired proof is refused.
5. `WorkspaceWrite` canary command, with a digest-bound approval of its own.
   With a scripted fake host: correct write yields a proof; wrong bytes, an
   extra file or no file yield none. One real canary per host runs only after
   the owner authorizes the spend.
6. `Network` canary on loopback, same acceptance shape as batch 5.
7. Operator attestation tier: refused by default, accepted only for named
   capabilities, visible in the receipt.

## Consequences

A write phase becomes routable on a host only after a canary succeeded on that
exact binary, version, adapter template and sandbox profile. Updating Claude
Code or Codex voids the proofs and puts writes back to UNKNOWN until a new
canary runs. Each canary costs one real model turn; that cost is not measured.

After batch 6, these stay UNKNOWN: `Tools`, `Subagents` and `Vision`; account
or plan changes and server-side feature flags, which no bound field observes;
Linux parity if the canaries only run on macOS; and any claim that a model
other than the canary model writes as reliably. A proof shows that the host
wrote once under these conditions, not that every run will.

Not verified by this ADR: whether a native Claude or Codex run reaches its
provider when the sandbox denies the network. If it cannot, every native run
needs `Network`, and provider egress must be separated from tool network access
by a later decision.

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
