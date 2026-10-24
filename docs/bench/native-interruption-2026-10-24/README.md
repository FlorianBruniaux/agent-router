# Native owner interruption and Runtime reopening, 2026-10-24

This lot tests interruption of the finite controller after candidate acceptance. It preserves the distinction between an accepted candidate and a completed application. The fixed public fixture uses scripted responses and no model, account or real registry.

## Source and execution boundaries

PR #207 merged as `4a581faf4d96291292e1fb99432d9643bfab93d4`. Its four qualified Linux product binaries remain byte-identical to the build from `261eeeb82f054644599afa6e45bfc4cd18deb3ae`. The new source commit is `a352f5504050f74f25692d95f8d2be8bcc7a4e8e`.

The Rust observer and C interposer were compiled from tree `82fce7b4359cd6e7c397fbf605ad9afb4a44e8db`. The signed commit adds only the fixture README and receipt/report retention in the Python driver to that reviewed tree. Rust and C bytes are identical. Separate manifests identify both driver revisions and the old and new binaries.

The interposer stops only the exact owner's private `lib.rs` FD, after the parent arms it at the third approval. It uses SIGSTOP before `write`, or after successful `fsync`. The parent observes the stopped PID, reads the committed intent and file state, then sends SIGKILL and reaps that process. The native worker and its namespace must already be cleaned up.

The observer reopens the existing Runtime with `open_with_admission`, preserving the original synthetic registry binding, policy hash, runtime ID and retention settings. It does not reconstruct application authority. A second opening follows an authored simulation of a later user edit and file mode change.

## Verification

All three native cases pass. Both interrupted owners exit by signal 9 after observed SIGSTOP; the completed control exits 0. Each case retains six scripted worker requests and a distinct permission refusal, with zero model calls. Candidate acceptance remains Passed. The interrupted cases retain one integration intent and no applied event; the completed control retains both intent and applied evidence.

Six actual Runtime reopenings preserve the complete journal, source bytes, modes, HEAD and logical index. The second reopening in each case preserves the authored later edit at mode 0400. The native cell reports memory.max 8,589,934,592, swap.max 0 and memory.peak 821,370,880 bytes. Build and native cells are stopped without OOM. Independent source, driver, compiled-binary and post-run reviews pass with no remaining finding. The post-run reviewer independently checked all 133 retained files and the input program identities without replaying the pilot.

Local format, warnings-denied all-feature workspace Clippy, all-feature workspace tests, generated harness parity and Gitleaks pass. The workspace reports 1,996 successes, zero failures and 43 ignored tests, including nested process helpers, with two test threads. The Linux application target reports nine top-level successes plus one nested helper. Four C interposer cases cover both cut boundaries, unarmed operation and a different target. The 34 packaging tests pass with the installed Homebrew Python; the first invocation with system Python did not start because `tomllib` was unavailable.

An initial local test attempt failed three new assertions: they called the clean-source validator after deliberately changing source bytes. The assertion now checks the actual Git HEAD. The corrected target and full workspace pass; no product behavior was changed for that failure.

## Evidence limits

This qualification covers process death at two instrumented boundaries. It does not cover interruption midway through a write, a machine crash or arbitrary concurrent writers. In-place writes remain non-atomic. Missing `integration.applied` remains application UNKNOWN even when candidate bytes were observed on disk.

Native reopening observes persisted state without replay. Separate process tests reconstruct a test-only handle and require explicit rejection of consumed approval, including the before-write case with the original source still present. Those tests do not run a native worker and do not establish a production resume API.

Human acceptance, publication and global host integration remain outside this lot. The fixed oracle is not rerun on the applied directory. CI is not used as acceptance evidence. Issues #132 and #11 remain open.

## Retained evidence

- [Source and driver provenance](source.json), [signed source pins](source-pins.json), [driver pins](driver-pins.json).
- [Local checks](local-checks.json), [validation history](validation-history.json), [Linux build](linux-build.json).
- [Independent reviews](independent-reviews.json).
- [Native outcomes and confinement](native-results.json), [public evidence projection](native-evidence.json), [133-file collection manifest](retained-manifest.json).

The public projection reserializes selected authored records and preserves their original byte hashes. Full raw logs, receipt/report bytes and collected files remain in the retained local evidence lot. No private host configuration or credential is part of this report.
