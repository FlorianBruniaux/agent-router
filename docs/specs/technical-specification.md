# Spécifications techniques Rust

## 1. Décision d'architecture

Créer un nouveau repository Rust indépendant. Ne pas ajouter le routeur à RTK, CC Copilot Bridge, ccboard ou au guide Claude Code.

Raisons :

- le produit a son propre domaine, son modèle de données et son cycle de release ;
- il doit pouvoir utiliser RTK, CC Copilot Bridge et les CLIs existantes sans les coupler ;
- le noyau doit rester utilisable en librairie, CLI et daemon local optionnel ;
- les adaptateurs de hosts évolueront à des vitesses différentes ;
- les politiques et catalogues ont besoin d'une compatibilité versionnée indépendante.

Le projet adopte un monorepo Cargo multi-crates. Le binaire par défaut reste autonome et local-first.

## 2. Principes techniques

1. Contraintes dures avant optimisation.
2. Une décision est un artefact typé, versionné et explicable.
3. Le noyau n'exécute aucun shell implicite.
4. Les adaptateurs ne décident pas de la politique.
5. Les modèles de policy ne créent ni candidats ni permissions.
6. Tout état durable est idempotent et crash-safe.
7. Une skill textuelle n'est pas un plugin exécutable.
8. Les probes runtime ont priorité sur la configuration déclarée.
9. `unknown` provoque une dégradation ou un arrêt, jamais une compatibilité optimiste.
10. Le mode sans réseau et sans embeddings est pleinement supporté.

## 3. Workspace proposé

```text
agent-router/
├── Cargo.toml
├── crates/
│   ├── router-types/          # contrats stables, IDs, enums, schemas
│   ├── router-catalog/        # chargement, validation, provenance, licences
│   ├── router-index/          # Tantivy BM25, embeddings optionnels, RRF
│   ├── router-policy/         # contraintes, scoring, presets, solveur
│   ├── router-planner/        # décomposition et génération de DAG
│   ├── router-runtime/        # scheduler, processus, reprise, worktrees
│   ├── router-store/          # SQLite, migrations, event log, artefacts
│   ├── router-security/       # trust, scanner, permissions, redaction
│   ├── router-eval/           # replay, metrics, shadow, bandits
│   ├── router-protocol/       # JSON-RPC/HTTP local, version negotiation
│   ├── router-adapter-api/    # traits pour hosts, providers et transports
│   ├── adapter-codex/         # CLI, App capabilities, agents, tasks
│   ├── adapter-claude/        # Claude Code plugin, hooks, Agent/Workflow
│   ├── adapter-copilot/       # Copilot CLI plugin et transport
│   ├── adapter-opencode/      # OpenCode
│   ├── adapter-api/           # OpenAI/Anthropic protocol bridge
│   └── agent-router-cli/      # clap, sorties humaines et JSON
├── assets/
│   ├── catalogs/
│   ├── presets/
│   ├── schemas/
│   └── adapters/
├── integrations/
│   ├── codex-plugin/
│   ├── claude-plugin/
│   ├── copilot-plugin/
│   └── skills/
├── tests/
│   ├── fixtures/
│   ├── contract/
│   ├── integration/
│   └── replay/
├── docs/
│   ├── adr/
│   ├── architecture/
│   ├── adapters/
│   └── security/
└── THIRD_PARTY_NOTICES.md
```

Le MVP peut regrouper certains crates pour limiter le temps de compilation. Les frontières de types, policy, runtime, store et adapters doivent toutefois exister dès le début.

## 4. Dépendances Rust envisagées

