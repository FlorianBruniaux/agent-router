# Advisory route for one real task

English | [Français](advisory-route.fr.md)

`advise` takes one complete task of your own and returns either a model, effort
and skills recommendation per phase, or an explained refusal. It ends with the
next manual action to take in your host. It is advisory only: it launches no
host or model, contacts no provider, grants no permission and enforces no budget.
It works with the bundled Codex catalogs and with the Claude Code catalogs
`claude-core-v2` (rated) and `claude-core-v1` (unrated); any catalog and host description with the
same contracts is read the same way.

The decision itself comes from the existing `route` policy. `advise` only maps
the task to a route request, labels where every fact comes from and writes the
next manual action. [ADR 0051](adr/0051-advisory-task-route.md) records the design.

## 1. Describe the task

Create a private parent directory, then let [`task-guide`](task-guide.md) collect
the facts. The parent must not be writable by group or others.

```sh
mkdir -m 700 -p ~/agent-router-advice
agent-router task-guide --text 'Add a --dry-run flag to scripts/backup.sh' \
  --interactive --output-directory ~/agent-router-advice/backup-task --json
```

Example answers for that task: `implement`, `medium`, `internal`, a budget
object, `["workspace_write"]`, an acceptance list, `["scripts"]`, then the
target environment `local`, operations `["workspace_change"]` and `reversible`
when the risk normalizer asks for them. The command exports `input.json`,
`risk-context.json` and `normalization.json` into the new directory.

`task-guide` never asks for required host capabilities. If the task needs tools,
add `"required_capabilities": ["tools"]` to a copy of `input.json` and run
`task-guide --task <copy>` again. Without a required capability, `advise`
checks no host capability and says so in its summary.

## 2. Describe the host

Choose one source of host facts.

**Discovered**: read the Codex model cache. This reads one local file and
launches nothing. Supply the independently observed full version of your Codex
executable.

```sh
agent-router inspect --host codex --model-cache ~/.codex/models_cache.json \
  --observed-version <codex version> --json \
  > ~/agent-router-advice/observation.json
```

Models and efforts from the cache are `discovered`. The cache says nothing
about tool support, so `tools` stays `unknown`.

