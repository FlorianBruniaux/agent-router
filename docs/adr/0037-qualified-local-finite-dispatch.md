---
adr: "0037"
title: "Execute approved finite clients through a qualified local Linux backend"
status: implemented
created: 2026-10-16
last_modified: 2026-10-16
scope: "Local coded transport and one-file private patch artifacts; no real provider or publication"
amended_by: []
supersedes: []
superseded_by: []
related: ["0007", "0008", "0028", "0032", "0033", "0035", "0036"]
---

# ADR 0037: Execute approved finite clients through a qualified local Linux backend

## Decision

`Runtime::execute_finite_client` consumes the opaque combined approval from ADR 0036. It requires durable admission and the exact retained namespace executable. It reuses the existing preparation, attempt, timing, output retention, patch capture and cleanup journal. Generic execution and generic reopening cannot accept this definition. Neither JSON receipts nor an authenticated historical capability record construct the backend.

The supported composition is Linux ARM64, the pinned Codex 0.159.2 musl client, the fixed finite client and C guard, and the canonical `linux-isolated-broker-v2` profile. Resource arguments remain the exact defaults, including a hard 8589934592-byte per-process address-space limit and zero core dumps. Qualification also uses an outer 8 GiB cgroup with zero swap; that outer aggregate limit is separate from the product's address-space limit. Other platforms and unsupported compositions refuse.

The local backend additionally requires a retained network namespace with no active interface other than the actual loopback, no IPv4 routes, all capability sets zero and NoNewPrivs set. Inactive kernel fallback interfaces are checked per device and cannot be activated under those restrictions. The broker exclusively reserves its port with an IPv4 wildcard listener and both reuse options disabled. Its instance digest binds the namespace identity. Recheck namespace, interface state and reservation through final dispatch; the guard independently checks actual interface flags before native exec. Ordinary loopback listeners and routed hosts refuse. Landlock restricts ports, not addresses: the isolated namespace plus exclusive reservation closes that gap, not the guard alone.

Inside durable preparation, the exact retained composition must demonstrate allowed broker TCP access, denied other ports and socket families, protected files, a small allocation and refused 9 GiB reservation, the native version, and a growing detached descendant that stops when the PID namespace ends. The actual bundle remains read-only during this probe. Every writable probe path uses a sacrificial owned backing. Observe effects and rearm the retained bundle under its exact opaque namespace cleanup lock before settling preparation. Failure preserves cleanup obligations and admission. Only observed effects mint fresh, short-lived OS proofs; they are checked again immediately before dispatch.

The generated home remains read-only except its existing private tmp grant. Its fresh 0644 installation slot already belongs under that tmp: the native resolver opens even a valid ID read/write. The parent alias, configuration, operator catalogue and copied programs remain protected. Startup pins bind the catalogue's bytes and retained file identity in `finite-codex-startup-v2`. Its metadata describes an authored local transport in direct mode. It is not vendor discovery, a model context measurement or proof that the real Luna supports this mode. No personal configuration or credentials are imported.

The finite producer always configures `local_fixture`. Admission derives this identity only from the opaque finite binding, never a caller receipt. Generic v2 admission still refuses an unknown provider, and a finite binding cannot hide a v3 remote provider. Existing v3 asset gates remain mandatory. This entrance admits only the capabilities it qualifies, with no tools, agents or skills selected.

## Private bounded writes

`Runtime::prepare_finite_workspace` creates a detached private Git view before context and final approval. The supported write scope is one pre-existing regular file with a flat name, one link and a maximum 32768-byte preimage. The view has separate file inodes from the original source. Its task, runtime owner, original path/head, actual cwd, scope, preimage and retained directory/file identities enter the combined review. Both Git snapshots must still be clean at the reviewed head after qualification and before dispatch. A frozen review is never silently relocated.

The native client stays in read-only mode. The fixed worker accepts one completed native agent message containing only a bounded replacement string. No response supplies a path. It rejects errors, tools, incomplete or ambiguous streams, duplicate keys and extra fields before modifying the reviewed target. It opens that existing inode relative to a retained directory descriptor, checks identity and preimage, writes in place and synchronizes it. An I/O failure may leave a partial private replacement and remains a failed attempt. The original repository is not written. Normal finalization captures the existing scoped patch artifact.

This slice grants private output and artifact authority only. Finite writes reject `Mutation::Integrate`; opaque handles cannot become generic publication handles. The private view stays available for patch review. Publication against the original repository, general file creation and managed disposal of executed views require a separate contract. In particular, this does not implement the absent `review.json` output required by the sealed r2 review task.

## Evidence and remaining boundaries

The [qualification folder](../bench/linux-finite-dispatch-2026-10-16/README.md) separates source checks, actual Linux runs, native decoding, external fixture acceptance and human acceptance. Host-reported scripted usage is telemetry; attempt accounting stays UNKNOWN. A successful transport remains `Verifying` with objective and validation UNKNOWN.

A parent EOF can stop the namespace while losing the in-process private cleanup owner. The helper's process-stop marker then does not settle filesystem cleanup or release admission. Recovery must retain UNKNOWN and the private state. No crash-recovery owner is reconstructed from pins JSON.

F12a and the 24-cell benchmark remain NO_GO. This does not connect the CLI benchmark dispatcher, qualified tools and H1 feedback, the real CodeModeOnly metadata/runtime, a real provider/account, cumulative native trial limits or publication. No model execution or savings is demonstrated by coded responses. The next execution work must qualify tools and feedback under the same restrictions before any separately approved real trial.
