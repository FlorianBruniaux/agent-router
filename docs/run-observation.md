# Observe an existing run without executing work

F04a, delivered by [PR #139](https://github.com/FlorianBruniaux/agent-router/pull/139), provides `agent-router observe`: one coherent, bounded and redacted projection of an existing runtime store. It does not start or recover a run. F04b remains open; the compact human view is only an initial F05a view, not a complete native timeline. The contract is recorded in [ADR 0025](adr/0025-run-observation.md).

## Read an existing store

Use a store and run identifier you already obtained from an authorised run. In these format examples, replace `ROOT` with that existing private store directory and `ID` with its existing run identifier:

```sh
agent-router observe --store ROOT --run-id ID
agent-router observe --store ROOT --run-id ID --json
```

An absent directory, database or run is refused. The command does not create a store, migrate it, repair history, acquire supervision ownership, cancel work or release permits. SQLite may create or update WAL/SHM coordination files while reading; passive observation does not guarantee zero filesystem activity. Private artifacts and lease files are not opened.

## Interpret the two views

The human view prints the snapshot digest, hashed run key, recorded state/attempt count, journal horizon, recorded cleanup and explicit unknown fields. It shows the last **20 selected-run event references**, each with its sequence and recorded state, and counts omitted references. `--json` contains the complete bounded selected-run reference list, including event hashes. Both formats derive from the same projection; separately invoked commands may observe different committed snapshots.

`schema_version` is 1. The JSON contains:

| Field | Meaning |
| --- | --- |
| `snapshot_sha256` | Digest of the entire redacted projection, with its own digest field emptied during hashing |
| `identity` | Hashed run/definition/receipt/plan/node identities; no raw identifier or path |
| `horizon` | Last sequence/hash of the verified global journal, including other runs |
| `known` | Persisted state, durable attempts, finite failure/cleanup interpretation and selected event references |
| `unknown` | Requested/observed model and effort, transport, validation, objective, duration, authenticity and current process liveness remain `unknown` |
| `completeness` | Journal coverage `complete` only through the retained checked horizon, external delivery `unknown`, private detail `redacted` |

A later committed append changes the horizon and snapshot digest even if it belongs to another run. Repeating an unchanged durable snapshot produces identical JSON bytes. The run key follows the current approval binding and can change with a retry; it is not a universal task/session/send identity.

The event references have no trusted timestamps or external notification semantics. A recorded terminal state or cancellation request does not prove process shutdown, useful output, human acceptance, a physical-send budget or complete provider delivery. Chain integrity does not establish authenticity against a same-user writer. Hashes support local correlation, not anonymisation or export consent.

## Refusal and bounded work

The global journal is capped at **100,000 rows**, **64 MiB** of text and **65,536 bytes per row**; a selected run is capped at **4,096 event references** and its identifier at **1,024 bytes**. Summary and cleanup dependency tables have additional row/byte ceilings in [ADR 0025](adr/0025-run-observation.md#bounds-and-refusal). SQLite preflight checks counts and byte lengths before decoding private JSON.

An unsupported schema, broken chain, sequence gap, substituted binding, summary/history mismatch, malformed private JSON or exceeded ceiling refuses the observation. Corruption or limits in another run can also refuse it because the global chain is checked. There is no truncated-success result: a refusal has no snapshot identity and must not be interpreted as an empty or complete report. Older readable schemas preserve unknown cleanup without migration.

The byte/row checks bound this reader’s Rust allocation and decoding work. They do not establish an OS sandbox or protection against every malicious database layout. The existing `status` and `events` commands keep their contracts.

## Delivery boundary

F04a is implemented, tested and independently reviewed locally. Twelve runtime and three CLI focused tests passed, and 19 independent probes found no confirmed blocker. The [source-bound verification record](bench/verification-observation-2026-10-07.json) records all 21 selected quality checks passed: 1,633 successful workspace Rust results plus one core check, and 290 Python tests. Rust counts sum result lines, including subprocess invocations, rather than unique test definitions; the focused tests are included in the workspace count. The 42 ignored opt-in native tests are not successes. This selection is not full CI equivalence and does not qualify a live provider, useful patch, release or measured gain.

The next F04b slice publishes the unchanged v1 output shape as `agent-router schema run-observation`, also listed by `schema --list` and `version --json`. It rejects extra fields, unsupported known claims, malformed digests and values outside the documented bounds. Schema validation does not recompute hashes, verify event ordering or establish the authenticity of a supplied snapshot. F04b still owns qualified native fields and malformed/lost/truncated notification coverage. F05a’s complete human/JSON timeline and F05b/#117 trustworthy timing remain open. See [CLI output](cli-output.md), [JSON contracts](reference/json-contracts.md), the [bench playbook](bench/playbook.md) and the [current finalization plan](bench/finalization-plan-2026-10-07.md).
