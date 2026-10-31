# Advisory route for one real task

English | [Français](advisory-route.fr.md)

`advise` takes one complete task of your own and returns either a model, effort
and skills recommendation per phase, or an explained refusal. It ends with the
next manual action to take in your host. It is advisory only: it launches no
host or model, contacts no provider, grants no permission and enforces no budget.
This first delivery is Codex first: the bundled catalog covers Codex model
identifiers. Any catalog and host description with the same contracts is read
the same way; this path has not yet been run with the Claude Code catalog (#218).

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
  Nothing was executed. In your codex host, select per phase plan: model gpt-5.6-sol at effort high with skills tdd; execute: model gpt-5.6-terra at effort medium with skills tdd; verify: model gpt-5.6-terra at effort high with skills tdd. Ratings come from catalog codex-core-v2, a declared policy prior, not a measurement. Support for tools is declared by you, not verified.
```

Select that model and effort in your host for each phase, load only the listed
skills, and keep your host's own permission settings. When a phase is marked
`fresh_instance`, run it in a new session. A declaration removes the snapshot
hash from the decision, because the decision no longer depends on the
observation alone. `network` cannot be declared (`advisory.declaration.network`).

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
  writes nothing. Answer them with `task-guide --task <file> --interactive`.
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
| `advice.json` | The complete report, published last as the completion marker |

`route --request route-request.json --catalog <catalog> --json` replays the same
decision. For an unmodified observation, also pass `--host-snapshot`. These
files contain your task text; keep them out of public repositories.
