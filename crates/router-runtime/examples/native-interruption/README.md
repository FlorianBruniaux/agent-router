# Native owner interruption fixture

This fixture qualifies a controller process killed at controlled application boundaries. It does not add a product resume command or an application capability. The `finite_reopen` example opens the same isolated fixture Runtime with its original synthetic admission configuration, then reports integrity, summary, verification, cleanup and events.

`cut.c` is a Linux `LD_PRELOAD` interposer loaded only into the disposable owner. It has no constructor. Once the parent arms it after candidate review, it checks both the exact executable and target FD path. It stops before the target write, or after a successful target `fsync`. The parent observes SIGSTOP, reads the durable intent and source, sends SIGKILL and waits for that PID. A resumed fixture exits without further writes.

`test_cut.py` checks both stop boundaries and the unarmed/wrong-target controls using a disposable executable. `qualify.py` composes the frozen public parent and inputs from the preceding application qualification. Its assembled directory requires `parent.py`, `controller.py`, `assets.json`, `identity.json`, `input-pins.json` and `inputs/`. The source and evidence manifest records their identities; this script does not discover providers, credentials or arbitrary workspaces.

The native cases preserve the original four qualified product binaries. Only the interposer and the separate Rust observer are newly compiled. The completed control runs without the interposer. Each case reopens twice; the second opening follows an authored simulation of a later user edit, including a file mode change. Full journal and source projections must remain unchanged across each opening.

The native observer never reconstructs an application handle or attempts replay. Separate Rust process tests under `finite_client/application/tests/process.rs` attempt the consumed approval with a test-only handle and require refusal, including when the source still contains the original bytes.

This is process-death evidence for the fixed public fixture. It does not cover a machine crash, arbitrary interruption inside a kernel write, arbitrary concurrent writers, human acceptance or a production API for continuing an interrupted application.

## Authored partial-write variant

The `kill_partial` case induces one real short write: the interposer calls the original `write` with half the candidate buffer and requires that exact return count, then stops before returning to `write_all`, synchronization or truncation. For the fixed public packet this writes 227 of 454 candidate bytes into the original 348-byte file. The parent requires the exact mixed image, distinct from both complete images, and retains its bytes before killing and reaping the owner.

A fresh disposable cell maps this case to `/task/entries/kill_before`, an existing allowed public fixture root, so the qualified Runtime observer can remain unchanged. The case and its evidence remain named `kill_partial`. Both openings use the original admission configuration and preserve the uncertain application history; the second also preserves an authored later edit. This proves a deliberately induced short write followed by process death, not arbitrary interruption inside a kernel syscall or power-loss durability.
