# Correlate attempts with supplied physical calls

B1b exposes the offline B1 calculator for [issue #120](https://github.com/FlorianBruniaux/agent-router/issues/120). It reads supplied files and writes the report to stdout. It opens no runtime store, starts no provider and changes no execution authority.

## Try the committed example

From the repository root:

```sh
cargo run -p agent-router-cli --locked -- eval attempt-correlation \
  --input tests/fixtures/evaluation/attempt-correlation-v1.json --json
```

Use `--json` for automation and reproducible reports. Without it, the existing evaluation output adapter renders a human view.

The input is synthetic. `evidence_trust: supplied_unverified` applies to the entire report. Each attempt either lists explicitly claimed physical observations or retains `physical_sends: "unknown"`. An absent claim never becomes zero calls. Calls without an attempt claim remain in `unattributed`.

Repeated identical physical observations count once. Conflicting observations for the same `(authority_sha256, physical_sha256)` refuse the input. Sharing an external request ID does not merge distinct physical observations. A claim to an attempt absent from the supplied runtime block also refuses the input.

## Read a complete observation v2 export

First obtain a [passive observation](run-observation.md) for an existing run:

```sh
agent-router observe --store "$AR_STORE" --run-id "$AR_RUN_ID" \
  --schema-version 2 --json > observation-v2.json
```

Then supply the physical observations and their caller-assigned runtime authority:

```sh
agent-router eval attempt-correlation \
  --observation observation-v2.json \
  --physical physical-observations.json \
  --runtime-authority-sha256 "$AR_RUNTIME_AUTHORITY_SHA256" --json
```

The runtime authority must be a lowercase 64-character hexadecimal digest chosen by the evidence producer. This command does not discover or authenticate it. The physical file is a strict envelope:

```json
{"schema_version": 1, "physical": []}
```

An empty list supplies no physical evidence. It does not establish that no call occurred. For a nonempty example, use the `physical` array in the committed fixture above. Its fabricated identifiers are not identifiers for your run. A real producer must supply its own source identities, unique positive sequences, evidence hashes and optional explicit attempt claims. This command does not generate missing IDs or infer claims.

The projection accepts the complete published v2 shape, including required nullable fields and the native fields whose only accepted value is `unknown`. It rejects v1, unsupported versions, extra or duplicate fields, invalid values, and disagreement between `known.attempts` and the attempt array. Validating this supplied shape does not recheck the private journal or recompute the snapshot hash.

`--input` and the observation mode are mutually exclusive. The observation mode requires all three arguments. Each input file is bounded at 4 MiB before decoding; the B1 calculator retains its own attempt and physical-observation ceilings.

## What this proves and what remains unknown

Invalid input uses the fixed `eval.attempt_correlation.invalid` error (exit 2); malformed command arguments use the existing `cli.invalid` error. Evaluation refusals use a static error and do not echo file contents or paths.

The two entry modes use the same B1 calculation and report contract. The report hashes bind the supplied projection and output; they do not prove that a provider made a call or that the supplied runtime block faithfully describes the run. The report's `input_is_a_projection_of_observe_v2_not_its_raw_output` limit describes the calculator input after CLI projection.

This command does not join thread IDs, turn IDs, usage, elapsed time or invoices. It never feeds external measurements into `observed_tokens` or `observed_cost_microusd`, which remain runtime escalation authority. A linked observation is not task success, user acceptance or evidence of savings.

The [accepted-revision calculator](accepted-changes.md) remains a separate calculation over supplied costs. Native collection, complete F06a correlation and consumer export F06b remain open. The [measurement binding contract](superpowers/plans/2026-10-10-measurement-binding-contract.md) records the boundaries between these components.

The [self-dogfood report](bench/self-dogfood-b1b-2026-10-10/README.md) records how this command was developed using an Agent Router recommendation and a Codex subagent. Its measurements remain separate from the synthetic correlation examples.
