---
adr: "0055"
title: Declared external skill candidates for advise
status: proposed
created: 2026-11-02
last_modified: 2026-11-02
scope: "Where `advise` gets candidate skills: an operator-supplied candidate document from an external router, its bounds, trust label, report statuses and replay rule. No change to catalog trust, skill attachment rules, preparation or execution."
amended_by: []
supersedes: []
superseded_by: []
related: ["0002", "0013", "0051", "0054"]
---

# ADR 0055: Declared external skill candidates for advise

## Context

Issue #243 asks where `advise` gets candidate skills before #237 wires them.
The owner wants suggestions from a real skill library. Facts below were read at
`c61529c`.

- `advise` takes skills only from `--skill <ID>`: at most 64 ids
  (`crates/agent-router-cli/src/advisory.rs:29`), source `operator-declared`,
  score 0 (`advisory.rs:31`, `advisory.rs:842-849`).
- `route --retrieval <index>` fills candidates from a BM25 index
  (`crates/agent-router-cli/src/main.rs:657-669`). The index is built by
  `agent-router index --metadata <asset metadata> --catalog <catalog>`
  (`crates/agent-router-cli/src/command.rs:561-577`), binds the catalog hash and
  returns only trusted assets
  (`crates/router-retrieval/src/index.rs:601-620`).
- A candidate attaches to a plan stage only if it is a catalog skill with trust
  `builtin`, `trusted_local` or `verified_third_party`, lists the task kind, and
  each required capability is `Supported` in the request and listed by the
  model (`crates/router-policy/src/preset.rs:171-201`). A candidate absent from
  the catalog is skipped (`preset.rs:178-180`).
- The three catalogs declare the same three skills: `tdd` (implement, needs
  `tools`), `critique-plan` (architecture, migration, security, no capability)
  and `security-guardian` (security, review, needs `tools`). BM25 over three
  documents already filtered by kind adds nothing.
- A cache-only Codex observation keeps `tools` `unknown`
  (`crates/agent-router-cli/tests/codex_catalog.rs:60`), so on Codex only
  `critique-plan` can attach without `--declare-capability tools`. The report
  labels that case `capability_not_supported` (`advisory.rs:458-460`); #244
  replaces it with `capability_unknown`.
- `router-import` has no CLI caller and emits only `untrusted` entries, with no
  signing key or activation code (ADR 0013).
- A candidate document contract already exists: the "BM25 candidate document"
  v1 (`docs/reference/json-contracts.md`, section of that name), parsed by
  `router_policy::parse_candidate_document`
  (`crates/router-policy/src/retrieval.rs:61-155`): `schema_version` `"1"`, one
  `source` of 1 to 256 bytes, at most 512 candidates, unique non-empty skill
  ids, integer scores from 0 to 1,000,000, unknown keys refused, a per-candidate
  `path` accepted and never read, 1 MiB maximum. Only `router-policy` tests call
  it; no CLI command does.
- A `RouteRequest` accepts at most 256 candidates, each with a non-empty
  source and a score from 0 to 1,000,000 (`crates/router-policy/src/route.rs:195-220`).
  `decision_id` hashes the canonical request, candidates included
  (`route.rs:81-91`).
- `router-prepare` loads the skill bodies of a selected phase candidate into
  the execution input (`crates/router-prepare/src/lib.rs:415-440`), so a
  trusted skill sits on a path toward execution. Whether the skills of an
  advisory `DecisionEnvelope` feed that path was not traced for this ADR; the
  decision below keeps outside-catalog candidates out of both.
- The owner runs an external skill router (BM25 over a few dozen `SKILL.md`
  files with a calibrated threshold per skill). It lives outside this
  repository; its output format and calibration were not read for this ADR.

## Options

### Option A: bridge from an external router

`advise --skill-candidates <file>` reads a bounded candidate document produced
outside the router. Candidates are declared, never verified, never granted
authority, and written into `route-request.json` so the decision replays
without the external tool.

