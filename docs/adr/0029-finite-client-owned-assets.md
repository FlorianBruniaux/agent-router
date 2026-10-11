---
adr: "0029"
title: "Retain reviewed finite-client asset copies"
status: implemented
created: 2026-10-11
last_modified: 2026-10-11
scope: "Owned preparation copies and refreshed preview only; no immutable OS view or native launch"
amended_by: []
supersedes: []
superseded_by: []
related: ["0028"]
---

# ADR 0029: Retain reviewed finite-client asset copies

## Decision

The producer-owned Codex preparation creates five private copies before the
approval digest is presented: client, native, helper, wrapper and sandbox
profile. Reuse `PrivateProgram` for bounded copying and hash verification.
Executable copies use mode 0500; the profile uses mode 0400 and a 64 KiB ceiling.
Each copy lives in a separate owner-private directory outside the task workspace.
The configuration remains `config.toml` in the producer-owned Codex home, mode
0400, because moving it elsewhere would change its native consumption contract.

The review retains open descriptors and device/inode/mode/owner identities for
the copied files and their directories. The existing source pins still receive
fresh checks, so copies do not weaken source substitution refusal. Failures
during preparation release the owned copies. Dropping approval releases the
copies and the existing producer-owned state; this is preparation cleanup,
not observed cleanup of an executed worker. A retained directory descriptor
also protects against inode reuse. Explicit cleanup refuses a substituted
directory; Drop preserves it and leaves moved owned bytes for reconciliation.
The pathname check followed by deletion is not atomic against a trusted
concurrent writer, and does not qualify worker exclusion.

The owned preparation hashes the prior review digest plus all six effective
asset paths/hashes and copied identities under `finite-client-copy-review-v1`.
Its capability binding uses `finite-client-contract-v3`. Manifest-only review
retains v2 and cannot present a native launch plan. Previous v2 proofs cannot
qualify the new owned-copy review.

The macOS serialization-only preview now derives helper, wrapper, profile,
client and native paths from those retained copies. Its `read_only_assets`
array declares the six assets that a future qualified supervisor must protect,
in this order: client, native, helper, wrapper, profile, configuration. It is a
protection requirement, not an enforcement receipt. All copies, originals,
private state and workspace are checked again before presenting the preview.

## Authority and remaining qualification

There is no Command conversion, launch method, provider send or new permission.
`PendingRun` / `ApprovedRun`, the authenticated proof store and verified OS
supervisor remain required. A serialized preview cannot reconstruct ownership.
Other OSes still refuse this preview; preparation itself has portable Unix
checks. No journal, observation format, helper protocol or resource default changes.

Mode 0500/0400 and hashing do not exclude another same-UID writer. The selected
OS policy must prevent worker writes to copy directories and configuration,
including unlink/rename and writes through the Codex home. Native compatibility
of copied system wrappers is also unqualified. The copied profile is preserved
byte-for-byte and is not rewritten to grant reads to new paths. A subsequent
qualified template must explicitly bind those paths before any launch.

F12a remains NO_GO. The live macOS default-memory probe of a copied helper
returns `sandbox-helper.setrlimit.address_space` with exit 125. Network
`ControllerBrokerOnly`, task-level containment, Mach/DNS, physical-send admission
and native lifecycle composition remain separately qualified requirements.
No native model turn, useful native patch or saving is established.

## Verification

Contract tests cover effective invocation paths, copied bytes and modes,
ownership cleanup, content and symlink substitutions for all six assets,
identical-byte inode replacement, source freshness and separate v3 binding.
The fixture uses synthetic files and the existing authenticated proof store;
it does not establish executable or native compatibility. Existing state,
approval, proof freshness and resource-limit checks remain in the same test target.

See the source-bound report in
`docs/bench/verification-finite-client-assets-2026-10-11.json` for observed checks
and the explicit lot-3 stop condition.
