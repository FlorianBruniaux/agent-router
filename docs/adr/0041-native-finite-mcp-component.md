---
adr: "0041"
title: "Qualify the native finite MCP component"
status: accepted
created: 2026-10-17
last_modified: 2026-10-17
scope: "Local native MCP component qualification; Runtime Tools admission remains disabled"
amended_by: []
supersedes: []
superseded_by: []
related: ["0038", "0039", "0040"]
---

# ADR 0041: Qualify the native finite MCP component

## Context

The finite file policy, stdio codec and nested process supervisor were tested with authored callers. They did not establish that the pinned Codex 0.159.2 Linux ARM64 client could initialize MCP, invoke its tools and consume the real results. Issues #132 and #133 still require composed approval, live evidence, cleanup and physical-send accounting.

## Decision

A fixed owner entry runs inside the existing helper-supervised PID namespace. It owns one ToolSession across connections, starts the native child under the native MCP guard, stops on policy/protocol failure and writes a bounded terminal observation to a precreated leaf. That observation contains hashes, file identities and redacted call telemetry. Worker bootstrap and terminal JSON are not approval, capability or cleanup authority.

Only the native MCP mode admits anonymous Unix SEQPACKET socket pairs. A syscall trace showed the pinned musl client needs this spawn-error channel. Addressed SEQPACKET sockets remain refused in every mode; legacy and candidate execution still refuse SEQPACKET pairs. File, network and memory rules are unchanged. The helper retains the exact 8 GiB address-space bound, and the qualification container retains 8 GiB aggregate memory with no swap.

The generated fresh native configuration disables optional native tools and explicitly approves only read_file, write_file and exec inside that temporary configuration. This does not grant product authority. Both the Runtime review entrance and legacy Linux composition refuse this MCP startup state.

Codex advertises three additional discovery wrappers whenever an MCP server exists. The authored broker validator pins the entire raw catalogue: three wrappers and one namespace containing the three finite tools. It projects a copy for the original frozen script, preserving raw requests. It recognizes the observed two-part successful result envelope, including the native timing prefix; errors, changed schemas, extra tools and falsified results are refused. This is fixture validation, not a model-facing catalogue rewrite or a broader Runtime allow-list.

## Evidence boundary

The local replay uses six public packets and authored Responses, no model or account. It checks actual file effects, scratch compilation/execution, RED/write/GREEN feedback and results returned through the real native client. The private startup state is armed before capture and disposed only under the matching stopped-namespace proof. An independent oracle checks the frozen outputs separately.

Preparation retains output and input identities. Terminal-output validation rejects a replaced inode even with identical bytes, changed public inputs and a falsified hash or relative path. The caller must still authenticate its observation channel and hold the exact cleanup proof.

## Deferred boundary

This component does not enable Runtime Tools execution. The proposed next profile must prepare an opaque public staging owner, retain the terminal descriptor, qualify fresh effects in the retained composition and transfer at most the approved output into the private artifact view after writer exclusion. It must preserve scratch verification through artifact capture. The private Git view must not be mounted writable for the worker. Automatic approval rejected changing that namespace/admission boundary without explicit approval for the concrete behavior; the proposal remains inert.

A real-provider bridge, authentic model metadata, cumulative usage and spending limits remain separate. The authored catalogue's direct mode, context window and zero/default usage are not vendor capability or billing evidence. No sealed benchmark cell or savings claim follows.