**Declared**: write a `HostSnapshot` file yourself (see the
[route request contract](reference/json-contracts.md#route-request)). Every
value in it is `declared`. A capability you do not list stays `unknown`; it is
not treated as absent.

```json
{
  "id": "my-codex",
  "harness": "codex",
  "supported_models": ["gpt-5.6-sol", "gpt-5.6-terra"],
  "supported_efforts": {
    "gpt-5.6-sol": ["high", "xhigh"],
    "gpt-5.6-terra": ["medium", "high"]
  },
  "capabilities": ["tools"]
}
```

## 3. Ask for advice

```sh
agent-router advise \
  --task ~/agent-router-advice/backup-task/input.json \
  --risk-context ~/agent-router-advice/backup-task/risk-context.json \
  --catalog assets/catalogs/codex-core-v3.json \
  --host-snapshot ~/agent-router-advice/observation.json \
  --skill tdd \
  --output-directory ~/agent-router-advice/backup-advice
```

Use `--host <file>` instead of `--host-snapshot` for a declared host. `--skill`
names a candidate skill and can be repeated. `--skill-candidates <file>` reads
skills named by an external router (see below). Add `--json` for the complete
versioned report. Exit `0` means a recommendation, `3` means no recommendation
or open questions, `2` means invalid input.

`--policy stable-v2` is the default. It sets, for each stage, a quality target
from the task risk and kind, then ranks plans by total shortfall against those
targets, then by cost ([ADR 0054](adr/0054-advisory-stable-v2-policy.md)).
`--policy stable-v1` reproduces the earlier flat scoring byte for byte, which is
the rollback. The report is still version 1 and gains a `scoring` object under
`stable-v2`: per stage, `target_micros`, `quality_micros`, `shortfall_micros` and
`reason_codes`. The margins and kind adjustments are declared priors, not
measurements. A reason code describes catalog data, not model quality:

| Code | Meaning |
| --- | --- |
| `catalog.stage.dominated` | One eligible model is at least as good on quality, cost and latency as every other, so no target changes the pick (the plan stage of `codex-core-v3`) |
| `catalog.cost.equal` | Every model meeting the target has the same cost, so the pick comes from the quality tie-break |
| `catalog.unrated` | Every eligible model has quality 0, so the pick is a tie-break |

`critical` risk also requires the strongest eligible verifier
(`verification.critical.not_max_quality` for the others).

Effort per stage also follows risk and kind under `stable-v2`, from a table of
declared priors (plan / execute / verify): low `medium / low / medium`, medium
`high / medium / medium`, high `high / medium / high`, critical
`xhigh / high / xhigh`. `architecture` and `migration` raise the plan effort by
one level, `security` and `review` raise the verify effort by one, `explore`
lowers the verify effort by one; never below `low`, never above `max`. If the
host does not offer the level, the nearest supported level is used and the
`scoring` entry says so: `effort_target` is what the table asked for, `effort` is
what you get, and `effort_provenance` is `policy_table` or `fallback`.
`table_digest` identifies the tables, `effort_basis` is
`declared_policy_prior`, and `effort_cost` is `not_modelled`: effort does not
enter cost or ranking. `stable-v1` keeps its fixed efforts (plan `high`, execute
`medium`, verify `high`).

## 4. Read the result

With the sanitized fixture cache used in the tests and a task requiring
`tools`, nothing is recommended:

```text
Status: no_recommendation
  host.models [discovered]: [...]
  host.capability.tools [unknown]: unknown
Next manual action:
  Nothing was executed and no model is recommended for your codex host. Keep your current host choice, or supply one of the facts listed in would_change and run advise again.
  - host.capability.unknown tools: Supply authenticated host evidence for 'tools', or declare it with --declare-capability tools; a declaration is reported as declared, not verified.
```

If you know your host session can use tools, say so explicitly:

```sh
agent-router advise ... --declare-capability tools
```

The same task then gets a recommendation:

```text
Status: recommended
  host.capability.tools [declared]: supported
Skill tdd: suggested
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/medium
Next manual action:
  Nothing was executed. In your codex host, select per phase plan: model gpt-5.6-sol at effort high with skills tdd; execute: model gpt-5.6-terra at effort medium with skills tdd; verify: model gpt-5.6-terra at effort medium with skills tdd. Catalog codex-core-v3: ratings come from the catalog, a declared policy prior, not a measurement. Support for tools is declared by you, not verified.
```

Select that model and effort in your host for each phase, load only the listed
skills, and keep your host's own permission settings. When a phase is marked
`fresh_instance`, run it in a new session. A declaration removes the snapshot
hash from the decision, because the decision no longer depends on the
observation alone. `network` cannot be declared, neither with the flag nor in a
host file (`advisory.declaration.network`). A declaration that contradicts
evidence of `unsupported` is refused (`advisory.declaration.contradicts_evidence`),
and so is any declaration with a generated catalog that embeds its own
observation (`advisory.declaration.embedded_observation`): `route` always rebinds
that observation, so the saved request could not replay.

### Claude Code example

[`claude-core-v2`](../assets/catalogs/README.md#claude-core-v2) rates the five
Claude entries: costs are the published output prices relative to Fable 5.1,
and qualities and stages are a proposed declaration that mirrors the Codex
tiers ([ADR 0057](adr/0057-claude-core-v2-declared-ratings.md)). Every entry
declares only `tools`, a model-side prior that does not replace a host fact. No
passive discovery of Claude models or efforts exists, so describe the host in a
declared file with the identifiers and efforts your account actually offers:

```json
{
  "id": "my-claude-code",
  "harness": "claude",
  "supported_models": ["claude-fable-5-1", "claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"],
  "supported_efforts": {
    "claude-fable-5-1": ["low", "medium", "high", "xhigh", "max"],
    "claude-haiku-5-5": ["low", "medium", "high", "xhigh", "max"],
    "claude-opus-5-5": ["low", "medium", "high", "xhigh", "max"],
    "claude-sonnet-5-5": ["low", "medium", "high", "xhigh", "max"]
  },
  "capabilities": []
}
```

```sh
agent-router advise --text 'Add a --dry-run flag to scripts/backup.sh' \
  --kind implement --risk medium --scope scripts --budget 0:600000:200000 \
  --catalog assets/catalogs/claude-core-v2.json --host ~/agent-router-advice/claude-host.json
```

For a task without required capabilities, the output contains:

```text
Plan: claude-opus-5-5/high -> claude-sonnet-5-5/medium -> claude-sonnet-5-5/medium
...
Next manual action:
  Nothing was executed. In your claude host, select per phase plan: model claude-opus-5-5 at effort high with no skill; execute: model claude-sonnet-5-5 at effort medium with no skill; verify: model claude-sonnet-5-5 at effort medium with no skill. Catalog claude-core-v2: ratings come from the catalog, a declared policy prior, not a measurement. The task requires no capability, so no host capability was checked.
```

`selection_basis` is `catalog_policy_prior`. Under `stable-v2` the plan follows
risk and task kind: a critical migration gets
`claude-fable-5-1/max -> claude-opus-5-5/high -> claude-fable-5-1/xhigh`. The
older [`claude-core-v1`](../assets/catalogs/README.md#claude-code-policy-catalog)
rates nothing, so it yields a deterministic tie-break (`unrated_tie_break`). A
task that requires `tools` gets a route only when the host snapshot lists the
capability or you pass `--declare-capability tools`, which is labelled
`declared`. The catalog's `tools` entry does not replace that host fact: without
it the task gets no recommendation.

## 5. Record how it went

After you do the task by hand, you can record whether you followed the
recommendation and how it ended, then read a local report. This needs the
`--output-directory` of step 3, because the record binds to the `decision_id`
saved in `advice.json`. Nothing is executed and nothing leaves your machine.

```sh
echo '{"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":2592000}' \
  > ~/agent-router-advice/prefs.json
agent-router feedback --store ~/agent-router-advice/store --json \
  configure --preferences ~/agent-router-advice/prefs.json

# Followed the recommendation, accepted as is, no cost reported by the host.
agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/backup-advice/advice.json \
  --adherence followed --result accepted-as-is

# Overrode it: say what you used instead, and the host-reported cost if any.
agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/other-advice/advice.json \
  --adherence overridden --result accepted-after-rework \
  --used-model gpt-5.6-terra --used-effort medium --used-no-skill \
  --cost-microusd 4200

agent-router feedback --store ~/agent-router-advice/store --json advisory-report
```

The first command is the explicit consent: nothing is recorded until
`collection_enabled` is true, and `retention_seconds` bounds how long records
stay. Then:

- `--adherence` (or `--followed`, `--overridden`) and `--result` are required. `--result` is `accepted-as-is`,
  `accepted-after-rework` or `abandoned`.
- An override must name at least one of `--used-model`, `--used-effort`,
  `--used-skill` (repeatable) and `--used-no-skill`. What you do not state is
  UNKNOWN. A followed recommendation takes none of them.
- `--cost-microusd` and `--tokens` are what your host reported. Omit them when it
  reported nothing: they stay UNKNOWN and are never stored as zero.
- Only enums, identifiers, numbers and the `decision_id` are stored. An
  identifier is checked for syntax, not against a catalog: letters, digits and
  `._:-`, with no `/`, no `\` and no leading `.` or `~`. The task text,
  objective, paths and any comment are not stored, and free text or a path in a
  flag is refused with `feedback.invalid`.
- Task kind, risk and the recommended phases are copied from the `advice.json`
  you pass, which you can edit. Only `decision_id` is a digest of the request, so
  the grouping by task kind rests on your own file. An override that states a
  model, effort and skills all equal to the recommendation is refused.
- Advising the same task again gives the same `decision_id`. Repeating the
  latest record is harmless; a different outcome (a retry or a correction) is
  stored as the next attempt, and the report counts the latest attempt once.
- A report with `status: no_recommendation` has nothing to follow
  (`advisory.outcome.not_recommended`), and a file that is not a version 1
  advice report is refused (`advisory.outcome.advice_invalid`).

`advisory-report` compares followed with overridden per task kind. Every rate
shows its numerator and denominator, the sample size and the UNKNOWN counts
(cost, tokens and, for overrides, the model, effort and skills used), plus how
many recommendations were a tie-break among unrated models. An empty group has
no rate, not zero. `decisions` and `sample_size` count each decision once;
`records` shows how many attempts were recorded. Costs are counted as known or
unknown, never summed.

`groups` splits the same counts by `policy_version` and `risk`, so you can read
`stable-v1` and `stable-v2` apart. A group under 5 decisions
(`minimum_sample`) is marked `insufficient_sample` and prints no rate.
`paired_trials` groups the decisions of one request by their `request_hash`:
each configuration (`policy_version` and `catalog_version`) is an arm, counted
once per trial, and two arms are never merged. Outcomes recorded before the
hash existed appear under `unknown_request_hash`.

To share a redacted copy or delete everything:

```sh
agent-router feedback --store ~/agent-router-advice/store --json export > outcomes.json
agent-router feedback --store ~/agent-router-advice/store --json purge
```

`export` lists the outcomes under `advisory_outcomes`; they hold no free text.
`purge` deletes them with the rest of the store.

**A personal report is not evidence of a general gain.** The records are
self-reported and unverified, the sample is yours and self-selected, and the
tasks differ. Comparative evaluation is tracked in
[issue #5](https://github.com/FlorianBruniaux/agent-router/issues/5). See
[feedback](feedback.md#advisory-outcomes) and
[ADR 0052](adr/0052-advisory-outcome-record.md).

## Quick entry: one command to advise, one to record

For a task you can state in a line, skip `task-guide` and the JSON files.
`--text`, `--kind` and `--risk` describe the task. `--budget`
(`MICROUSD:MILLISECONDS:TOKENS`) and `--scope` (repeatable) have no default:
omit one and the command exits `3` and asks for it, as `task-guide` does.
`--task` and `--risk-context` cannot be combined with `--text`.

```sh
agent-router advise --text 'Add a --dry-run flag to scripts/backup.sh' \
  --kind implement --risk medium --scope scripts --budget 0:600000:200000 \
  --catalog assets/catalogs/codex-core-v3.json --host-snapshot observation.json \
  --output-directory ~/agent-router-advice/backup-advice

agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/backup-advice --followed --result accepted-as-is
```

`advisory-record` accepts the advice directory in place of `advice.json`, and
`--followed` or `--overridden` in place of `--adherence`. It keeps the rules of
[ADR 0052](adr/0052-advisory-outcome-record.md): consent first, no hidden state,
no "latest advice" lookup. The store path must be absolute.

### Compare the two policies on one task

`advise --compare-policies` decides the same request under `stable-v1` and
`stable-v2` and prints, per stage, each policy's model and effort and whether
they differ. It does not combine with `--policy`, and it runs neither plan.

```sh
agent-router advise --text 'Add a --dry-run flag to scripts/backup.sh' \
  --kind implement --risk high --scope scripts --budget 0:600000:200000 \
  --catalog assets/catalogs/codex-core-v3.json --host-snapshot observation.json \
  --compare-policies --output-directory ~/agent-router-advice/backup-compare

agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/backup-compare --policy stable-v2 \
  --followed --result accepted-as-is
```

With `--output-directory`, each decision is saved as a normal `advise` export in
a subdirectory named after its policy (`stable-v1/`, `stable-v2/`), each with its
own `decision_id`. `compare.json` is written last and lists the arms, the stage
table and a `request_hash` that is the same for every arm. Exit codes follow
`advise`: `0` when every policy recommends, `3` otherwise.

`advisory-record` needs `--policy <name>` for a comparison directory (or its
`compare.json`) and refuses it for a single advice. `--followed` records that you
followed that policy's plan. `--overridden` with the `--used-*` fields records
that you departed from it. To say you followed neither, run the command once per
policy with `--overridden`. The record binds to that policy's `decision_id` and
carries the `request_hash` of `compare.json`, the same for every arm; an arm
whose `advice.json` states another one is refused
(`advisory.outcome.advice_invalid`). A single advice records the `request_hash`
of its `advice.json` too (ADR 0056).

Facts you do not state take these fixed values. They are not inferred from the
text and each one appears in the report with provenance `defaulted`:

| Fact | Flag | Default |
| --- | --- | --- |
| Sensitivity | `--sensitivity` | `internal` |
| Environment | `--environment` | `local`; none for `migration`, which is asked |
| Reversibility | `--reversibility` | `reversible`; none for `migration`, which is asked |
| Operations | `--operation` | `workspace_change` for `implement` and `migration`, else `read_only` |
| Allowed mutations | `--mutation` (`none` for an empty set) | `workspace_write` for `implement` and `migration`, else none |
| Acceptance | `--acceptance` | `The operator judges the result by hand.` |

`critical` risk and `irreversible` effects are never defaulted: they exist only
when you pass `--risk critical` or `--reversibility irreversible`. A fact you
state is not marked `defaulted`. With every flag equal to its default the report
is byte-identical to the file-based path; without them the decision and the
task mapping are the same and only the `defaulted` fact rows are added.
`--require-capability` names required host capabilities; none by default.

## Provenance

| Label | Meaning |
| --- | --- |
| `declared` | Stated by you, your host file, `--declare-capability` or the catalog prior. Not checked. |
| `discovered` | Read from a host observation whose digest and age were checked. Not authenticated. |
| `verified` | Checked by this command, such as the observation digest, or backed by authenticated evidence. |
| `unknown` | No evidence was supplied. |
| `defaulted` | A fixed quick-entry default used because you stated nothing. Not a statement by you. |

Catalog ratings are always `declared`: `builtin-policy-prior` and
`builtin-unrated-policy-prior` are policy bounds, not measured quality, price or
latency.

## Task mapping

| Task field | Advisory route |
| --- | --- |
| `objective`, `kind`, `acceptance`, `required_capabilities` | Routed unchanged |
| `risk` | The effective risk after the normalizer's floor is routed; the declared value is reported next to it |
| `scope`, `budget`, `allowed_mutations`, `sensitivity` | Reported as `not_enforced_by_advisory_routing` with their values |
| `deliverables`, `constraints`, `dependencies` | Reported as `not_enforced_by_advisory_routing` |

Nothing is dropped silently: every field appears in `mapping` with its handling.

## Refusals and limits

- A host model absent from the catalog is refused with `advisory.model.unknown`;
  the command never guesses a rating or an alias.
- A skill absent from the catalog is reported as `absent_from_catalog` and never
  suggested. A catalog skill is suggested only when its trust, task kinds and
  required capabilities fit and the router attaches it to a selected phase.
- An incomplete task returns `needs_clarification` with the open questions and
  writes nothing; if `--output-directory` was given, the summary says the
  directory was not created. Answer them with `task-guide --task <file> --interactive`.
- A task file over 1 MiB is refused with `advisory.task.too_large`.
- `capability attest` records do not feed `advise`: an attestation keeps
  capability support `unknown` in the observation and is consumed only by
  phase routing with an explicit acceptance list.
- Skills come from `--skill` and from `--skill-candidates`; there is no
  retrieval index option in `advise`.

## Skills declared by an external router

`--skill-candidates <file>` takes a candidate document v1 (the BM25 candidate
document in [JSON contracts](reference/json-contracts.md)), at most one file per
run. It is declared input: the report labels every row `declared`, never
verified, and records the SHA-256 of the file as a `declared` fact. The router
executes nothing and reads no skill body ([ADR 0055](adr/0055-declared-external-skill-candidates.md)).

Bounds, each refused with a named error and never truncated:

- the document limits of the candidate document (1 MiB, unique ids, scores 0 to
  1,000,000, unknown keys refused), with their `retrieval.*` codes;
- at most 256 candidates, counting `--skill` ids (`advisory.skill_candidates.too_many`);
- each skill id is 1 to 96 bytes of `[A-Za-z0-9._:-]` and starts with a letter or
  digit (`advisory.skill_candidate.invalid_id`);
- the document `source` is 1 to 64 bytes of `[a-z0-9._-]` and not
  `operator-declared` (`advisory.skill_candidates.source_invalid`).

In the saved `route-request.json` each file candidate has the source
`external-declared:<source>`, and the candidates are ordered by skill id, so the
file order does not change the `decision_id`. An id given by `--skill` keeps the
`operator-declared` row; the file row is dropped from the request and reported
as `operator_declared`. The saved request replays through `route` without the
file.

Under `stable-v2` a stage gets at most 2 skills, ranked `--skill` first, then
score descending, then id, and a file candidate needs a score of at least
300000. Skills are chosen after the plan, so they never change the models.
Under `stable-v1` the frozen rules apply: no cap and no floor. Only trusted
catalog skills attach. A candidate the catalog does not know is never attached:
it is reported as `outside_catalog_suggested` (the two best at or above 300000,
also named in the next manual action) or `outside_catalog`, and loading it is
your decision, outside the router.

Each file row has `id`, `status`, `source`, `score_micros`, `provenance`
(`declared`) and `stages`. Statuses: `suggested`, `not_attached` (cut by the
cap), `below_floor` (`stable-v2` only), `untrusted`, `incompatible_task_kind`,
`capability_unknown`, `capability_not_supported`, `outside_catalog_suggested`,
`outside_catalog` and `operator_declared`. A candidate that is not attached
because of trust, task kind or capability keeps that status even when its score
is under the floor.

## Saved files

Without `--output-directory`, nothing is written. With it, the command creates
a new directory with mode 0700 and files with mode 0600, using the same
no-clobber publication as `task-guide`. It never reuses an existing path.

| File | Content |
| --- | --- |
| `task.json` | The task input as read |
| `risk-context.json` | The explicit risk context, if supplied |
| `route-request.json` | The exact request given to the router |
| `decision.json` | The decision; `explain --decision` reads it |
| `advice.json` | The complete report, published last as the completion marker. It is identical to the printed `--json` report, including `output_directory` |

`route --request route-request.json --catalog <catalog> --policy <policy_version>
--json` prints the same bytes as `decision.json` (tested), where
`<policy_version>` is the `policy_version` of `decision.json` (`stable-v2` unless
`advise --policy stable-v1` was used). `route` defaults to `stable-v1`, so a
saved `stable-v2` decision needs the explicit flag:

| Host source | Replay |
| --- | --- |
| `--host` file | as above |
| `--host-snapshot`, no declaration | add the same `--host-snapshot` |
| `--host-snapshot` with `--declare-capability` | as above, without the snapshot: the request holds the overlay and the decision is unbound |
| generated catalog with an embedded observation | as above; `route` rebinds the embedded observation, which is why declarations are refused there |

These files contain your task text; keep them out of public repositories.
