# Agent Router used to develop Agent Router

Status: B1b implemented, reviewed by the coordinator and validated by the selected local gates. User acceptance and native execution qualification remain separate.

This usability trial implements the [attempt-correlation CLI](../../attempt-correlation.md) for [issue #120](https://github.com/FlorianBruniaux/agent-router/issues/120). The task and inputs were fixed before delegating implementation.

## Reproduce the advisory decision

From the repository root:

```sh
agent-router route \
  --request docs/bench/self-dogfood-b1b-2026-10-10/request.json \
  --catalog docs/bench/self-dogfood-b1b-2026-10-10/catalog.json --json > decision.json
agent-router explain --decision decision.json
```

The retained [request](request.json), [catalog](catalog.json), [decision](decision.json), [host provenance](host-provenance.json) and [sealed manifest](manifest.json) identify the routing inputs and result. The full task contract is retained privately under the manifest's hash; it is not an additional public artifact. The manifest's SHA-256 is `f822d8b5db07feb09de1a02a84addecf0633330b33a07041a58a4fe500e6d612`.

## Execution boundary

The execute-stage recommendation was `gpt-5.6-sol/high`, applied as the requested configuration of one Rust subagent through Codex's collaboration tool. Planning and review stayed with the coordinator. This does not execute the entire three-phase plan, prove provider inference identity or qualify Agent Router's native runtime.

Only `gpt-5.6-sol` is both present in the unchanged bundled catalog and exposed by the active delegation tool. The catalog's other two models are unavailable through that tool; its newer models have no catalog entries. This trial therefore tests a usable routing-to-delegation workflow with one eligible model, not meaningful multi-model optimization. Catalog quality/cost/latency weights are policy priors, not measured prices.

## Measurement rules

Record implementation corrections, local gate outcomes and elapsed command times, including failures. Host-reported token metadata is distinct from billed usage. Cached input is part of input; reasoning output is part of output. Do not add parent and child counters unless their scopes are known to be disjoint. Invoice cost and savings remain unknown. There is no control arm, repeated trial or claim of economic benefit. User acceptance remains separate from local checks and coordinator review.

The command's synthetic fixtures and temporary runtime-store tests are not telemetry from this Codex session. B1b correlates supplied facts; it does not automatically collect this trial. Native broker/network and macOS resource admission remain separate work.

## Observed result

The routed subagent implemented the CLI slice in three Rust files. The coordinator reviewed the change and recorded two implementation defects: absent required nullable fields were accepted, and string-only enums accepted object representations. Both were reproduced, corrected and covered by regression tests. A separate duplicate-key test was strengthened because its original input was also malformed.

The [measurement record](measurements.json) contains source hashes, gate results, initial failing probes, the corrected probes and timestamped host metadata. It contains no raw conversation or provider body.

| Measurement | Observed result | Scope |
| --- | --- | --- |
| Initial route | 1.447 s | One local invocation; no provider call |
| Preparatory build | 61.901 s | Existing target cache, before routing |
| Worker implementation span | 1,152 s | Worker UTC markers; coordinator work overlaps this interval |
| Focused tests | 5 passed, 0 failed | Final command wall time 9.72 s; test time 1.72 s |
| Independent probes | 17/17 passed after correction | Earlier failures retained in the record |
| Workspace tests | 1735 passed, 0 failed, 42 ignored | 587.358 s, macOS, all features, sockets outside sandbox |
| Other local gates | fmt, Clippy, xtask, Gitleaks passed | Exact commands and elapsed times in the record |
| Routing replay | Byte-identical decision | Final binary, 0.0126 s; not a general latency benchmark |

Workspace counts sum result lines with zero filtered tests. They exclude filtered helper-subprocess summaries and are not a count of unique definitions. Ignored opt-in native tests do not establish native readiness. These selected local gates are not cross-platform CI equivalence.

### Host-reported tokens

| Scope | Input | Cached input, included in input | Output | Reasoning output, included in output |
| --- | ---: | ---: | ---: | ---: |
| Implementation subagent | 7,270,171 | 7,100,800 | 39,000 | 11,209 |
| Coordinator through recorded cutoff | 9,674,786 | 9,500,416 | 50,252 | 25,444 |

These are session counters across repeated model requests, not unique prompt sizes. The implementation session metadata confirms the requested `gpt-5.6-sol/high` configuration. The coordinator metadata records `gpt-6-astra/xhigh`; Agent Router did not select or change that configuration. The counters are kept separate because overlap between their scopes was not independently established. No combined total, invoice cost or savings ratio is inferred. Publication and later interactions fall outside the recorded coordinator cutoff.

## Use the delivered slice and continue the trial

Run the examples in the [CLI guide](../../attempt-correlation.md). The direct fixture and the CLI-produced synthetic-store export both succeed; absent physical evidence remains unknown. The existing human view also succeeds when `--json` is omitted.

Before a meaningful model comparison, reconcile the candidate catalog with the models actually exposed by the execution surface, keep policy priors distinct from measured data, and freeze a paired-task protocol with quality gates. A smaller fresh coordinator context should be tried as a separate configuration; this run does not measure its benefit. Native admission and automatic usage collection remain separate work.
