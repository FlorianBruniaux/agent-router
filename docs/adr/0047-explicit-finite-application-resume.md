---
adr: "0047"
title: Explicit finite application resume on an intact preimage
status: proposed
created: 2026-10-24
last_modified: 2026-10-24
scope: "Existing private finite output; one fresh approval; no partial-write repair"
amended_by: []
supersedes: []
superseded_by: []
related: ["0043", "0045", "0046"]
---

# ADR 0047: Explicit finite application resume on an intact preimage

## Context

An interrupted application can consume its intent before writing any byte.
ADR 0046's original in-memory authority does not survive controller death.
An old approval must remain consumed and later user edits must be preserved.

## Decision

New applications persist `integration.context.v1` atomically with their original
`integration.intent`, before writing. The context binds the canonical private
sibling source, directory identity, run definition and execution approval,
original review, and a bounded source snapshot. Legacy intents without context
cannot reconstruct application authority.

`Runtime::finite_resume` reopens only this context. `finite-resume --config ...
--run-id ... --json` inspects an existing runtime with its original admission
configuration. The runtime's exclusive store flock remains held. Opening may
recover store state; inspection does not write the source or append a resume
intent. No provider or worker is started and no registry is created by the CLI.

Only the exact original preimage is eligible: directory and file identity,
owner, mode, link count, modification/change timestamps, all flat packet file
hashes, clean original Git revision and logical index tree must match. The
Git metadata is fingerprinted before any Git subprocess; changed configuration,
includes, alternates or symlinks refuse. This scan is bounded to 256 entries
and 8 MiB total, with at most 1 MiB per metadata file. The
current explicit candidate-acceptance record must still match the original
review. Bounds remain 16 files and 32768 bytes per file.

With `--approve`, the command emits a fresh `resume.review` and waits at most
30 seconds for a JSON line with schema_version 1, kind `resume.approve`, and
its approval_digest. The digest binds original approval, current source snapshot,
chain tip, observed/candidate hashes and a random handle nonce. It cannot be
used by another invocation. The library handle expires after 60 seconds.

An immediate transaction compares the chain tip and requires the original
context/intent pair with no later integration event, then appends the sole
`integration.resume_intent.v1`. Final descriptor/snapshot checks precede an
in-place write of at most 32768 bytes. Sync and byte/identity checks precede
`integration.resume_applied.v1`. Original attestations and intent remain intact.

## Consequences

Candidate bytes observed without a terminal event are not historical proof of
completion. Partial writes, changed files, changed metadata, and uncertain or
consumed resumes receive no write approval. A second interruption consumes the
resume permanently. There is no automatic repair, rollback, or replay.

The exclusive-controller source ownership assumption remains unchanged. These
checks do not lock out arbitrary same-user filesystem writers or detect every
possible history outside the process. A file with identical bytes but changed
modification/change metadata is deliberately refused.

Native qualification is separate from unit tests. It retains the 8 GiB limit,
zero swap, existing closed private access, network-disabled outer cell, and
scripted public fixtures. Neither a candidate acceptance nor a resume terminal
establishes human acceptance, savings, or general task completion.

## Rejected options

- Reconstructing authority from a caller-supplied source path or a legacy intent.
- Reusing the original approval or approving across separate handle lifetimes.
- Repairing a partial file or treating candidate bytes as a proven application.
