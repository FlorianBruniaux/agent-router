# Native owner death after an authored partial write, 2026-10-24

This follow-up induces a real short write in the fixed public finite application, then kills the owner before the rest of the candidate can be written. It extends the two-boundary qualification in #208 without changing product binaries or expanding private access.

## Exact cut

The original `lib.rs` has 348 bytes; the candidate has 454 bytes and first differs at offset 112. The interposer calls the original `write` with half the candidate buffer, requires an exact 227-byte return, then sends SIGSTOP before returning to `write_all`, synchronizing or truncating. The parent requires the complete mixed image `candidate[:227] + original[227:]`, length 348, and verifies it differs from both complete versions before SIGKILL and reaping.

The `kill_partial` case uses a new disposable Linux cell. It maps to the existing allowed public root `/task/entries/kill_before`, so the qualified Rust reopening observer stays byte-identical. The four product programs also stay unchanged. Only the fixture interposer is rebuilt.

Two actual `Runtime::open_with_admission` calls must preserve the original synthetic admission binding, complete journal, source bytes and modes, HEAD and logical index. The second opening follows an authored later edit at mode 0400. The observer reconstructs no application authority and does not attempt replay; the separate explicit replay-refusal process tests from #208 remain distinct evidence.

## Verification

The five local gates pass: format, warnings-denied all-feature Clippy, all-feature workspace tests, generated harness parity and Gitleaks. Five C interposer cases pass, including the new actual partial write and the prior before/after/unarmed/wrong-target controls.

The single native case passes. It observes the exact 348-byte mixed file, SIGSTOP 19, then SIGKILL/reaped 9. The accepted-candidate attestations and consumed intent survive, with no applied event. Both reopenings preserve the full journal and source projection; the later authored edit survives at mode 0400. Six scripted worker requests include a distinct permission refusal, with zero model calls.

The workspace reports 1,996 successes, zero failures and 43 ignored, including nested helpers. The native cell reports memory.max 8,589,934,592, swap.max 0 and peak 832,593,920 bytes. Both cells are stopped without OOM. Independent source, driver, compiled-binary and post-run reviews pass. The reviewer rehashed all 46 retained files and nine input programs, recalculated the mixed image and confirmed the complete reopening history without rerunning the pilot.

## Limits

This is a deliberately induced short write followed by process death. It does not interrupt an arbitrary kernel syscall or qualify power-loss durability. The partial file is observed before synchronization; an unchanged database history does not make in-place application atomic.

Candidate acceptance stays separate from application. An intent without an applied event leaves application UNKNOWN. This lot provides no production command to continue an interrupted application and no human acceptance, publication or global host qualification. #132 and #11 stay open; #208 remains an unmerged dependency.

## Evidence

- [Independent reviews](independent-reviews.json).
- [Source and driver identities](source.json), [Linux build and reused programs](linux-build.json), [local gates](local-checks.json).
- [Native outcome and limits](native-results.json), [public records and mixed bytes](native-evidence.json), [46-file collection manifest](retained-manifest.json).

Source commit `f355551cf7a5fc76415f4c21d1368ef86190f539` changes three fixture files and their README. The compiled tree differs only by that README. The preserved previous lots retain their original files and identities. Public records are reserialized with original byte hashes; the full raw collection remains retained locally.
