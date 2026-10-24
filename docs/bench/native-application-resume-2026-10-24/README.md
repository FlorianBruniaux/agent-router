# Explicit native application resume, 2026-10-24

The product command `finite-resume` applies an accepted private candidate after
controller death only when its original preimage and authority remain intact.
A new approval belongs to the current process. The old intent and attestations
remain in history, and a separate resume intent is consumed before writing.

The production Rust source is signed commit `da9fd4383055715e333f3393dbcc0766631d8261`.
The native fixture at `f2f0ce43b0fbcd5f219181e8b87bd61282151ce2` adds five evidence
saves without changing product code or assertions. The Linux binaries were built
from the exact da9fd438 archive; the fixture revision is recorded separately.

## Observed native behavior

Each case begins with a real finite native execution over scripted public input,
then observes SIGSTOP at the selected write boundary and kills/reaps the original
controller. The new product command reopens the original synthetic admission.

| Case | Observed result |
| --- | --- |
| `kill_before` | Old process approval refused; fresh approval applies exact candidate once; replay refused. |
| `kill_after` | Candidate bytes observed, original completion absent; fresh write approval refused. |
| `kill_partial` | Exact authored 227-byte short write leaves a mixed file; no repair or write approval. |
| `resume_interrupted` | A fresh resume intent is committed, then the controller dies before writing; next invocation reports consumed and refuses replay. |
| `later_edit` | A later authored edit with mode 0600 is reported divergent and preserved by a new invocation. |

Every case also refuses a wrong admission policy and preserves a subsequent
mode-0400 authored edit. These are scripted edits, not human acceptance. The
successful case preserves other packet files, modes, HEAD, logical index and
refs. Original event prefixes and acceptance records remain unchanged. The
completed resume adds only its intent and completion events.

## Verification

The five local gates pass: format, warnings-denied workspace Clippy, workspace
tests, harness parity and Gitleaks. Workspace logs report 2,004 passes, zero
failures and 43 ignored tests, including nested helper output. ADR validation
also passes. Linux runs 15 application tests plus one nested helper and two CLI
tests successfully before native qualification.

All five native cases pass. Their 311 retained files are hash-checked. The build
cell and five native cells are stopped. Every cell retains the 8,589,934,592-byte
memory limit, zero swap, disabled network, read-only root, no host mounts and
existing private-access restrictions. Build peak is 1,823,424,512 bytes; maximum
native peak is 811,249,664 bytes. No memory-limit event or OOM is recorded.
Independent source, fixture, build, native-gate and post-run reviews pass. The
reviewer verifies retained evidence without rerunning the native cases.

## Limits

Only an intact original preimage can resume, once. Legacy intents without a
persisted controller context, changed metadata or Git state, partial writes and
later edits are refused. Candidate bytes without a completion record remain an
observation. A second interruption leaves the resume consumed. The operation is
not crash-atomic and does not qualify power-loss recovery or arbitrary same-user
concurrent writers.

There is no model, provider account, real registry, publication, human acceptance
or savings claim. #132 and #11 remain open. Remote CI is separate from these
local and native results; #208 and #209 remain unmerged dependencies.

## Evidence

- [Source and fixture identities](source.json), [local gates](local-checks.json), [Linux build](linux-build.json).
- [Native cases and limits](native-results.json), [source/history observations](native-evidence.json), [retained file manifests](retained-manifests.json).
- [Independent review identities](independent-reviews.json).
- [Command protocol](../../reference/finite-application-resume.md) and [ADR 0047](../../adr/0047-explicit-finite-application-resume.md).

Full raw records remain retained locally. Published summaries preserve their
hashes and distinguish candidate acceptance, current source and completed write.
