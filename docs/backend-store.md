# Durable backend activation store

`router-backend-store` persists externally signed promotion evidence and activation
history. It does not generate signing keys, contact a model, install a backend or
activate a policy automatically. The store implements
`router-execution-decision::ActivationAuthority`; its shared descriptor lock
remains held until the dispatch guard is dropped after process creation.

## Parent authority and public API

`StoreConfig` schema 1 contains `baseline_policy_hash`, `scope_sha256` and public
`AuthorityKey` entries (`role`, `id`, `public_key_hex`). Each of the four roles is
required. Keys must be valid Ed25519 public keys, physically distinct, with no
duplicate role/id. Ordering is canonical before hashing. `from_json` rejects
unknown fields, repeated keys and inputs over 64 KiB. No private key belongs in
this document or in the store.

- `BackendStore::create(path, config)` creates a new store below an existing
  parent directory. An existing final path is refused, including a partially
  initialized store. It returns a handle with `config_hash()` for recording in
  independently trusted parent configuration.
- `BackendStore::open(path, expected_config_hash, expected_head)` requires an
  independently supplied configuration digest. Never obtain that expected digest
  by reading the same store being opened. A changed baseline, scope, public key,
  role or key identity invalidates the digest.
- `review(artifact, evidence, now)` reruns signed held-out, subgroup, shadow and
  canary gates and returns opaque `ReviewedPromotion`. It writes nothing and
  consumes no approval nonce. Its serializable projection contains hashes,
  counts and expiry only.
- `activate(signed_approval, reviewed, now)` revalidates the complete promotion
  at the current activation time, verifies the current state hash, signature,
  nonce and expiry, then atomically commits evidence and transition together.
- `rollback(signed_approval, now)` verifies an explicit rollback to the pinned
  deterministic baseline. Expired historical evidence does not prevent rollback.
- `status(now)` returns hashes, event/promotion counts, expiry and whether an
  external checkpoint was checked. It never exports prompts, observations,
  model names, public keys, signatures or source labels.
- `read_guard(now)` returns a concrete `StoreGuard` implementing `ActivationGuard`.
  The production `ActivationAuthority::acquire()` uses the current system clock,
  rereads state under the lock and maps failures to `AuthorityUnavailable`.
- `historical_guard(state_hash, original_time)` reconstructs a fully verified
  predecessor as `HistoricalActivation`. This implements `ActivationGuard`
  only, never `ActivationAuthority`. Use it to recreate an old opaque decision
  for status, verification or reopen after rollback. The exact head supplies
  logical order; timestamps have one-second resolution, so equality with the
  following transition is accepted. Times strictly outside that head's interval
  fail. Such a reconstruction cannot replace `acquire_dispatch` against the
  current `BackendStore`, including when activation and rollback occurred in the
  same second.

`expected_head` is an optional independently retained **checkpoint**, not a
self-authenticating value from the snapshot. It must equal the verified current
head, genesis or a verified predecessor. Signed successors are allowed; a history
that loses the checkpoint fails. Keep the latest trusted checkpoint externally
when rollback detection is required. A checkpoint older than the current head
cannot detect rollback to a later accepted prefix.

## Transaction and locking

The private directory is 0700 and files are 0600. Every existing directory
component is opened with descriptor-relative `O_NOFOLLOW`; the final directory
must belong to the current user. Files must be regular, owned by that user and
have a single hard link. Symlinks, FIFOs, broad modes and changed lock-file
identities fail. Root and lock descriptors use close-on-exec. Renaming a parent
cannot redirect an already-open descriptor into another directory.

`activation.lock` has a stable inode. Each operation opens its own descriptor:
readers use nonblocking shared `flock`, writers nonblocking exclusive `flock`.
No retry loop or unbounded lock wait is hidden in the library. Busy means retry
later with fresh state and authority. Separate processes and separate store
handles observe the same lock. Multiple readers are allowed; a reader retained
for dispatch prevents activation or rollback until its guard is dropped.

The runtime must acquire the guard, validate the opaque execution decision using
its **current** state and roots immediately before spawning, retain the guard
through process creation, then release it. Holding an earlier clone or releasing
the guard before spawning does not meet this contract. A rollback while a job is
queued invalidates that old decision; rollback cannot interleave with the guarded
spawn section.

The durable transaction is one typed `state.json` snapshot containing config,
all retained promotion evidence, signed events, current head and an integrity
hash. Under the exclusive lock the writer:

1. reads and fully verifies the current snapshot;
2. compares the signed approval's expected head and checks replay/expiry gates;
3. constructs the entire successor in memory;
4. writes private `state.pending` and syncs its contents;
5. compares the previous snapshot bytes again;
6. renames the candidate over `state.json` within the same directory and syncs
   the directory.

A failed candidate never changes the published head. A crash before rename leaves
the previous snapshot authoritative. A complete renamed snapshot is verified on
restart. A crash between rename and directory sync follows the filesystem's
usual durability boundary; the library does not promise which of the two valid
snapshots survives a power loss. A final directory-sync error after the rename
returns `backend_store.published_durability_unknown`, distinct from
`backend_store.io` (nothing published, including a failed rename) and from every
earlier failure: the new head may already be visible and its durability is
UNKNOWN. Reopen and inspect the head before retrying, and never blindly generate
another approval. Policy: ADR 0017
(`docs/adr/0017-bounded-retention-and-publish-durability.md`).