| Domaine | Crate | Motif |
| --- | --- | --- |
| CLI | `clap` | commandes et sorties stables |
| Async | `tokio` | processus, IO, serveur local |
| Sérialisation | `serde`, `serde_json`, `toml`, `serde_yaml` | catalogues et protocoles |
| Erreurs | `thiserror`, `anyhow` au bord CLI | erreurs typées dans les libs |
| Stockage | `rusqlite` avec SQLite bundled | distribution simple et WAL |
| Recherche | `tantivy` | BM25, champs et index incrémental |
| HTTP | `axum`, `reqwest`, `tower` | daemon et adaptateurs API |
| Streaming | `futures`, `tokio-stream`, `bytes` | events et provider streams |
| Observabilité | `tracing`, `tracing-subscriber` | traces structurées |
| Hash | `sha2` | provenance et intégrité |
| IDs | `uuid` ou ULID | traces et tâches |
| Fichiers | `tempfile`, `walkdir`, `ignore` | inventaire et staging sûr |
| Validation | `jsonschema` ou schemas générés | contrats externes |
| Embeddings optionnels | `ort` ou `fastembed` derrière feature | reranking local |
| Tests | `proptest`, `insta`, `assert_cmd` | invariants et golden tests |

Règles : dépendances minimales, features Cargo explicites, SBOM, audit de supply chain, pas de moteur Python obligatoire au runtime.

## 5. Modèle de domaine

### Identifiants

Tous les IDs sont opaques, stables et namespacés :

```rust
pub struct ModelId(pub String);
pub struct AgentId(pub String);
pub struct SkillId(pub String);
pub struct PromptId(pub String);
pub struct ToolId(pub String);
pub struct HarnessId(pub String);
pub struct PolicyId(pub String);
pub struct TaskId(pub Uuid);
pub struct RunId(pub Uuid);
```

Les alias sont résolus à l'entrée. La décision et le store conservent l'ID canonique et l'alias demandé.

### TaskContext

```rust
pub struct TaskContext {
    pub objective: String,
    pub task_kind: TaskKind,
    pub risk: RiskProfile,
    pub scope: Scope,
    pub allowed_mutations: Vec<MutationClass>,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub budget: Budget,
    pub deadline: Option<DateTime<Utc>>,
    pub data_policy: DataPolicy,
    pub host_snapshot: HostSnapshotId,
    pub assumptions: Vec<Assumption>,
}
```

### Catalog assets

Chaque asset partage :

- `id`, `version`, `schema_version` ;
- `source`, `commit`, `license`, `authors` ;
- `sha256`, `signature`, `trust_tier` ;
- `capabilities`, `requirements`, `restrictions` ;
- `valid_from`, `valid_until`, `deprecated_by` ;
- `examples`, `triggers`, `anti_triggers` ;
- `context_cost_estimate`.

`SkillSpec` ajoute `kind: Instruction | Executable`, permissions et outils requis. `AgentSpec` ajoute rôle, sandbox, output contract et délégation permise. `ModelSpec` ajoute modalities, tools, reasoning efforts, context, coût, provider et cache semantics.

### ExecutionPlan

```rust
pub struct ExecutionPlan {
    pub id: PlanId,
    pub policy: PolicyRef,
    pub steps: Vec<PlanStep>,
    pub edges: Vec<Dependency>,
    pub estimated: PlanEstimate,
    pub required_approvals: Vec<ApprovalGate>,
    pub fallback: Vec<FallbackPlan>,
    pub explanation: DecisionTrace,
}

pub struct PlanStep {
    pub task: TaskSlice,
    pub harness: HarnessId,
    pub transport: TransportKind,
    pub agent: AgentBinding,
    pub model: ModelId,
    pub effort: EffortLevel,
    pub skills: Vec<SkillBinding>,
    pub prompts: Vec<PromptBinding>,
    pub tools: Vec<ToolBinding>,
    pub sandbox: SandboxPolicy,
    pub verification: Vec<VerificationSpec>,
    pub retry: RetryPolicy,
    pub timeout: Duration,
}
```

### États runtime

```text
planned -> awaiting_approval -> queued -> preparing -> running
running -> succeeded_transport -> verifying -> verified
running -> failed | timed_out | cancelled
verifying -> verification_failed | verified
queued -> blocked | cancelled
```

