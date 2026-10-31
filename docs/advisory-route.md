# Advisory route for one real task

English | [Français](advisory-route.fr.md)

`advise` takes one complete task of your own and returns either a model, effort
and skills recommendation per phase, or an explained refusal. It ends with the
next manual action to take in your host. It is advisory only: it launches no
host or model, contacts no provider, grants no permission and enforces no budget.
It works with the bundled Codex catalogs and with the operator-declared
Claude Code catalog `claude-core-v1`; any catalog and host description with the
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
  --catalog assets/catalogs/codex-core-v2.json \
  --host-snapshot ~/agent-router-advice/observation.json \
  --skill tdd \
  --output-directory ~/agent-router-advice/backup-advice
```

Use `--host <file>` instead of `--host-snapshot` for a declared host. `--skill`
names a candidate skill and can be repeated. Add `--json` for the complete
versioned report. Exit `0` means a recommendation, `3` means no recommendation
or open questions, `2` means invalid input.

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
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Next manual action:
  Nothing was executed. In your codex host, select per phase plan: model gpt-5.6-sol at effort high with skills tdd; execute: model gpt-5.6-terra at effort medium with skills tdd; verify: model gpt-5.6-terra at effort high with skills tdd. Catalog codex-core-v2: ratings come from the catalog, a declared policy prior, not a measurement. Support for tools is declared by you, not verified.
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

[`claude-core-v1`](../assets/catalogs/README.md#claude-code-policy-catalog) is
operator-declared: every entry is unrated (quality 0) and declares only
`tools`, a model-side prior that does not replace a host fact. No passive
discovery of Claude models or efforts exists, so describe
the host in a declared file with the identifiers and efforts your account
actually offers:

```json
{
  "id": "my-claude-code",
  "harness": "claude",
  "supported_models": ["claude-haiku-4-5-20251001", "claude-opus-5-5", "claude-sonnet-5-5"],
  "supported_efforts": {
    "claude-haiku-4-5-20251001": ["medium", "high"],
    "claude-opus-5-5": ["medium", "high", "xhigh"],
    "claude-sonnet-5-5": ["medium", "high"]
  },
  "capabilities": []
}
```

```sh
agent-router advise --task ~/agent-router-advice/backup-task/input.json \
  --risk-context ~/agent-router-advice/backup-task/risk-context.json \
  --catalog assets/catalogs/claude-core-v1.json --host ~/agent-router-advice/claude-host.json
```

For a task without required capabilities, the output ends with:

```text
Plan: claude-haiku-4-5-20251001/high -> claude-haiku-4-5-20251001/medium -> claude-haiku-4-5-20251001/high
Next manual action:
  Nothing was executed. In your claude host, select per phase plan: model claude-haiku-4-5-20251001 at effort high with no skill; execute: model claude-haiku-4-5-20251001 at effort medium with no skill; verify: model claude-haiku-4-5-20251001 at effort high with no skill. Catalog claude-core-v1: every selected model is unrated in the catalog, so this choice is a deterministic tie-break, not a quality recommendation. The task requires no capability, so no host capability was checked.
```

`selection_basis` is `unrated_tie_break`: with equal scores the router picks by
identifier order, so the result names a model, not a better one. A task that
requires `tools` gets a route only when the host snapshot lists the capability
or you pass `--declare-capability tools`, which is labelled `declared`. The
catalog's `tools` entry does not replace that host fact: without it the task gets
no recommendation.

## Provenance

| Label | Meaning |
| --- | --- |
| `declared` | Stated by you, your host file, `--declare-capability` or the catalog prior. Not checked. |
| `discovered` | Read from a host observation whose digest and age were checked. Not authenticated. |
| `verified` | Checked by this command, such as the observation digest, or backed by authenticated evidence. |
| `unknown` | No evidence was supplied. |

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
- There is no retrieval index option yet; skills come only from `--skill`.

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

`route --request route-request.json --catalog <catalog> --json` prints the same
bytes as `decision.json` (tested):

| Host source | Replay |
| --- | --- |
| `--host` file | as above |
| `--host-snapshot`, no declaration | add the same `--host-snapshot` |
| `--host-snapshot` with `--declare-capability` | as above, without the snapshot: the request holds the overlay and the decision is unbound |
| generated catalog with an embedded observation | as above; `route` rebinds the embedded observation, which is why declarations are refused there |

These files contain your task text; keep them out of public repositories.
