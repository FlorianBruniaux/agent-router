# Supplied per-physical usage pricing

This offline bridge prices explicitly supplied `HostUsage` for existing model
observations in an accepted-changes input. It reuses qualified pricing and the
unchanged B1 calculator. No collector, provider call, native execution, invoice
verification or producer qualification occurs.

```sh
agent-router eval supplied-usage-pricing \
  --input tests/fixtures/evaluation/supplied-usage-pricing/complete.json --json
agent-router eval supplied-usage-pricing \
  --input tests/fixtures/evaluation/supplied-usage-pricing/complete.json --portable --json
```

The checked-in example is synthetic. Three physical model calls, including a
failed call and a timed-out call, each price to 150 microusd. Their supplied
complete inventory produces 450 microusd for one selected accepted change.
This establishes arithmetic for the fixture, not real cost, savings or quality.

## Input and identity

`SuppliedUsagePricingInput` schema version 1 contains:

- `accounting`: the existing `AcceptedChangesInput` schema version 1.
- `pricing`: `QualifiedPricingTable` schema version 2 with explicit cache and
  reasoning rate semantics.
- `physical_usage`: at most 4096 rows, each containing `key`, `evidence_sha256`,
  `model`, normalized `usage`, and `scope: "per_physical_call"`.

The entire input is limited to 4 MiB. JSON duplicate keys, unknown fields,
unsupported versions and unsupported usage scopes are refused. Missing scope,
session totals, cumulative snapshots and last-turn counters cannot be joined.
Missing rates or rate scope produce unknown totals, with any calculable subtotal
kept separately. There is no assumption that an omitted counter means zero.

Every row must reference an existing model observation with exactly the same
`PhysicalObservationKey` and evidence digest. A physical key consists of
`authority_sha256` and `physical_sha256`; identical rows for that key count once.
Conflicting rows are rejected. A preexisting cost is rejected even if the new
estimate would happen to equal it, avoiding ambiguous replacement.

External request IDs never determine deduplication. Two physical calls with the
same external ID remain distinct. The same physical digest under two authorities
also remains distinct. The bridge cannot reconcile cross-authority views of one
call. A supplier must establish an additive inventory separately; the bridge
preserves its claims without certifying them.

## Calculation and evidence limits

The original accounting is validated before any cost is changed. This preserves
rejection of duplicate conflicts that a replacement could otherwise hide. Usage
counts must agree with provenance, remain within `HostUsage::MAX_TOKENS`, and
respect the input/cache and output/reasoning subset relationships. Known input
plus output must also remain within that bound. Pricing overflow is rejected.

Only `QualifiedUsageCost.total_microusd` can populate an observation's
`cost_microusd`. A partial `known_subtotal_microusd` stays in the pricing
projection and cannot become a B1 observation total. Failed calls, retries and
timeouts remain included through the existing B1 accounting rules.

No usage coverage, pricing coverage, inventory, acceptance or identity is
upgraded. An incomplete inventory stays incomplete even when every supplied row
can be priced. An omitted usage row leaves its observation's cost unchanged.
The transformed accounting is then validated and calculated through B1 again.
Its report/input digests bind that transformed input; the portable projection
also retains the original accounting input digest.

## Local report and portable projection

The default report contains `accounting`, the existing B1 report, and
`portable_pricing`. The default report is local evidence: its B1 fields retain
supplied assignment IDs and external request IDs. It is **not** a portable
allowlist export.

`--portable` emits only `portable_pricing`. This typed allowlist contains hashes,
fixed strings/enums, numeric costs and counts, nullable costs, and per-row
supplied coverage enums. Model names are hashed. Pricing source/version text,
assignment IDs, external request IDs and arbitrary references are omitted.
No regex sanitization is used. The projection retains an input digest, original
accounting input digest, priced accounting report digest, pricing-table digest,
and each source evidence digest and usage-row digest.

Both reports label evidence `supplied_unverified`. The projection labels cost
`estimated_not_invoice` and explicitly lists unverified producer authenticity,
identity, coverage and cross-authority overlap. Hashes bind supplied content;
they do not authenticate it. Portable per-row subtotals are not a new additive
cohort total. Retain original private evidence for any later reconciliation.

The Rust entry points are `supplied_usage_pricing_report(&[u8])` and
`calculate_supplied_usage_pricing(&SuppliedUsagePricingInput)`, both returning
`Result<SuppliedUsagePricingReport>`. The JSON entry point suppresses
content-bearing parser diagnostics and emits only static error descriptions.

## Local contract checks

```sh
cargo test -p router-feedback --test supplied_usage_pricing --test accepted_changes --locked --offline
```

Tests cover physical retries/failures, duplicate conflicts, source substitutions,
missing rates and scope, incomplete coverage, cross-authority identity, token
consistency/overflow, strict parsing, and nonempty secret sentinels in portable
fields and parser errors. Existing B1 fixtures remain unchanged.
