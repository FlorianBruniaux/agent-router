# Linux finite tool guard canaries

The operator-approved patch is applied byte-for-byte. Ten actual Linux guard tests pass with an unchanged **8589934592-byte** outer memory limit, zero swap and the existing per-process address-space limit. The Rust feedback script compiles and runs one authored failing test, then succeeds after a controller-authored correction. No model, provider, sealed benchmark trial, runtime Tools proof or source publication is involved.

This qualifies the guard under this test composition. The native MCP bridge, trusted tool controller, owned startup configuration, terminal runtime journal and approved end-to-end tool dispatch remain unimplemented or unqualified. Runtime still refuses Tools and new-file execution. See [ADR 0039](../../adr/0039-finite-tool-guard-canaries.md).

## What is observed

- The legacy guard before and after the patch allows broker-only TCP, denies addressed Unix/UDP/IPv6, protects public input and configuration, and retains one explicitly writable existing file. Its metadata changes and unlink remain denied.
- The new native profile permits a Unix stream connection to the one initially exposed pathname socket. It refuses an explicit workspace-write grant. **The filter does not match an RPC pathname**: it permits Unix stream sockets within its visible/scoped namespace. The composition must expose only intended endpoints. The initial mounted socket inventory is checked; the existing writable private tmp remains a separate grant.
- The tool profile refuses addressed network/Unix socket creation and sees public input read-only. Anonymous Unix stream socket pairs remain intentionally allowed. It creates and changes permissions/times only in the 16 MiB scratch tmpfs. Attempts through public paths, scratch symlinks, read-only descriptors and `/proc/self/fd` cannot change public bytes, mode or timestamps.
- Existing mounted files must produce permission/read-only errors; `ENOENT` cannot satisfy those assertions. The actual synthetic private control exists in the outer harness and is absent from the inner view. This is an absence-of-mount observation, not a Landlock decision on an existing private file.
- Actual PID namespace identities differ from the controller's, capability masks are zero, NoNewPrivs is set and no extra descriptors are inherited. A small allocation succeeds, a 9 GiB reservation and an increase of the hard limit refuse. The outer cgroup separately records 8 GiB and zero swap.
- A detached session writes a scratch heartbeat and an independent output stream before normal exit, timeout or controller-pipe EOF. The output stream reaches EOF within the collection deadline, the exact helper lease becomes namespace-clean and its exclusive lock is available. Native-profile file heartbeats also stop growing. These controlled effects do not mint a Runtime cleanup handle or prove private-state recovery after a real application crash.

The baseline guard refuses the new tool mode before the canary executes. The [BPF fragment review](evidence/bpf-review.json) separately simulates the old and new socket-filter branches; it is not a kernel test or proof of whole-program equivalence.

## Composition and compiler

The test image is locally pinned, not downloaded. The outer container has no network, no host-file mounts, a read-only root, uid 65534, no capabilities, NoNewPrivs, 256 pids and one CPU. Its existing named volume holds public source/build cache and the previously pinned helper. Rootless nested bubblewrap requires unmasked proc paths and an unconfined outer seccomp profile, as in the earlier local qualification. These are harness settings, not product grants. Effective settings and mounts are retained in [container.json](evidence/container.json).

Each tool gets an empty root, read-only `/usr`, read-only public candidate, a fresh proc mount and scratch tmpfs. The guard binary is executable but not readable to the tool. No controller, original repository, personal home or credential directory is bound. Public metadata safety depends on the **read-only mounts as well as the guard**: Landlock does not provide that metadata restriction alone. A future runtime must construct and validate this exact boundary before claiming Tools support.

The image's `cc` alias points outside `/usr` through `/etc/alternatives`; `/etc` stays unmounted. Direct GCC executes, but the initial Rust linker launch returns EPERM. Rust 1.85.1 uses a Unix sequenced-packet pair in its fork/exec fallback when a modified PATH and a bare program name prevent posix_spawn. This source-based diagnosis is an inference, not a syscall trace. See [Rust's launch implementation](https://github.com/rust-lang/rust/blob/1.85.1/library/std/src/sys/pal/unix/process/process_unix.rs#L68) and [the fallback condition](https://github.com/rust-lang/rust/blob/1.85.1/library/std/src/sys/pal/unix/process/process_unix.rs#L422).

The fixture supplies an immutable compiler adapter under read-only `/usr/local/bin`. It invokes the pinned Rust compiler with an absolute GCC linker path already permitted below `/usr`. No additional socket permission, `/etc` mount, writable PATH component or larger resource limit is introduced. The supplied [feedback.py](feedback.py) remains byte-identical to the public H1 script. Its exact entry command is `python3 -B feedback.py`. The before/after Rust candidate is authored here; no sealed task or agent supplies the correction. A future startup review must pin the adapter and toolchain as well as the guard.

## Local reproduction

Requires the pre-existing local image and cache identified in [reproduce.py](reproduce.py), Docker, `rtk` and Python 3.11+. Missing prerequisites stop; nothing is pulled or installed. Run from this repository:

```sh
python3 -B docs/bench/linux-finite-tools-guard-2026-10-16/reproduce.py \
  --output /private/tmp/ar-finite-guard-evidence-new
```

The destination must not exist. The runner transfers exactly five public files, checks the reviewed old/new guard and feedback hashes, records effective Docker settings and removes only its own container. It preserves the shared cache volume and exports results even on test failure. The historical guard comes from the fixed parent commit. The exact [source manifest](evidence/source-manifest.json), [results](evidence/results.json), [program hashes and limits](evidence/programs-and-limits.log) and [canary log](evidence/canaries.log) describe the retained successful run. Disposable container teardown is not product recovery evidence.