Seuls `verified`, `failed`, `verification_failed`, `timed_out`, `cancelled` et `blocked` sont terminaux. Une transition invalide échoue et crée un événement d'intégrité.

## 6. Pipeline de décision

### Étape A : normalisation

- parse objectif, contraintes et overrides ;
- construit le snapshot du host ;
- classe task kind, risque, réversibilité et forme de preuve ;
- transforme les chemins en identités canoniques ;
- retire secrets et payloads non nécessaires des features de routing.

La classification initiale est déterministe et règle-based. Un classifieur LLM peut ajouter des signaux, jamais réduire le risque déterministe.

### Étape B : décomposition

Le planificateur propose zéro à N étapes. Les sorties LLM sont validées par un schema strict, normalisées, bornées et rejetées si elles étendent le scope.

Une version déterministe couvre les presets connus. Le LLM n'est nécessaire que pour les objectifs complexes.

### Étape C : récupération

Index Tantivy avec champs :

| Champ | Boost initial |
| --- | --- |
| identifiant et nom exact | 8.0 |
| task kinds | 6.0 |
| triggers | 5.0 |
| description | 3.0 |
| exemples | 2.5 |
| tags et langages | 2.0 |
| chemins | 2.0 |
| anti-triggers | exclusion ou pénalité forte |

BM25 produit une liste par type d'asset. Le query builder conserve phrases, symboles, chemins et termes techniques au lieu de tout réduire en mots isolés.

L'option embeddings calcule une liste sémantique. La fusion RRF utilise par défaut :

```text
rrf(document) = sum(weight_r / (k + rank_r(document)))
```

avec `k = 60`, poids configurables et déduplication par ID canonique. Les top-k restent assez larges pour éviter qu'un retriever médiocre supprime le bon candidat.

### Étape D : contraintes

Chaque contrainte implémente :

```rust
trait Constraint {
    fn evaluate(&self, ctx: &DecisionContext, candidate: &CandidatePlan)
        -> ConstraintVerdict;
}
```

`ConstraintVerdict` vaut `Eligible`, `Excluded { code, evidence }` ou `Unknown { code }`. La politique définit si un unknown particulier exclut, demande une approbation ou permet une dégradation.

Les contraintes principales sont host, auth, capability, tools, modality, effort, context, data, sandbox, network, budget, concurrency, recursion, cache compatibility, licence et trust.

### Étape E : génération de plans

Le moteur ne calcule pas le produit cartésien complet. Il utilise :

1. top-k par type ;
2. jointures par compatibilité déclarée ;
3. beam search borné ;
4. dominance pruning ;
5. limites de diversité par famille de modèle et transport.

Le beam state contient les décisions des étapes déjà assignées, coût, latence, contexte, risque résiduel et violations.

### Étape F : scoring

Score initial :

```text
utility =
  wq * expected_quality
  + wr * retrieval_relevance
  + wh * historical_fit
  + wc * cache_affinity
  - wcost * normalized_cost
  - wlat * normalized_latency
  - wctx * context_pressure
  - wrisk * residual_risk
  - wcoord * coordination_overhead
  - wswap * model_switch_penalty
```

Les valeurs sont normalisées par classe de tâche. Les poids appartiennent au preset et à la politique d'organisation. Un score ne compense jamais une exclusion dure.

Le `expected_quality` cold start vient du catalogue. Avec feedback, il vient d'une estimation calibrée et conserve une incertitude. Un bonus d'exploration est autorisé uniquement dans les modes low-risk et shadow/canary.

### Étape G : décision et trace

La sortie inclut : snapshot ID, policy ID, candidats, exclusions, scores, estimate, plan, alternatives et confiance. Le hash de décision couvre les entrées redacted et les versions de catalogues.

## 7. Policy engine

### Politique déterministe

Format TOML ou YAML validé :