Bounds: `MAX_EVENTS` (128) is a lifetime cap on activation events, and
`MAX_PROMOTIONS` is derived as `MAX_EVENTS / 2` (64). An activation requires no
active promotion, so each promotion costs at least an activate and a rollback
event; the event cap is the binding limit and refuses further activations with
`backend_store.limit`. Nothing is pruned: replay from genesis needs every event
and promotion to rebuild used nonces and artifact versions, and dropping either
would reopen nonce and version replay. Checkpoint compaction (a signed restore
from a snapshot with nonce and version carry-over) is deferred; see ADR 0017
(`docs/adr/0017-bounded-retention-and-publish-durability.md`).

A leftover staging file is ignored by readers. The next exclusive writer removes
only that exact private, regular, single-link `state.pending` before staging its
own candidate. It never scans or deletes arbitrary files. No backup, retention
purge or history reset is implicit.

## Verification, bounds and trust limits

Snapshot JSON rejects unknown fields and duplicate keys, including nested maps.
Limits are 64 MiB per snapshot, 16 retained promotions and **128 signed events**.
Hitting a limit fails without dropping old evidence or reusing nonce space.
Promotion documents retain the promotion library's bounds. Restoring a snapshot
replays signatures and gates for every retained promotion at its original
activation/validation time, then replays every activation event at its original
time. A promotion must be referenced by its first activation at that exact time.
Current dispatch still checks actual proposal expiry, current state, policy,
context, selected candidate and current trust roots. History reconstruction does
not renew an expired promotion. Reads therefore have bounded verification work,
not a constant-time or low-latency guarantee.

Hashes and a private directory detect accidental corruption and constrain
cooperating processes. They do not defeat an adversary with arbitrary writes as
the same Unix user, who can replace an entire valid history or interfere with
filesystem locks. An independent checkpoint is necessary to reject rollback
below that checkpoint. A process with authority to replace both the store and its
external pin is outside this boundary. OS permission checks are not evidence of
host sandbox enforcement.

External signatures authenticate submitted reviewer/trial attestations. They do
not manufacture human labels or establish that an external trial occurred. Tests
use synthetic keys and fixtures. The active field in storage does not authorize
execution by itself; existing parent approval, source state, sandbox and v3 asset
gates remain required. Other platforms fail explicitly until an equivalent
locking and path contract is implemented.

## Validation

The crate tests cover private creation, duplicate ownership of initialization,
read-only review, activation and restart, expired historical evidence and
rollback, stale CAS/nonces, cross-process shared/exclusive contention, revocation
between review and dispatch, independent root/checkpoint mismatch, corruption,
truncated and oversized JSON, interrupted staging, symlinks/hardlinks/modes,
replaced lock identity, redacted status and the full 128-event boundary. The
historical tests reconstruct the identical original decision after rollback,
including same-second transitions, and prove current dispatch still refuses it.
The filesystem transaction has no provider or global integration dependency.

```sh
cargo test -p router-backend-store --locked --offline
cargo clippy -p router-backend-store --all-targets --all-features --locked --offline -- -D warnings
```

## CLI lifecycle

The CLI uses `agent-router backend-activation` with `init`, `status`, `review`,
`activate` and `rollback`. All commands require `--json` and an independently
retained `--expected-config-hash`. Existing-store commands optionally accept
`--expected-head` as the checkpoint described above. There is no clock override,
private key generation or signing command.

```sh
agent-router backend-activation init --store /absolute/private/store --config public-authority.json --expected-config-hash EXPECTED_CONFIG_HASH --json
agent-router backend-activation status --store /absolute/private/store --expected-config-hash EXPECTED_CONFIG_HASH --json
agent-router backend-activation review --store /absolute/private/store --expected-config-hash EXPECTED_CONFIG_HASH --artifact backend-artifact.json --evidence promotion-evidence.json --json
agent-router backend-activation activate --store /absolute/private/store --expected-config-hash EXPECTED_CONFIG_HASH --artifact backend-artifact.json --evidence promotion-evidence.json --approval signed-activate.json --json
agent-router backend-activation rollback --store /absolute/private/store --expected-config-hash EXPECTED_CONFIG_HASH --approval signed-rollback.json --json
```

The example hash placeholder is deliberately not an accepted digest. Compute and
retain the real hash from the separately reviewed parent configuration. `init`
compares it before creating a directory. `review` emits the promotion hash for
external approval, but does not activate it. A signer outside this CLI binds the
current state hash, target promotion or baseline, action, nonce and expiry in
`Signed<ActivationApproval>`. `activate` reruns evidence validation and rereads
wall time before applying the signature. Reusing a stale approval fails CAS.

Input reads are bounded and reject final symlinks and nonregular files. Config
and artifact inputs are limited to 64 KiB, approvals to 16 KiB; evidence is also
subject to the strict promotion parser's effective 4 MiB JSON limit. Raw input,
prompts and key material never appear in CLI success output or error text. A
successful activation stores authority for the explicit semantic execution path;
it does not start a process or remove any runtime approval or asset gate.