### Option B: import with trust promotion

Wire `router-import` into the CLI and add a promotion path (signed manifest,
operator key) so library skills become `trusted_local` catalog skills. They
could then attach to plan stages and, as trusted skills, become loadable by
`router-prepare`.

### Option C: status quo

Catalog skills only. Three skills, at most one attachable on Codex. Does not
meet the owner's goal.

## Decision

Recommended: Option A, reusing the existing candidate document v1 instead of a
new schema. No code reading showed a blocker.

### Input

- `advise --skill-candidates <file>`, at most one file per run, parsed by
  `parse_candidate_document` with its existing limits and error codes.
- `advise` adds stricter bounds, each refused with a named error and never
  truncated:
  - at most 256 candidates, the `RouteRequest` limit
    (`advisory.skill_candidates.too_many`);
  - each skill id is 1 to 96 bytes of `[A-Za-z0-9._:-]` and starts with an
    ASCII letter or digit (`advisory.skill_candidate.invalid_id`). This is the
    path-free identifier rule of the advisory outcome record
    (`crates/router-feedback/src/advisory_outcome.rs:27-28`), so a suggested id
    can later be recorded in `used.skills`. It also keeps ids printable in the
    human report;
  - the document `source` is a label of 1 to 64 bytes of `[a-z0-9._-]`
    (`advisory.skill_candidates.source_invalid`).
- In the request, each file candidate gets the source
  `external-declared:<label>`. That prefix is reserved: an operator `--skill`
  keeps `operator-declared`, and a document cannot claim either value.
- Merge with `--skill`: the union, at most 256 rows. When an id appears in
  both, the operator row wins and the file row is dropped from the request,
  then reported as `operator_declared`.
- Order in `route-request.json`: by skill id, then source. The same candidate
  set gives the same request bytes and the same `decision_id` whatever the
  order in the file.
- The SHA-256 of the file bytes is reported as a `declared` fact. It is not in
  the request: the candidates themselves are.

### Catalog membership and trust

Attachment to the decision keeps today's rule: only trusted catalog skills
that pass kind and capability checks. Option A does not create a new skill
class in the catalog or in the decision.

A candidate outside the catalog is reported, never attached. It never appears
in `decision.stages[*].skills` and is never a catalog or phase-candidate skill,
so it has no path to `router-prepare` or a native run. Its row says that the external router named it, that its body was
not read, and that nothing about it was verified. This is a report-only
suggestion with no authority. `advise` executes nothing (ADR 0051); a human
who loads the skill in a host does so outside the router.

Trust implications, stated plainly:

- The router cannot vouch for an outside-catalog skill. A wrong or compromised
  external router can point the human at a harmful skill in their own library.
  The label `declared` and the absence of attachment are the only protections.
- The id charset bounds what the file can inject into the report; the score
  and source are echoed as data.
