# M2: retained host traces linked to one task

`trace_to_task.py` imports the retained B1b host exports into
[real-report.json](real-report.json). It links the source revision, supplied
file digests, coordinator and implementation work, usage windows, local checks
and user verdict. Python's standard library is the only dependency.

## Reproduce the report

From the repository root, set `M2_PRIVATE_DIR` to the directory containing the
retained private exports. Its value is deliberately absent from this document.
The importer reads only `parent-usage-baseline.json`,
`parent-usage-cutoff.json`, `child-usage-cutoff.json` and
`capture-parent-usage.py` there. It hashes the capture script without executing
it. The public measurements file is the fifth explicit input.

```sh
python3 -B docs/bench/measurement-m123/m2/trace_to_task.py import-b1b \
  --private-dir "$M2_PRIVATE_DIR" \
  --measurements docs/bench/self-dogfood-b1b-2026-10-10/measurements.json \
  > /tmp/m2-real-report.json
cmp /tmp/m2-real-report.json docs/bench/measurement-m123/m2/real-report.json
```

Each source is bounded at 4 MiB. SHA-256 digests identify the input bytes.
The importer uses an allowlist and emits no local source path, original session
identifier, conversation payload or arbitrary source note. Exported revisions
accept a SHA-256 digest or a bounded map of relative Rust source paths and
digests; source commits require a 40-character Git SHA. Window timestamps must
be valid UTC timestamps, or null. Configuration and duration fields reject
nested values and values outside their intended formats. Invalid inputs
return exit 2 with a fixed error. It never opens the raw session tree.

## What the retained evidence covers

| Host counter | Cutoff on 2026-10-10 (UTC) | Reported total tokens |
| --- | --- | ---: |
| Parent supplied cumulative delta | 15:18:45.923 | 9,725,038 |
| Child retained cumulative snapshot | 15:09:31.458 | 6,822,482 |
| Child later public cumulative snapshot | 15:10:22.363 | 7,309,171 |

These are separate observations. The later child snapshot does not add another
7,309,171 tokens to the earlier snapshot. Parent/child overlap remains unknown,
so the combined total is null. The parent delta is copied from the retained
export; the baseline is hashed and recorded, but this adapter cannot recompute
the delta without the missing absolute endpoint. The child's counter origin
is also unknown. None of these counters is independently reconciled to billing.

Cached input is included in input. Reasoning output is included in output.
Uncached input is the subtraction of the two supplied input counters, with no
price attached. Host-reported money, estimated cost and invoiced cost are three
separate null fields. There is no subscription allocation, model-price lookup
or policy-score conversion.

The five local gates are supplied passing results, linked through their log
hashes to the source-file digest set. Implementation elapsed time is 1,152
seconds from the earlier worker markers, including review feedback. Local gate
durations remain separate because they may overlap. Human active time is
unknown. The user acceptance remains unknown, so cost per accepted task is
undefined. This import does not rerun those earlier gates or establish remote CI.

Session labels and the two work-attempt labels are producer-assigned aliases;
each imported attempt records `session_identity_kind: report_local_alias` and
`observed_native_session_id: null`. These aliases are not native runtime IDs
or physical provider requests. The recorded
revision is a supplied set of file digests, not an invented accepted commit.
The report is `supplied_unverified`, and later work is outside its cutoffs.

## Reuse with supplied task evidence

[fixtures/synthetic-window.json](fixtures/synthetic-window.json) documents the
small normalized input shape. All its identifiers and counters are fabricated.

```sh
python3 -B docs/bench/measurement-m123/m2/trace_to_task.py report \
  --input docs/bench/measurement-m123/m2/fixtures/synthetic-window.json
```

An identical snapshot with the same ID counts once; a conflicting one refuses.
Attempt IDs must be unique and test/attempt links must resolve. A genuine retry
has a separate attempt and scope. Summation across scopes requires an explicit
supplied evidence digest asserting disjoint scopes. Multiple windows in one
scope remain non-additive, even with that assertion. The adapter does not infer
non-overlap from filenames, timestamps or parent/child labels. Missing counters
and attempts without usage block the combined total. Calculations cover only
the supplied inventory; input hashes do not authenticate its completeness.

## Existing cost calculator boundary

Host session snapshots cannot supply the physical keys required by
[accepted-changes](../../../accepted-changes.md). The real report therefore
marks that projection unavailable. M2 creates no runtime observation or ledger.

For an already qualified, explicitly supplied accepted-changes document,
`physical-input` checks the declared provenance and physical key presence,
then preserves the document for the existing strict calculator. The calculator
owns full contract validation, identity conflicts, acceptance history and cost
arithmetic. This passthrough does not qualify evidence or authenticate prices.

```sh
python3 -B docs/bench/measurement-m123/m2/trace_to_task.py physical-input \
  --input tests/fixtures/evaluation/accepted-changes-v1.json \
  --provenance synthetic_supplied > /tmp/m2-accepted-input.json
"$AGENT_ROUTER_BIN" eval accepted-changes --input /tmp/m2-accepted-input.json --json
```

That fixture's prices are synthetic. It demonstrates calculator integration,
not financial coverage for B1b. The real report keeps all money unknown.

## Behavior checks

```sh
python3 -B -m unittest discover -s docs/bench/measurement-m123/m2 -v
```

Set `AGENT_ROUTER_BIN` to an existing executable to include the offline
calculator integration test. Without it, that test reports a skip; no build or
provider call occurs. With the binary, 20 tests passed during this implementation.
They cover repeated snapshots, genuine retries, conflicting identities,
parent/child overlap, missing usage, cache subsets, undefined acceptance ratios,
incomplete pricing, import privacy and bounded input. The initial test-first
run refused seven missing behaviors and had eight missing-output failures
before the adapter was implemented; later tests added boundary coverage.
Review reproduced nested payload disclosure, then 13 malformed-metadata
subcases verified the correction at the CLI and importer boundaries. Five
further CLI cases cover malformed usage objects, excessive integer durations
and boolean, floating-point or null gate exit statuses. Invalid statuses
refuse the import; they never become passing or failing test evidence.