```toml
[policy]
id = "builtin/default-v1"
mode = "stable"

[weights]
quality = 1.0
cost = 0.25
latency = 0.15
risk = 1.0
context = 0.20

[limits]
max_steps = 12
max_depth = 2
max_concurrency = 4
max_candidates_per_type = 20

[fallback]
on_policy_error = "deterministic-safe"
on_high_risk_unknown = "require-review"
```

### Sidecar optionnel

Contrat inspiré des bonnes frontières observées, réimplémenté sans code WorkWeave :

- `GET /v1/capabilities` ;
- `POST /v1/rank` avec contexte redacted et candidats immuables ;
- `POST /v1/outcome` ;
- `POST /v1/feedback` ;
- artifact digest et schema version obligatoires ;
- délai court et retry borné ;
- aucune permission filesystem ou workspace ;
- aucune capacité à ajouter candidat, tool ou permission ;
- échec fermé pour une politique explicitement requise, fallback visible sinon.

Le sidecar peut être Python pour la recherche. Le produit et le MVP ne doivent pas en dépendre.

## 8. Apprentissage et évaluation

### Stockage statistique

Par contexte discret ou embeddings de contexte :

- Beta posterior de réussite ;
- moyenne et variance de qualité ;
- distributions coût, tokens et latence ;
- classes d'échec ;
- propension de la décision ;
- feedback humain séparé ;
- timestamp pour drift et decay.

### Bandit

Le premier algorithme online recommandé est Thompson Sampling contextualisé sur une petite liste de plans, avec exploration désactivée pour high-risk. LinUCB peut servir de baseline. Les modèles plus complexes attendent assez de données.

### Garde d'activation

Une policy candidate passe :

1. validation de schema ;
2. replay sur corpus figé ;
3. comparaison aux baselines ;
4. tests de sous-groupes ;
5. shadow local ;
6. canary borné ;
7. activation signée ;
8. surveillance de drift et rollback.

### Métriques

- success@budget ;
- quality-cost frontier ;
- regret ;
- calibration Brier/ECE ;
- precision@k et recall@k des assets ;
- route stability ;
- constraint violation rate ;
- fallback rate ;
- transport failure rate ;
- humain override rate ;
- p50/p95 de décision et exécution.

## 9. Runtime et scheduler

### Processus

- `tokio::process::Command` avec argv, jamais `sh -c` par défaut ;
- nouveau process group sur Unix ;
- SIGTERM puis SIGKILL borné ;
- stdout et stderr streamés et capturés ;
- limites de bytes ;
- UTF-8 tolérant et payload brut disponible comme artefact ;
- code de sortie conservé ;
- classification des erreurs auth, rate limit, timeout, crash, network, policy et verification.

### Concurrence

Sémaphores globaux, par provider et par agent logique. Un scheduler équitable évite qu'un workflow volumineux bloque toutes les petites tâches. Les raisons de queue sont typées et visibles.

### DAG

- validation acyclique ;
- états atomiques par étape ;
- contenu des prompts figé par hash ;
- reprise refuse une définition modifiée sauf migration explicite ;
- retries avec idempotency key ;
- dépendances transmettent uniquement les artefacts autorisés ;
- taille maximale de contexte transmis ;
- fail-fast ou continue configuré par workflow.

### Worktrees

Le runtime crée un worktree seulement sur plan approuvé. Il conserve base commit, branche, scope de fichiers et diff. `apply`, `discard` et `keep` sont explicites. Aucun reset, stash ou staging global.

## 10. Stockage

SQLite WAL, foreign keys et migrations monotones.

Tables initiales :

```text
catalog_snapshots
catalog_assets
host_snapshots
tasks
plans
plan_steps
decisions
candidate_scores
constraint_verdicts
runs
run_attempts
events
artifacts
verifications
outcomes
feedback
policy_artifacts
skill_competence
performance_stats
```

Les événements sont append-only. Les tables matérialisées accélèrent les requêtes mais peuvent être reconstruites. Les writes critiques utilisent transactions et fichiers temporaires atomiquement renommés pour les artefacts.

