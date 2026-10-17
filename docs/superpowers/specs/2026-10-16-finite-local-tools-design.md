# Finite local tools and harness feedback

Implementation status: preparation and inert protocol/policy only. The current runtime continues to refuse tools and new-output execution. Automatic approval review refused the proposed C guard changes before any modification. The security profile, socket permissions, memory limits and private mounts are unchanged. The launcher design below is not implemented or qualified by this slice.

This extends the approved local-only F12a work at 33dc663. The human request is to connect local tools and H1 feedback to the benchmark with no model execution or wider private access. These implementation choices are the builder's, not new benchmark budgets. PRs 180/181 remain dependencies, not merge authority.

## Contract

A new opaque preparation binds the public packet, exact output grant, actual private view, owned programs, local tool limits and startup before final approval. `ReplaceExisting` requires the original preimage. `CreateNew` requires a missing flat output and never precreates it. Existing preparation continues to refuse missing targets. No source publication or arbitrary filesystem path is authorized.

The native client exposes exactly one local MCP server, `finite`, with `read_file({path})`, `write_file({path,content,create_new?})` and `exec({argv,timeout_ms?})`. `create_new` defaults to false. Creation requires both explicit true and an approved absent target; a second creation refuses. Replacement requires an existing inode owned by this preparation and its current checked preimage. Scratch uses the same explicit creation distinction. Names are flat public files or one flat component below `.scratch`; traversal, symlinks and multiple links refuse. All request shapes and work are bounded.

H0 and H1 have identical tool capabilities and limits. Only H1 supplies immutable `HARNESS.md` and `feedback.py`. The exact feedback command is `python3 -B feedback.py`. A nonzero command result is returned normally so the native client can read a red diagnostic, write a correction, call feedback again and receive its actual green result. Truncation, timeout, cancellation or budget exhaustion stops further effects; a partial result cannot become successful feedback.

## Process and authority boundary

The existing exact PID namespace remains the lifetime owner. A trusted fixed launcher runs the narrow tool controller before installing native restrictions, then launches the guarded native worker. The native worker reaches only the typed controller through a private local socket and the MCP codec. Tools cannot supply another controller, program root, environment, home or network grant.

Each exec uses the existing framed helper and an additional rootless namespace. It sees only the public candidate files read-only, the permitted system toolchain read-only, and one per-attempt scratch mount. It has no access to the native home, configuration, copied client/helper/guard, controller, oracle, original repository or network. Descendants inherit these restrictions. Tool cleanup requires the exact nested namespace proof, not group termination. All tool processes also remain descendants of the outer namespace, including detached sessions. Stopping the outer owner must stop them all.

Only the trusted tool controller performs approved public output writes. Tools can write temporary material only in scratch. Scratch persists across tool calls, is private to an attempt and disappears with its namespace. Its byte limit is enforced by tmpfs; entry/depth limits are checked before and after an isolated command, with a bounded traversal and a terminal refusal on excess. This observed entry limit is not a claim of an in-flight kernel inode quota. The unchanged outer 8 GiB memory bound also bounds metadata consumption. No automatic increase follows from a refused compile.

The initial local qualification configuration proposes 8 calls, 8192 raw output bytes per call, 32768 cumulative output bytes, 20000 ms per exec, 60000 ms cumulative exec time, 90000 ms session time, 32768 bytes per file, 16777216 scratch bytes and 64 observed scratch entries and depth 4. These are explicit local configuration values to bind and test, not the sealed benchmark's unapproved common token/time limits. The 24 sealed rows remain `not_run` and execution false.

A retained, bounded tool journal lives outside candidate mounts and is writable only by the trusted controller. Runtime reads it under the exact stopped namespace lease before settling private cleanup, validates output identities/hashes and records typed observations in its existing journal. Invalid or incomplete cleanup evidence retains UNKNOWN and admission. Native events and JSON reports never mint authority.

## Acceptance

- Actual native local Responses loop consumes red feedback, performs the scoped write and consumes a second green feedback result. No scripted result substitutes for the actual command result.
- H0 runs its own check through the same tools and caps. Scratch survives between write/compile/execute calls.
- t05 starts absent, creates only its approved output, refuses duplicate creation and captures the new-file patch. Existing-file and immutable-input changes remain distinct.
- Actual tool attempts cannot read private paths, mutate immutable input/source, reach the broker/network, change limits or escape through detached descendants.
- Test malformed/duplicate inputs, path substitutions, tool/call/output/time/scratch exhaustion and incomplete results, plus normal/timeout/cancel/parent EOF cleanup.
- Run the complete local quality gate and actual Linux qualification with the 8589934592-byte limit and zero swap. Keep independent fixture acceptance, OS effects, native decoding, accounting and human acceptance separate.

## Ownership

The runtime worktree owns runtime contracts, producer/worker/controller, guard, lifecycle integration and its tests/docs. The coordinating chat owns sealed r2 exports, authored Responses fixtures and independent oracles under its existing benchmark folder. It also supplies an inert pure MCP codec candidate in its task tmp directory; only the runtime owner integrates it. Neither chat overwrites the other's source or imports the dirty primary checkout as a production overlay.