- No capability is inferred from a candidate. A catalog skill that needs
  `tools` stays blocked on Codex until `tools` is declared or proven (#244).

### Selection, cap and floor

Under `stable-v2` (ADR 0054), skills are chosen after the plan:

- catalog candidates: at most 2 per stage, for the selected model of that
  stage, ranked operator rows first, then score descending, then id. File
  candidates need a score of at least 300000 (declared prior). Without stage
  affinity (#238), each stage gets the same ranking;
- outside-catalog candidates: the two highest above the floor, by score then
  id, are reported as `outside_catalog_suggested` and named in the next manual
  action; the others as `outside_catalog`.

The floor applies to the producer's score, which is not on the scale of the
BM25 index's `route_utility_micros`. The external router is expected to drop
candidates below its own per-skill threshold first; the floor is a coarse
second cut.

Under `stable-v1`, file candidates follow the frozen v1 rules: no cap, and
attached scores enter the v1 utility. Only catalog skills can attach, so the
skill payload is bounded by the catalog's skill count.

With the cap, the skill payload per combination is at most 1 + 3 x 2 = 7. On
v3 (6 plan, 10 execute, 8 verify candidates) that is 480 x 7 = 3,360 work units
against the 1,000,000 limit.

### Report statuses

Every candidate in the file gets one row: none is dropped silently. Each row
carries `id`, `status`, `source`, `score_micros`, `provenance: "declared"` and
the stages it is attached to.

| Status | Meaning |
| --- | --- |
| `suggested` | catalog skill attached to at least one stage |
| `not_attached` | eligible catalog skill cut by the per-stage cap |
| `below_floor` | file score under 300000 |
| `untrusted` | catalog skill with a trust that never attaches |
| `incompatible_task_kind` | catalog skill not listed for the task kind |
| `capability_unknown` | blocked only by an UNKNOWN capability (#244) |
| `capability_not_supported` | blocked by an unsupported or absent capability |
| `outside_catalog_suggested` | not in the catalog, above the floor, top 2 |
| `outside_catalog` | not in the catalog, other rows |
| `operator_declared` | same id given by `--skill`; the operator row is used |

### Determinism and replay

Parsing, bounds, merge, order and selection are pure functions of the file
bytes, the `--skill` values, the request and the catalog. No clock, network or
store. `route --policy <policy_version>` on the saved `route-request.json`
reproduces `decision.json` without the file or the external router. The report
rows can be recomputed from the same saved request and catalog.

## Consequences

- The owner can bring library suggestions into `advise` with a small adapter
  on the external router side that writes candidate document v1.
- The decision contract, the catalog schema, skill trust rules and
  `router-prepare` are unchanged.
- Two runs with different candidate files give different `decision_id`s even
  when the plan is the same, because the request differs. That is the replay
  identity working, not a change of advice.
- Outside-catalog suggestions are advice to a human, with no verification.
  They are not measured; benefit stays with #175.

### UNKNOWN

- The external router's score scale and calibration, and whether 300000 is a
  useful floor on it.
- Whether the owner's router can emit document v1 without loss (per-skill
  thresholds are not part of the document).
- Which stages a library skill fits; no stage affinity until #238.

### Acceptance (#237)

| Requirement | Test |
| --- | --- |
| A library skill reaches the report for an implement task | `advise_suggests_a_library_skill_for_an_implement_task` (RED first): an outside-catalog candidate is `outside_catalog_suggested` and absent from `decision.stages[*].skills` |
| Cap and determinism | `skills_per_stage_are_capped_and_selection_is_deterministic`: 256 candidates, 11 models, `selected`, no `WorkLimitExceeded`; negative control without the cap |
| No untrusted attachment | `advise_does_not_attach_untrusted_candidate` |
| Replay | `route --policy` on the saved `route-request.json` equals `decision.json` without the file |
| Bounds | `advise_refuses_more_than_256_candidates`, `advise_refuses_invalid_candidate_id`, `advise_refuses_reserved_source_label` |
| Order independence | `candidate_order_in_file_does_not_change_decision_id` |
| Golden | the #234 matrix shows a skill line for implement and security cases, attached or with a named status |

## Rejected and deferred options

- Option B, import with trust promotion: deferred. It reverses the inert stance
  of ADR 0013, needs a signing and promotion flow and a catalog revision per
  library change, and makes library bodies loadable by `router-prepare`.
  Trigger: the owner wants a library skill attached to a plan stage for native
  preparation, not only suggested to a human.
- Option C, status quo: rejected for the owner's goal.
- A catalog-bound BM25 index in `advise`, reusing `route --retrieval`: it
  reaches only catalog skills at attachment, needs an asset metadata document
  per catalog, and duplicates the owner's external router.
- Treating `tools` as supported on Codex so more catalog skills attach:
  rejected; unknown evidence is never successful verification (ADR 0002).
