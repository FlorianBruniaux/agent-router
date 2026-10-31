# Agent Router compared with similar projects

Research date: 2026-09-30; Kimchi added on 2026-10-07. Documentation-only
comparison: no project below was
installed or run for this page. Each cell comes from the project's own
repository or documentation, read on the date above; a cell the sources did
not settle is written "not documented". Kimchi sources have their own date
and a pinned repository revision below. Star counts, release tags and push
dates were read from the GitHub API on the same day and will drift.

This page compares design properties. It does not compare quality, cost or
speed: Agent Router has no measured gain yet (see
[README.md](../README.md#why-it-exists), "No measured cost or quality gain is
claimed yet") and the 2026-09-30 pilot is 10 tasks with one repetition
([bench playbook, "Limits of the 2026-09-30 pilot"](bench/playbook.md#limits-of-the-2026-09-30-pilot)).

## Method

1. Discovery: one Perplexity deep-research query and two Perplexity searches
   for projects that route model choice (and optionally reasoning effort) for
   coding agents or LLM calls, per request, per session or per phase. The
   synthesized research answer was used only to find candidates; none of its
   claims is reused without a primary source.
2. Starting list: the repositories analysed in
   [repository-analysis.md](research/repository-analysis.md) and the earlier
   [Omnigent comparison](omnigent-comparison.md).
3. Verification: each kept project's README, documentation pages and GitHub
   metadata (license, latest release, last push), fetched on 2026-09-30.
   Kimchi was added from its README and Coding overview on 2026-10-07.
   The other entries were not re-verified for that addition.
4. Agent Router side: every claim cites a line of this repository.

A project is kept when it chooses a model (or a model and effort) for coding
work or LLM calls from the task or request. Plain provider gateways that only
load-balance or fail over between deployments of a chosen model are dropped.

## Projects kept

### Kimchi Coding

Kimchi is a coding agent with an LLM orchestrator that selects models from
configured role pools during a session. Its roles include planning, building,
review, exploration and research; requests carry model and phase tags for
usage attribution. This overlaps with Agent Router's phase-based selection,
but the documented choice is made by an orchestrator during execution.

Keep the products distinct: Kimchi Coding uses Kimchi Inference for model
access; Inference is a separate serverless API. The CLI's Apache-2.0 license
does not describe the hosted service's terms. Per-role reasoning effort and
a replayable routing receipt are not established by the sources read.

- [Model roles and delegation](https://github.com/getkimchi/kimchi/blob/74f4ecef40ba182c6fed3a9bca2934057119e52d/README.md#model-roles), [phase tracking](https://github.com/getkimchi/kimchi/blob/74f4ecef40ba182c6fed3a9bca2934057119e52d/README.md#phase-tracking) and [CLI license](https://github.com/getkimchi/kimchi/blob/74f4ecef40ba182c6fed3a9bca2934057119e52d/README.md#license) (read 2026-10-07).
- [Coding overview and Coding vs. Inference](https://docs.kimchi.dev/docs/coding-overview) (read 2026-10-07). No installation, execution or performance comparison was performed for this entry.

### claude-code-router (musistudio)

A local gateway and control plane for coding agents (Claude Code, Codex,
OpenCode, Kimi CLI and others) that listens on `http://127.0.0.1:3456`.
Routing in v3 is user-configured: conditions on request headers or body, then
rewrites, retries and ordered fallbacks, or a Node.js route script with a
bounded API. A `<CCR-SUBAGENT-MODEL>provider/model</CCR-SUBAGENT-MODEL>` tag
routes Claude Code subagent requests. Request logs record the resolved
provider, model, credential, status, latency, tokens and estimated cost.
Client keys carry request, token and image limits. Reasoning effort handling
is not documented on the routing page.

- Repository: [github.com/musistudio/claude-code-router](https://github.com/musistudio/claude-code-router), MIT, v3.1.1 released 2026-09-16, 37,486 stars (read 2026-09-30).
- Routing guide: [docs/src/content/docs/en/configuration/routing.md](https://github.com/musistudio/claude-code-router/blob/main/docs/src/content/docs/en/configuration/routing.md) (read 2026-09-30).
- Earlier 1.x releases documented scenario keys (`default`, `background`, `think`, `longContext`, `webSearch`); the npm page indexed on 2025-08-06 shows them. The current routing guide does not list them, and that older page was not re-read.

### Omnigent Smart Routing

Omnigent is a meta-harness that wraps Claude Code, Codex, Pi and YAML-defined
agents, runs on the user's machine and describes itself as alpha. Smart
Routing picks a harness and model from the first message of a session, with
either a built-in LLM judge or an external `routes:select` service. The choice
applies to the rest of the session. If the external service returns an error
or an invalid selection, the server surfaces the reason and falls back to the
session's default harness. Reasoning effort is not documented on the routing
page.

- Routing docs: [omnigent.ai/docs/build/routing](https://omnigent.ai/docs/build/routing) (read 2026-09-30).
- FAQ (license, alpha status, local): [omnigent.ai/faq](https://omnigent.ai/faq) (read 2026-09-30).
- Repository: [github.com/omnigent-ai/omnigent](https://github.com/omnigent-ai/omnigent), Apache-2.0, v0.16.0 released 2026-09-29 (read 2026-09-30).
- Agent Router already has an offline translation of the Omnigent routing contract ([integrations/omnigent.md](../integrations/omnigent.md), [README.md:380](../README.md)); see the [earlier comparison](omnigent-comparison.md).

### OpenRouter Auto Router

A hosted router selected with `model: "openrouter/auto"`. A lightweight
classifier assigns each prompt one of about 30 task types; candidates are
ranked by the community's share of spend on that task type over a trailing
7-day window, within a `cost_tier` band (`low` to `max`). Each turn is ranked
from scratch, with stickiness to the previous model while it stays a top
candidate. `allowed_models` and `excluded_models` restrict candidates and
`provider.max_price` caps a request. The response names the chosen model; an
opt-in header returns the task classification. No extra fee for the router.
The reasoning-tokens guide states that "dynamic router models
(`openrouter/auto`, `openrouter/free`) omit the `reasoning` field", so effort
cannot be set on the Auto Router. OpenRouter is reached through its own API
endpoint (`https://openrouter.ai/api/v1`), and the routing page says prompts
"are classified in-flight", on OpenRouter's side.

- Docs: [openrouter.ai/docs/guides/routing/routers/auto-router](https://openrouter.ai/docs/guides/routing/routers/auto-router) (read 2026-09-30). The routing unit follows its sentence "The router ranks candidates from scratch on each turn".
- Reasoning: [openrouter.ai/docs/guides/best-practices/reasoning-tokens](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens) (read 2026-09-30).
- Hosted API: [openrouter.ai/docs/quickstart](https://openrouter.ai/docs/quickstart), "OpenRouter gives you access to hundreds of AI models through a single API endpoint" (read 2026-09-30).

### LiteLLM Router and Auto Routing

LiteLLM's Router picks among deployments per request (weighted shuffle,
latency, usage, least-busy, cost-based or custom strategy) with retries,
cooldowns and fallbacks. Auto Routing, in beta since v1.94.x, maps each
request to a complexity tier (`SIMPLE`, `MEDIUM`, `COMPLEX`, `REASONING` or
custom tiers) using a heuristic scorer by default, or an LLM classifier,
keyword rules, embeddings or a Python plugin, with optional
`session_affinity`. Logs mark the cause of each decision (for example
`cause=complexity_scorer`). It runs as a self-hosted SDK or proxy.
Reasoning effort is set per model entry: the docs declare "one `model_list`
entry per rung, identical except for the reasoning parameter in its
`litellm_params`", with `reasoning_effort` (for example `low`, `high`,
`xhigh`) for OpenAI models and `thinking: {type: enabled, budget_tokens:
8192}` for Anthropic models, so a tier can land on a given effort rung.

- Router docs: [docs.litellm.ai/docs/routing](https://docs.litellm.ai/docs/routing) (read 2026-09-30).
- Auto Routing docs: [docs.litellm.ai/docs/proxy/auto_routing](https://docs.litellm.ai/docs/proxy/auto_routing) (read 2026-09-30).
- Repository: [github.com/BerriAI/litellm](https://github.com/BerriAI/litellm), MIT except the `enterprise/` directory ([LICENSE](https://github.com/BerriAI/litellm/blob/main/LICENSE)), v1.103.1 released 2026-09-30 (read 2026-09-30).

### vLLM Semantic Router

A self-hosted decision layer in front of model backends, deployed behind
Envoy (External Processing) on Docker or Kubernetes. Signals extract facts
from each request (keyword, embedding, domain, complexity, context length,
jailbreak, PII and others); projections combine them; decisions apply boolean
rules with priorities to choose an eligible candidate set; an algorithm picks
one model or a bounded multi-model path. The overview states "The choice is
written to `x-selected-model`". For reasoning, the current configuration page
only shows a provider-level default (`providers: defaults: reasoning_effort:
medium`); per-decision reasoning appears in the older v0.1 docs, which were
read only as a search excerpt and are not used in the table.

- Overview: [vllm-sr.ai/docs/overview/semantic-router-overview](https://vllm-sr.ai/docs/overview/semantic-router-overview/) (read 2026-09-30).
- Configuration: [vllm-sr.ai/docs/installation/configuration](https://vllm-sr.ai/docs/installation/configuration/) (read 2026-09-30); older v0.1 reasoning settings: [vllm-sr.ai/docs/v0.1/installation/configuration](https://vllm-sr.ai/docs/v0.1/installation/configuration/) (search excerpt only, 2026-09-30).
- Repository: [github.com/vllm-project/semantic-router](https://github.com/vllm-project/semantic-router), Apache-2.0, v0.4.0 released 2026-09-27 (read 2026-09-30).

### Plano (formerly Arch gateway)

An AI-native proxy and data plane for agent applications. It supports three
routing modes: explicit `provider/model`, aliases, and preference-aligned
routing where the Arch-Router model matches a request to user-described routes
(a name and a description such as "understand and explain existing code
snippets"). Each route lists a primary model and fallbacks, optionally
reordered by `cheapest` or `fastest`. Routing is per request; the
`X-Model-Affinity` header pins a session. It runs self-hosted, with Ollama or
vLLM serving the router model.

- LLM routing guide: [docs.planoai.dev/guides/llm_router.html](https://docs.planoai.dev/guides/llm_router.html) (read 2026-09-30).
- Repository: [github.com/katanemo/plano](https://github.com/katanemo/plano) (the `katanemo/archgw` URL redirects there), Apache-2.0, 0.4.37 released 2026-09-28 (read 2026-09-30).

### RouteLLM (LMSYS)

A framework for serving and evaluating learned routers between one strong and
one weak model. Four routers are provided (`mf` matrix factorization,
`sw_ranking`, `bert`, `causal_llm`); a request goes to the strong model when
its predicted win probability exceeds a calibrated threshold. It runs as a
Python client replacement or a local OpenAI-compatible server. It is the
reference research baseline for per-request cost/quality routing.

- Repository: [github.com/lm-sys/RouteLLM](https://github.com/lm-sys/RouteLLM), Apache-2.0, last push 2024-08-10 (read 2026-09-30).

### Not Diamond

A hosted routing service. The `/modelSelect` endpoint takes OpenAI-format
messages and the candidate providers, and returns a recommended model and a
`session_id` used to send feedback. It uses a pre-trained router or a custom
router trained on the user's evaluation data, with a `cost` or `latency`
tradeoff and a `cost_quality_tradeoff` value from 0 to 10. Whether the
service also proxies the model call is not documented on the pages read.

- Docs: [docs.notdiamond.ai/docs/what-is-model-routing](https://docs.notdiamond.ai/docs/what-is-model-routing.md) and the [/modelSelect reference](https://docs.notdiamond.ai/reference/token_model_select_v2_modelrouter_modelselect_post.md) (read 2026-09-30).
- The Python SDK repository [Not-Diamond/notdiamond-python](https://github.com/Not-Diamond/notdiamond-python) is archived, last push 2025-12-11, no license detected by GitHub (read 2026-09-30).

### Aider architect mode

Aider's architect mode sends each request first to the main model (the
architect), then to an editor model that turns the proposal into file edits.
The editor model comes from built-in defaults for the main model or from
`--editor-model`. The reasoning guide documents `--reasoning-effort` and
`--thinking-tokens` "to control the reasoning effort of models which support
this setting"; it does not distinguish the architect and editor roles. The split is fixed by configuration, not chosen per task.

- Docs: [aider.chat/docs/usage/modes.html](https://aider.chat/docs/usage/modes.html) and [aider.chat/docs/config/reasoning.html](https://aider.chat/docs/config/reasoning.html) (read 2026-09-30).
- Repository: [github.com/Aider-AI/aider](https://github.com/Aider-AI/aider), Apache-2.0, v0.86.0 released 2025-08-09, last push 2026-05-22 (read 2026-09-30).

### Host-native settings: Claude Code subagents and Codex profiles

Both hosts let the user fix a model and effort per role without any router.
A Claude Code subagent declares `model` (alias, full ID or `inherit`) and
`effort` (`low` to `max`) in its frontmatter, plus tool and permission
settings; Claude chooses which subagent to delegate to from the subagent's
`description`. Codex overlays a profile file with `--profile` and exposes
`model`, `model_reasoning_effort`, `plan_mode_reasoning_effort` and
`review_model`, and can point its provider at an external router with
`openai_base_url`.

- Claude Code subagents: [code.claude.com/docs/en/sub-agents](https://code.claude.com/docs/en/sub-agents) (read 2026-09-30). License: "© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service." ([LICENSE.md](https://github.com/anthropics/claude-code/blob/main/LICENSE.md), read with the GitHub contents API on 2026-09-30; GitHub detects no SPDX license for the repository).
- Codex: [advanced configuration](https://learn.chatgpt.com/docs/config-file/config-advanced) and [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference) (read 2026-09-30). Repository [github.com/openai/codex](https://github.com/openai/codex), Apache-2.0.

## Comparison table

| Project | Routing unit | How the choice is made | Where it runs | Reasoning effort | What the decision leaves behind | Run gate before execution | License |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Agent Router** | Each phase of a task (plan, execute, verify), decided before the run | Constraints filter first, then deterministic scoring; optional semantic proposals stay advisory inside the admissible pool | Local CLI, offline for routing | Chosen per phase with the model | Receipt, explanation, exclusions, missing evidence; same inputs replay to the same receipt | Approved digest, fresh capability evidence, verified OS sandbox, Git worktree for writes | Apache-2.0 |
| Kimchi Coding | Task delegation by role/phase during a session | LLM orchestrator selects from configured role pools | Local coding CLI; Kimchi Inference for model access | Per-role effort not documented in sources read | Session history; model/phase usage tags | Not established by sources read | Apache-2.0 (CLI); hosted service separate |
| claude-code-router | Each request through the gateway | User rules on headers and body, route scripts, subagent tag, ordered fallbacks | Local gateway | Not documented | Request logs: resolved provider, model, status, latency, tokens, estimated cost | Not applicable (proxy); client keys with request and token limits | MIT |
| Omnigent Smart Routing | Session, from the first message | LLM judge, or an external routing API | Omnigent server on the user's machine; the judge calls a configured LLM | Not documented | Error reason on router failure, then fallback to the default harness | Not documented on the routing page | Apache-2.0 |
| OpenRouter Auto Router | Each turn, with session stickiness | Task-type classifier ranked by community spend, within a cost tier | Hosted | Not available on `openrouter/auto` | Chosen model; opt-in task classification header | Not applicable (hosted API); per-request price cap | Hosted service |
| LiteLLM Auto Routing | Each request, optional session affinity | Complexity tiers from a heuristic scorer, LLM classifier, keywords, embeddings or plugin | Self-hosted SDK or proxy | Set per model entry (`reasoning_effort`, `thinking`) | Log line with the decision cause | Not applicable (proxy) | MIT, except `enterprise/` |
| vLLM Semantic Router | Each request | Signals, projections and prioritized boolean decisions, then a selection algorithm | Self-hosted behind Envoy, Docker or Kubernetes | Provider-level default (`reasoning_effort`) | `x-selected-model` header, observability | Not applicable (proxy); jailbreak and PII signals | Apache-2.0 |
| Plano | Each request, optional session pin | Explicit model, alias, or Arch-Router preference match on described routes | Self-hosted proxy | Not documented | Not documented on the page read | Not applicable (proxy) | Apache-2.0 |
| RouteLLM | Each request | Learned strong/weak router with a cost threshold | Self-hosted Python server | No | Not documented | Not applicable | Apache-2.0 |
| Not Diamond | Each request | Pre-trained or custom-trained router with a cost or latency tradeoff | Hosted API | Not documented | Recommended model and a `session_id` for feedback | Not applicable (recommendation) | Hosted service; SDK archived |
| Aider architect mode | Each request, two fixed roles | User configuration with defaults for the editor model | Local CLI | Not distinguished per role in the docs | Not applicable (fixed configuration) | Not documented on the pages read | Apache-2.0 |
| Claude Code subagents / Codex profiles | Per subagent (Claude Code), per profile or run (Codex) | User configuration; Claude Code picks the subagent from its description | Inside the host | Per subagent (`effort`), per profile and for plan mode (Codex) | The configuration files | The host's own permission and approval settings | Claude Code: all rights reserved, Commercial Terms; Codex: Apache-2.0 |

Agent Router cells cite [README.md:74](../README.md) (per phase),
[README.md:120](../README.md) (constraint-first), [README.md:121](../README.md)
(deterministic, replayable, semantic proposals advisory),
[README.md:122](../README.md) (approved digest, fresh evidence, OS sandbox),
[README.md:80](../README.md) (Git worktree for writes, patch preview),
[README.md:124](../README.md) (local and offline for routing) and
[README.md:479](../README.md) (license).

## What Agent Router does differently

These are design properties, not measured outcomes.

- **Phase as the routing unit.** Plan, execute and verify can each get a
  model, effort and skills ([README.md:74](../README.md)). Kimchi also uses
  roles and phases, with orchestration during the session; Agent Router
  decides before the run. Proxies above route one request at a time; Omnigent
  routes one session; Aider fixes two roles by configuration.
- **Hard constraints before scoring.** Permissions, budgets, risk and missing
  evidence remove candidates before any score, and "a higher score never makes
  an inadmissible candidate valid" ([README.md:120](../README.md)). vLLM
  Semantic Router also separates eligibility (decisions) from selection
  (algorithms), and OpenRouter honors `allowed_models` and
  `excluded_models` restrictions.
- **Replayable receipt.** The same inputs give the same decision, receipt and
  explanation, including exclusions ([README.md:121](../README.md)). The other
  projects document logs or headers naming the chosen model; none of the pages
  read documents a replayable decision record.
- **Approval bound to execution.** A run needs an approved digest, fresh
  capability evidence and a verified OS sandbox
  ([README.md:122](../README.md)). Proxies do not execute work, so this column
  mostly does not apply to them; host-native settings rely on the host's own
  permission system.
- **Unknowns stay unknown.** Unknown host capabilities, costs and identities
  are reported as `UNKNOWN` ([README.md:123](../README.md)).
- **No network needed to route.** Routing needs no provider call, daemon or
  network access ([README.md:124](../README.md)), once Cargo dependencies
  are available ([README.md:220](../README.md)). OpenRouter and Not Diamond
  decide on a hosted service; Omnigent's built-in judge and LiteLLM's LLM
  classifier call a model.
- **Cumulative session budgets.** Capacity and conservative cost/token
  allocations are reserved across runtime roots
  ([README.md:161](../README.md)). OpenRouter caps a single request;
  claude-code-router limits client keys.

## Where others are stronger

- **Maturity and adoption.** claude-code-router (MIT, 37,486 stars, v3.1.1),
  LiteLLM (59,927 stars, release on 2026-09-30) and vLLM Semantic Router have
  public releases and wide use. Agent Router's public release is "still gated
  by license and security review" ([README.md:195](../README.md)) and clone
  access is required while the repository is private
  ([README.md:205](../README.md)).
- **Routing inside a live session.** The proxies re-route every request as the
  conversation goes. Agent Router "does not switch the model of the
  conversation you are in: each approved phase runs as a separate host
  process" ([README.md:101](../README.md)).
- **Provider breadth and failover.** OpenRouter, LiteLLM, claude-code-router
  and Plano carry the model call, with retries and fallbacks across providers.
  Agent Router is not a proxy and "no provider transport is wired into
  execution today" ([README.md:124](../README.md)).
- **Learned or data-driven routing.** RouteLLM and Not Diamond train routers;
  OpenRouter ranks by observed community spend. Agent Router's human-labelled
  evaluation corpus is still open work ([README.md:84](../README.md)).
- **Serving infrastructure.** vLLM Semantic Router ships Envoy, Kubernetes
  and safety signals (jailbreak, PII) for production traffic.
- **Session product.** Omnigent adds shared sessions, web and mobile
  interfaces and multi-harness orchestration (see the
  [earlier comparison](omnigent-comparison.md)).
- **Platforms.** Agent Router has no Windows native execution
  ([README.md:383](../README.md)); claude-code-router ships a desktop app for
  macOS, Windows and Linux.
- **Licensing.** Most projects above are MIT or Apache-2.0. Agent Router is
  Apache-2.0 ([LICENSE.md](../LICENSE.md)).
- **Zero setup.** Claude Code subagents and Codex profiles need no extra tool.

## When to pick something else

- You want each request of an ongoing Claude Code session re-routed by simple
  rules or a script: claude-code-router.
- You want one hosted endpoint that picks a model and handles providers:
  OpenRouter Auto Router, or Not Diamond for a recommendation you then call.
- You run your own multi-model serving stack and need per-request routing with
  safety signals: vLLM Semantic Router, LiteLLM or Plano.
- You want a learned cost/quality threshold between two models: RouteLLM.
- A fixed planner/editor split is enough: Aider architect mode, or a Claude
  Code subagent or Codex profile with its own model and effort.
- You want a session product across harnesses with a UI: Omnigent.
- You want a coding agent that orchestrates role-specific models during a
  session: Kimchi Coding.

Agent Router fits when the routing decision itself must be checked before
anything runs: per phase, under explicit permissions and budgets, with a
replayable receipt and an approved, sandboxed run.

## Projects considered and dropped

- **Martian**: the current gateway docs ([docs.withmartian.com](https://docs.withmartian.com/), read 2026-09-30) show explicit model selection and integrations for Claude Code and Codex, but no automatic routing logic.
- **Portkey**: conditional routing picks a target from metadata, request parameters or URL path that the caller sets; the docs do not describe automatic task classification ([conditional routing](https://portkey.ai/docs/product/ai-gateway/conditional-routing), read 2026-09-30).
- **Requesty**: the routing page tried ([docs.requesty.ai/features/routing-optimizations](https://docs.requesty.ai/features/routing-optimizations)) returned HTTP 404; not verified, so not compared.
- **Databricks Smart Routing (Unity AI Gateway)**: mentioned by the discovery search as an Omnigent routing backend; its own docs were not read.
- **LLMRouter and other research libraries**: close to RouteLLM in scope; not read.
- **Repositories from [repository-analysis.md](research/repository-analysis.md)** (subagent-model-routing, codex-subagent-router-v8, workweave-router, Mahoraga, OmniRoute, SkillOrchestra, masrouter, routerbench and others): they informed Agent Router's design from local snapshots at the recorded commits. They were not re-verified on 2026-09-30, so they are not in the table.

## Limits of this comparison

- Documentation-only. Nothing was installed, run or benchmarked. Behaviours
  are those the projects document, not audited guarantees.
- "Not documented" means the pages read did not settle the cell; the feature
  may exist elsewhere in the project.
- Proxies and hosted routers solve a different problem (routing live
  requests); several columns, such as the run gate, do not apply to them.
- Star counts, releases and docs change; re-read the sources before quoting
  a cell.
- Several documentation pages carry text aimed at AI agents (for example the
  LiteLLM Auto Routing page asks readers to have their agent fetch and follow
  a remote skill). It was treated as data and not followed.