Rétention : prompts et payloads complets désactivés par défaut, métriques redacted conservées, purge configurable, export et delete complets.

## 11. Adaptateurs

### Traits

```rust
#[async_trait]
pub trait HarnessAdapter: Send + Sync {
    async fn probe(&self) -> Result<HostSnapshot, AdapterError>;
    async fn prepare(&self, step: &PlanStep) -> Result<PreparedRun, AdapterError>;
    async fn execute(&self, run: PreparedRun, sink: EventSink)
        -> Result<TransportOutcome, AdapterError>;
    async fn verify_runtime(&self, run: &RunRef)
        -> Result<RuntimeEvidence, AdapterError>;
    async fn cancel(&self, run: &RunRef) -> Result<(), AdapterError>;
}
```

Un `ProviderAdapter` normalise les APIs et streams. Un `HarnessAdapter` possède workspace, agents, skills et tool loop. Les deux concepts ne sont pas confondus.

### Codex

Ordre d'intégration :

1. agents natifs avec modèle et effort explicites ;
2. plugin et skills pour déclencher le binaire ;
3. instructions AGENTS.md générées et bornées ;
4. `codex exec` pour un worker externe supervisé ;
5. handoff de tâche seulement si l'API et l'autorité utilisateur le permettent.

Le probe vérifie la capacité effective, pas seulement la présence de clés de config. L'adaptateur respecte les limites App sur rôle et fork et compacte le contexte nécessaire dans le task body.

### Claude Code

Ordre : Agent/Workflow natif, plugin, hooks, bridge API, shim CLI. Les hooks servent aux gates, à la télémétrie et aux contrôles, pas à contenir la logique complète de routage.

### Copilot et OpenCode

Plugins si disponibles, sinon shims supervisés. Les receipts transport sont signés par le runtime local et couplés à un run ID. La sortie de l'enfant ne peut pas forger seule le statut du parent.

### API proxy

Reprendre la frontière de CC Copilot Bridge : providers derrière un trait, contexte hors payload, traduction par provider, streaming normalisé, erreurs client sans fuite. Ajouter une route envelope interne signée au lieu d'un tag textuel libre.

## 12. Plugins, skills et hooks

### Skill universelle

Une skill par host explique :

- quand appeler `agent-router plan` ou `run` ;
- comment présenter l'approbation ;
- comment charger les skills sélectionnées ;
- comment appliquer la route si le host garde l'exécution ;
- comment renvoyer outcome et feedback.

Elle reste courte et ne duplique pas le catalogue.

### Hooks

Hooks candidats :

- SessionStart : probe léger et résumé de capacités, cache court ;
- PreToolUse / PreAgent : valider envelope, scope et approval ;
- PostToolUse : événement d'outcome technique ;
- Stop : rappeler une vérification manquante une fois ;
- SessionEnd : nettoyage des processus et leases.

Chaque host ayant des événements différents, les hooks sont générés depuis un contrat commun puis testés séparément.

### Instructions

Les instructions ne doivent contenir que la politique nécessaire au parent : déclencheurs, frontière d'autorité, commande du binaire et format de résultat. Les détails résident dans les assets chargés à la demande.

## 13. Sécurité

### Menaces

- skill ou prompt empoisonné ;
- exécution de script tiers ;
- exfiltration de secrets par provider ou hook ;
- path traversal et symlink escape ;
- confusion entre projet et home ;
- modèle ou agent inventé ;
- fallback permissif ;
- forging de receipt ;
- prompt retenu dans logs ;
- concurrence qui écrase une décision plus récente ;
- policy sidecar compromis ;
- cache contenant une route devenue incompatible.

### Contrôles

