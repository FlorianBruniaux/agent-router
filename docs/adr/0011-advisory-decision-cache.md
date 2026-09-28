# 0011: Authenticated advisory cache of backend proposals

Status: accepted for the `router-advisory-cache` library. No CLI wiring yet.

## Context

Backend proposals are recomputed per phase. A cache can save a backend call, but
a cached decision must never become authority. Review of `router-decision`
found four hazards:

- `validate_proposal` (`crates/router-decision/src/lib.rs`) checks structure
  only, so an admissible but forged proposal on disk would be believed.
- `BoundedDecisionContext::from_phase` sorts and truncates to
  `MAX_BACKEND_CANDIDATES` before any backend runs. A cache that replaced this
  evaluation could hide a better fresh candidate.
- `SearchResult` in `router-retrieval` may carry text and must not be persisted.
- Candidate aliases `cNN` are regenerated per context, so an alias means nothing
  without the table that produced it.

## Decision

Add a new library crate, `router-advisory-cache`, following the private-file
rules of `router-backend-store` (`openat` with `NOFOLLOW`, directory `0700`,
files `0600`, staged write then `renameat`). It owns its serialized entry format,
so no type is added to `router-types`. No SQLite and no new descriptor-holding
state.

- Opt-in and advisory. Only a `BackendProposal` is stored: an alias, confidence
  and digests. No receipt, grant, permission, outcome, prompt, task text or body.
- The entry is authenticated with HMAC-SHA256 (`hmac` 0.12, on the `sha2` 0.10
  already in the workspace). The caller supplies the key, at least 16 bytes. It is
  never stored, serialized or printed. `verify_slice` compares in constant time.
- The key name is SHA-256 over length-prefixed fields: host, host version,
  snapshot digest, policy hash, catalog hash, backend, router version, the exact
  bytes of the redacted `BackendRequest` (task kind, risk, phase and the
  context-bound artifact digest) and a new `BoundedDecisionContext::alias_digest`.
- `advise` recomputes the key from a fresh context and resolves the hit through
  `selection_for`. The cache never reduces the set evaluated by `from_phase`.
- Absent, expired, malformed, mismatched, unauthenticated, unreadable or
  rejected entries are all `Miss`. Lookup never returns an error and never blocks
  routing.
- Writers are serialized by a non-blocking exclusive `flock` on `cache.lock`, as
  in `router-backend-store`. A writer that finds it held gets `Busy` at once and
  never stalls; the caller may ignore it or retry. It is chosen over blocking so
  that a stuck holder cannot hang the caller. The lock does not check the
  device and inode of the lock file: replacing it needs an attacker with the
  same uid, outside the threat model of a `0700` directory.
- Under the lock, a writer publishes through one staging file and `renameat`. A
  live, authenticated entry is never replaced: a loser that gets the lock later
  receives `Exists`. An entry that is expired,
  malformed, unauthenticated, unreadable, a symlink or of the wrong mode is
  replaced, so a planted or stale file cannot block its key. Readers take no
  lock and see either the old or the new file.
- Under the lock, orphaned `*.pending` files are removed (`unlinkat` never
  follows a link). Only `*.json` names count toward the 1024 entry bound, which
  is enforced by a scan on a fresh descriptor.
- Reads are bounded to 16 KiB and the lifetime to 24 hours. Expired entries are
  removed only when their key is stored again.

## Key handling

- Domain separation: the cache key uses the label `advisory-cache-key-v1` and
  the entry MAC uses `advisory-cache-entry-v1`, each field length-prefixed. The
  caller must not reuse the HMAC key for anything else.
- `MacKey` is held in `Zeroizing` memory and wiped on drop. The `hmac` state
  built for each operation is not wiped: not done.
- Files are created with modes `0600` and `0700` through the process umask. A
  stricter umask yields modes the exact-mode check rejects, so storing fails
  closed. A looser umask cannot widen the modes, which are requested minimal. This umask behavior is not tested.
- `alias_digest` is redundant with the artifact digest through the public API,
  because that digest covers the whole phase request. It is kept as a second
  binding, and tested inside `router-decision` where the table can differ behind
  an identical redacted request.

## Consequences

- Not verified: multi-process contention (tests use threads, whose locks are
  separate open file descriptions like separate processes, but this was not run
  across processes), non-Unix platforms and crash recovery beyond orphan purge.
- A directory full of expired entries under different keys refuses every new
  key (`Limit`), since expired entries are removed only when their own key is
  stored again.
- A full directory refuses new entries until an operator clears it or an
  expired entry's key is stored again.
- Confidentiality of proposals is not provided, only integrity and binding.
- The cache is only as trustworthy as the key custody of its caller.
