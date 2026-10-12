---
adr: "0032"
title: "Bind producer-owned finite-client startup state"
status: implemented
created: 2026-10-11
last_modified: 2026-10-11
scope: "Private startup preparation and Linux diagnostic only; no native execution authority"
amended_by: []
supersedes: []
superseded_by: []
related: ["0029"]
---

# ADR 0032: Bind producer-owned finite-client startup state

## Decision

`OwnedBroker::prepare_codex_state` initializes credential-free startup state before presenting an owned-copy review. It creates a fresh UUID-shaped installation ID in `tmp/installation_id`, mode 0644, under the existing private mode-0700 temporary directory. The exact relative symlink `installation_id -> tmp/installation_id` lets the pinned native client open its installation slot without granting writes to the configuration parent. No personal ID or credentials are imported. Mode 0644 matches Codex 0.159.2's startup contract; the private ancestor prevents access by other users.

Retain descriptors for both the regular installation slot and the symlink, together with their device/inode/mode/owner identities. Linux opens the symlink with `O_PATH | O_NOFOLLOW`; macOS uses `O_SYMLINK`. Other platforms refuse this state contract. Checks reject changed bytes, permissions, target or identical-byte inode replacement before review, proof validation and preview. The owned HostState lifecycle releases the new slot and alias with approval.

The copy review advances to `finite-client-copy-review-v2`. Its new `finite-codex-startup-v1` digest binds the temporary directory, installation path/hash/identity, alias identity, relative target and SQLite environment key. The retained home and generated configuration remain bound by the prior review. Owned-copy capability binding advances to `finite-client-contract-v4`; authenticated v2 or v3 proofs cannot validate it. Manifest-only preparation remains v2. Old owned callers need fresh evidence, not silent proof reuse.

The private producer derives exactly PATH, HOME, CODEX_HOME, TMPDIR and CODEX_SQLITE_HOME. Both temporary locations resolve under the owned home. The serialization-only macOS preview gains SQLite's location, without becoming a process grant. Native compatibility of this composition is qualified only under Linux in this lot. The finite diagnostic client accepts the producer's exact five-key map as part of its hashed stdin and refuses missing, redirected or extra variables before effects.

## Qualification and authority

The opt-in Linux controller consumes this producer directly, with no fixture-created installation ID. The [dated startup campaign](../bench/linux-owned-startup-2026-10-11/README.md) reuses the retained Landlock/seccomp and external namespace fixtures, unchanged helper limits, broker port rules and protected-copy canaries. It checks startup and copied identities after execution and keeps ownership until external namespace teardown. The isolated client submits only initialize and thread/start, with no model turn.

The symlink is a known producer artifact, not an arbitrary input symlink exception. The target is writable temporary state; changed content causes later validation to refuse. Descriptor checks are preparation freshness, not atomic exclusion against a trusted same-UID race. The diagnostic OS policy protects the home parent and copied assets, but no new production backend or capability proof is granted here.

F12a stays NO_GO. Durable PendingRun/ApprovedRun dispatch, an approved namespace owner with retained leases, fresh authenticated OS capability evidence and broker admission remain required before a simulated provider task. Production ControllerBrokerOnly still refuses. The 8589934592-byte resource default, protocol, approval store and observation schemas remain unchanged.

## Verification

Contract tests exercise the real producer and authenticated proof validator, including corruption before review, slot and alias substitutions, stale v2/v3 bindings and exact preview environment. Diagnostic parser tests refuse redirected home/tmp/SQLite paths and LD_PRELOAD. The external four-case campaign covers normal completion, deadline, EOF and helper death, with detached activity before teardown and an independently stable heartbeat after namespace destruction. Source/build/campaign hashes and the local verification results live in the dated report; they do not establish a useful patch or savings.
