# M1: campaign model preparation

This campaign catalog provides five declared models and 29 eligible model/effort
pairs from the active Codex collaboration surface observed on 2026-10-10.
It is separate from the distributed catalog. A local CLI route can select among
at least two eligible models. Actual model invocation and provider identity remain
UNKNOWN until M4.

## Artifacts and evidence boundary

- `catalog.json`: existing unsigned catalog schema 1, with all three route stages.
  Quality, cost and latency values are arbitrary uncalibrated policy priors.
  `cost_micros` is a policy score, never money or a measured price.
- `host-snapshot.json`: existing `router_types::HostSnapshot` shape, projected
  from the active `collaboration.spawn_agent` definition. The observation includes
  `gpt-6.1-sol`, `gpt-6-astra`, `gpt-6-sol`, `gpt-6-luna` and `gpt-5.6-sol`.
  Each supports low/medium/high/xhigh/max/ultra in that metadata; Luna stops at max.
- `provenance.json`: date, source, canonical host digest and explicit UNKNOWN
  provider inference, provider identity, native runtime and capability evidence.
  The projection reuses the serialized host contract. It is not a native CLI
  discovery probe, authenticated `HostObservation` envelope or runtime proof.
- `request-template.json`: valid route request embedding that exact host. It has
  no required capabilities and low risk for the offline demonstration. M3/M4 must
  supply their sealed task requirements separately. Empty capability fields grant
  no tools, workspace writes, network or execution permissions.
- `campaign.py`: bounded stdlib preparation and supplied-identity comparison.
  It performs no invocation, uses no credentials and modifies no global config.

JSON inputs are capped at 256 KiB, model pools at 64 entries, duplicate keys and
unexpected fields rejected. Provenance binds the host using SHA-256 of UTF-8 JSON
with sorted keys, compact separators and no trailing newline. Those canonical
hashes differ from raw file hashes retained in `verification.json`. A live
preparation refuses future or older-than-24-hour metadata; the clock override is
only for replay. Refresh the source observation on a new host/session before M4.
Hash consistency proves neither source authentication nor permission.

## Reproduce preparation

From the repository root:

```sh
python3 docs/bench/measurement-m123/m1/campaign.py prepare
python3 docs/bench/measurement-m123/m1/campaign.py prepare --now 1791662585
python3 -m unittest discover -s docs/bench/measurement-m123/m1 -p 'test_*.py' -v
```

The first command checks current freshness. The second replays the captured
observation and reproduces `prepare-example.json`. `READY_FOR_PREPARATION`
requires at least two distinct eligible models, with every candidate in the
catalog/host effort intersection. Exclusions retain `model_absent` or
`effort_incompatible`. It carries `execution_authorized: false`.

To include actual CLI behavior, set `AGENT_ROUTER_BIN` to an existing reviewed
Agent Router executable before running the suite. Without that environment
variable, the actual CLI test explicitly skips; the other 11 tests still run.
The test does not build Cargo or invoke a model.

```sh
"$AGENT_ROUTER_BIN" route \
  --catalog docs/bench/measurement-m123/m1/catalog.json \
  --request docs/bench/measurement-m123/m1/request-template.json --json
```

`route-example.json` records the observed decision. Two identical CLI calls were
byte-identical, and the test checks all selected and alternative stages against
the intersection. Temporary requests also exercise real CLI exclusions for an
absent model and an incompatible effort. The current demonstration chooses Luna
for all selected stages and retains Sol alternatives. No rule forces diversity;
this route is not evidence of quality, useful task adaptation or savings.

## Identity comparison for M3/M4

```sh
python3 docs/bench/measurement-m123/m1/campaign.py identity \
  --record docs/bench/measurement-m123/m1/identity-example.json
```

A record has exactly `requested`, `observed`, `declaration`. Each non-null identity
has exactly `model` and `effort`; `observed` or either observed component may be
null. A declaration, when present, has exactly `requested`, `observed`, `reason`
and must bind both supplied identities. This comparison consumes supplied data,
not a verified provider observation:

| Status | Meaning | Exit |
| --- | --- | --- |
| `MATCHED_SUPPLIED` | Both supplied components agree | 0 |
| `UNKNOWN` | Observed identity is missing or partial | 3 |
| `SUBSTITUTION_UNDECLARED` | Model or effort differs without a bound declaration | 3 |
| `SUBSTITUTION_DECLARED` | A bound declaration records the mismatch | 3 |
| `INVALID` | Malformed input, stale preparation or broken binding | 2 |

The example returns UNKNOWN and exit 3. All identity reports retain
`provider_identity_verified: false`. A declared substitution remains a mismatch;
it does not pass the campaign's exact-identity gate or authorize replay. M4 must
connect authenticated/qualified observation evidence and approved invocation
before making any runtime claim.

`verification.json` records the actual binary digest, 12 passing tests with no
skip, raw input/output hashes and evidence limits. The version command exposes
no source revision, so the build revision stays UNKNOWN here and must be linked
to the parent M0 build evidence.