- canonicalisation et allowlist des chemins ;
- `openat` ou vérification équivalente contre les symlinks pour writes sensibles ;
- permissions privées ;
- secrets uniquement via handles/env filtré, jamais dans la decision trace ;
- manifests hashés et éventuellement signés ;
- scanner statique multi-niveaux avec verdict humain pour ambiguïtés ;
- pas de réseau pour skills exécutables par défaut ;
- sandbox processus, limites CPU/mémoire/PID ;
- cache key incluant host, versions, policy, catalogue, risque et hash de tâche redacted ;
- high-risk fail closed ;
- receipts composés par le parent depuis code de sortie et métadonnées, pas parsés naïvement dans stdout ;
- redaction testée sur fixtures de secrets.

Les scanners regex sont des détecteurs, pas des preuves de sûreté. Le rapport doit conserver cette limite.

## 14. Configuration et précédence

```text
builtin defaults
  < user config
  < project config
  < organization policy restrictive
  < explicit task override
  < approval-bound temporary override
```

Les politiques restrictives utilisent une fusion monotone : réduction de permissions, budgets ou candidats. Les fichiers gérés portent markers et hashes. Toute modification globale suit inventaire, candidat inerte, diff, approbation liée au digest, revalidation des preimages, écriture atomique et rollback.

## 15. CLI

```text
agent-router inspect [--host auto] [--json]
agent-router catalog list|show|validate|refresh
agent-router route --task <text|file> [--preset id] [--json]
agent-router plan --task <text|file> [--dry-run]
agent-router run <plan-id> [--approve <digest>]
agent-router explain <decision-id>
agent-router status <run-id>
agent-router cancel <run-id>
agent-router workflow list|show|resume|cancel
agent-router artifacts list|show|apply|discard
agent-router feedback <run-id> --verdict ...
agent-router replay <dataset> --policy <id>
agent-router policy shadow|activate|rollback
agent-router doctor [--deep]
```

Toutes les commandes ont une sortie JSON versionnée. Les sorties humaines sont compactes et indiquent les chemins d'artefacts au lieu d'imprimer les logs complets.

## 16. API locale optionnelle

Transport Unix socket par défaut, loopback TCP optionnel. JSON-RPC ou HTTP versionné. Auth par token local créé 0600, protection CSRF pour UI éventuelle, aucune écoute réseau externe sans configuration explicite.

Endpoints : capabilities, route, plan, execute, events, outcome, feedback, artifacts et health. SSE pour événements. Le daemon n'est pas requis pour les commandes one-shot.

## 17. Tests

### Unitaires

- parsing et canonicalisation ;
- transitions d'état ;
- chaque contrainte ;
- BM25, fusion RRF et dominance ;
- calcul des scores ;
- redaction ;
- policy precedence ;
- licence et trust.

### Property based

- aucune route inéligible ne gagne ;
- une restriction supplémentaire n'élargit jamais l'ensemble éligible ;
- reprise ne réexécute pas une étape vérifiée ;
- sérialisation roundtrip stable ;
- DAG toujours acyclique après normalisation ;
- cache invalidé sur changement de snapshot.

### Contract tests

Un fake host et un fake provider caractérisent streaming, timeouts, signaux, receipts, cancellation, retries et effort. Chaque adaptateur réel possède un corpus versionné de payloads et métadonnées.

### Sécurité

Fixtures traversal, symlink, prompt injection, secret, forged receipt, script hostile, hook timeout, policy sidecar compromis et cache poisoning.

### E2E

- plan Sol, exécution Terra, revue indépendante dans Codex ;
- plan Opus, exécution Sonnet dans Claude Code ;
- cross-harness par shim ;
- workflow interrompu et repris ;
- worktree appliqué explicitement ;
- host sans subagents qui dégrade vers plan manuel.

Les tests live sont séparés, coûteux et opt-in. Un test structurel ne permet pas de revendiquer un comportement runtime.

## 18. CI, qualité et release

