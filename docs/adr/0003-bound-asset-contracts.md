---
adr: "0003"
title: Bind asset constraints separately from legacy wire documents
status: implemented
created: 2026-09-27
last_modified: 2026-09-27
scope: "`router-asset-contracts`, consumed by the phase gate (`router-policy` `asset_phases`), preparation and the runtime final gate on the exact prepared input."
amended_by: []
supersedes: []
superseded_by: []
related: []
---

# ADR 0003: Bind asset constraints separately from legacy wire documents

Legacy model and skill records omit provider/auth/context/modalities and the
complete skill activation manifest. Adding optional fields with permissive
defaults would allow older execution paths to ignore newly declared constraints
while apparently accepting the same catalog.

Add strict, versioned asset-contract and availability documents, bound to the
canonical catalog and host snapshot respectively. Keep legacy wire bytes and
hash domains unchanged. A new phase receipt version must bind these documents,
gate policy, candidate requirements and opaque skill approvals. Reconstructed
execution must repeat the gate on the exact adapter-prepared input bytes.

A document's self-computed hash is integrity evidence. Independent expected
hashes and parent-controlled scope authorization establish the source boundary;
the library does not accept a `trusted` boolean, child `approved` field or
deserialized approval token. Review approvals cover the manifest, selected body,
scanner findings, policy and scope. Unknown authentication, features, context or
accounting fails closed, and an executable skill has no instruction fallback.

This preserves historical dry-run compatibility while making new enforcement
explicit. It also requires callers to carry independent authority and actual
prepared bytes. Static scanner findings remain heuristic, license allowlists
remain operator decisions, and synthetic fixtures cannot establish native host
behavior. A positive gate does not replace phase, runtime or sandbox approval.
