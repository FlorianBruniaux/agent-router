---
adr: "0005"
title: Pin durable backend authority through dispatch
status: implemented
created: 2026-09-27
last_modified: 2026-09-27
scope: "`router-backend-store`. The runtime validates the dispatch guard before spawn and releases it right after the spawn attempt."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0005: Pin durable backend authority through dispatch

An in-memory promotion proof can become stale while a run waits in a queue.
Persisted JSON cannot itself be an execution capability. The application reloads
signed state under a shared OS lock, reconstructs its opaque proof, validates it
immediately before process creation and retains that lock until the child is
spawned. Activation and rollback take the same lock exclusively, across processes.
Guard destruction explicitly unlocks its open file description: closing the
parent descriptor alone can leave the lock held in a concurrently forked child
until exec. CLOEXEC is retained, and failed post-lock checks use the same guard.

The store uses a bounded typed snapshot with atomic replacement, rather than
adding a second SQLite database for at most 128 activation events. Every path
operation remains relative to open no-follow directory descriptors on macOS and
Linux. This avoids reopening a SQLite database through a mutable pathname or
claiming portable write semantics from `/dev/fd`. Private staging, file sync,
rename and directory sync form one publication transaction; CAS, signature and
nonce checks precede it. No history is silently purged at the limit.

Configuration fixes the baseline policy, authority scope and externally trusted
public keys. Opening requires an expected configuration hash from outside the
store. An optional independently retained history checkpoint detects truncation
below that checkpoint while permitting valid signed successors. Arbitrary
same-user rollback without an external checkpoint remains outside the proof.

Restoring expired history revalidates evidence at its original validation times.
Dispatch still checks current expiry and current authority. Status exports only
hashes, bounded counts and expiry. No private signing key or automatic activation
is introduced. The locking contract is expressed by `ActivationAuthority` and
`ActivationGuard` rather than a trusted boolean or a deserialized report.