- `cargo fmt --check` ;
- `cargo clippy --all-targets --all-features -- -D warnings` ;
- `cargo test --workspace` ;
- MSRV ;
- audit dépendances et licences ;
- deny `unsafe_code` ;
- fuzz des parsers externes ;
- tests macOS/Linux, Windows à partir de phase 3 ;
- reproductibilité des catalogues et schemas ;
- SBOM et checksums de binaries ;
- release LTO, strip et codegen-units 1 après mesure ;
- Homebrew et archives signées.

## 19. Roadmap

### Phase 0 : cadrage et contrats, 1 à 2 semaines

Livrables : ADRs, schemas, corpus de 100 fixtures, matrice de hosts, modèle de licence, baseline manuelle.

Gate : les décisions attendues des fixtures sont validées humainement. Aucun code d'intégration globale.

### Phase 1 : noyau déterministe et CLI, 3 à 4 semaines

Types, catalogues, probes passifs, contraintes, score cold start, trace, `inspect`, `route`, `explain`, SQLite minimal.

Gate : zéro violation de contrainte, déterminisme golden, p95 inférieur à 100 ms.

### Phase 2 : récupération hybride, 2 à 3 semaines

Tantivy BM25, index incrémental, RRF, progressive disclosure, eval precision/recall. Embeddings derrière feature flag.

Gate : rappel@5 convenu sur corpus, index invalidé correctement, aucun corps de skill chargé avant sélection.

### Phase 3 : runtime durable, 3 à 5 semaines

Scheduler, processus, events, timeouts, cancellation, receipts, artefacts, DAG, reprise et worktrees.

Gate : crash/restart E2E, aucun double run, intégration worktree explicite.

### Phase 4 : Codex et Claude Code, 3 à 5 semaines

Plugins, skills, hooks, agents natifs, shims et matrices de probes. Preset `plan-execute-verify` complet.

Gate : traces runtime prouvent les modèles et efforts ou rapportent unknown. CLI et App testées séparément.

### Phase 5 : autres harnesses et API bridge, 3 à 4 semaines

Copilot, OpenCode, APIs compatibles, provider trait et translation.

Gate : contract suite identique et fallbacks visibles.

### Phase 6 : feedback et policy learning, 4 à 6 semaines

Outcomes, Beta competence, replay, shadow, bandit, drift, quarantine et rollback.

Gate : aucune activation sans amélioration statistique et sous-groupes non régressifs.

### Phase 7 : UX et écosystème

TUI ou web local, marketplace signé, packs, documentation et importers. Cette phase vient après la stabilité des contrats.

## 20. Risques de projet

| Risque | Impact | Réponse |
| --- | --- | --- |
| APIs de hosts instables | adaptateurs cassés | probes versionnés, contract tests, dégradation |
| Trop de dimensions de score | politique incompréhensible | contraintes d'abord, poids peu nombreux, explain |
| Peu de feedback réel | apprentissage trompeur | cold start déterministe, minimum samples, shadow |
| Explosion combinatoire | latence | top-k, beam search, pruning |
| Skills tiers hostiles | secrets ou writes | trust tiers, scanner, sandbox, approbation |
| Monolithe | maintenance lente | crates et traits stables, scope MVP strict |
| Promesses produit supérieures aux preuves | confiance perdue | runtime evidence, unknown explicite |
| Licence de sources | impossibilité de distribuer | clean room, notices, refus sans licence |
| Changement global de config | casse utilisateur | installation digest-approved et réversible |

## 21. Premier incrément recommandé

Ne pas commencer par un daemon, un bandit ou un plugin complet. Commencer par :

1. `router-types` ;
2. catalogues de dix modèles, dix agents, vingt skills et quatre hosts ;
3. Tantivy BM25 ;
4. contraintes dures ;
5. preset `plan-execute-verify` ;
6. `route --json` et `explain` ;
7. 100 fixtures humaines ;
8. adaptateur Codex en dry-run.

Cet incrément valide la proposition centrale, le choix joint et l'explicabilité, avant de payer le coût du runtime multi-harness.
