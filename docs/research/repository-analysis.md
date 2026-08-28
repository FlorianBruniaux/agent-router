# Analyse comparative des repositories

## Résultat

Aucun projet analysé ne couvre seul le besoin complet. Les briques existent séparément : transport inter-harness, sélection modèle et effort, orchestration durable, récupération de skills, sélection d'agents, sécurité des extensions, apprentissage par feedback et intégration native aux clients.

La bonne synthèse est un nouveau plan de contrôle universel en Rust avec quatre frontières :

1. un noyau déterministe qui normalise, filtre et explique ;
2. un moteur de récupération hybride pour agents, skills, prompts et outils ;
3. un solveur qui choisit un plan complet sous contraintes ;
4. des adaptateurs minces pour Codex, Claude Code, Copilot, OpenCode et les APIs compatibles.

BM25 est pertinent pour générer des candidats, pas pour prendre seul la décision finale. Une correspondance lexicale élevée ne prouve ni la capacité d'un modèle, ni la disponibilité d'un outil, ni la sûreté d'un skill, ni la qualité attendue d'un agent.

## État du corpus local

Les vingt clones sont propres au moment de l'analyse. Les commits ci-dessous constituent le snapshot étudié.

| Repository | Commit | Licence observée | Nature | Apport principal | Limite structurante |
| --- | --- | --- | --- | --- | --- |
| AgenticX | `fd61743f55e5e09de4924eb261ffb5e8f31256d2` | Apache-2.0 | Framework généraliste | Registry/versioning de skills, guard, collaboration et suivi d'usage | Surface énorme, beaucoup de sous-systèmes indépendants du besoin |
| agent-arche | `3124c9d9fdd159a471bcf3cd467dc7b2fa14f474` | MIT | Installateur Codex | Installation sélective, contexte budgété, profils de skills | Ranking lexical artisanal, routage largement confié au modèle |
| busytok | `499666bffa2e75574489c658f6faca1e4c18aea4` | Apache-2.0 | Runtime Rust | Identité logique persistante, file d'attente, annulation, SQLite, sidecars | Modèle et provider choisis explicitement, pas de sélection jointe automatique |
| subagent-model-routing | `2cdacdefed82eed521a0eae15f29552a6b8e1a2c` | MIT | Runtime Python et plugins | Transport inter-harness, workflow DAG, worktrees, reçus vérifiables | Python, décision de route surtout doctrinale ou explicite |
| codex-orchestrator | `22067c88f051ef8685ec826b5f54737deb13414f` | MIT | Plugin et hooks | Séparation chair, workers, escalade, ledger et fresh-eyes review | Politique prescriptive, forte dépendance aux conventions du host |
| codex-sol-terra-orchestration | `6ee3c15714b5f8d695b1a7aaa2178364d1c67346` | MIT | Overlay de configuration | Modèle parent, feuille et coordinateur, preuves runtime, limites Codex | Configuration statique, ne route pas skills, prompts et autres providers |
| codex-subagent-pack | `25875de868337dbbca34f3c06b855637f41ceacc` | MIT | Générateur de packs | Catalogues typés, précédence, validation sécurité, manifest et doctor | Résolution à l'installation, pas de routeur runtime universel |
| codex-subagent-router-v8 | `018d1675ed376758ce22f34ca51b5d9c7f1a70de` | MIT avec contenu tiers | Routeur Codex JavaScript | Score agent, skills par phase, risque, modèle, effort, plan et fallback | Monolithe, heuristiques lexicales, transport effectué par le parent |
| semantic-router | `88f06d0ea02c7f7d6435b82b604bb3a4cb759f93` | Apache-2.0 | Infrastructure de routage | Séparation Signals, Projections, Decisions, Algorithms, Model Pool | Très gros système orienté requêtes modèles, pas orchestration native de harnesses |
| weft | `b91b9c7a6cf30bf54abf36b479eaafff4100d8e8` | Apache-2.0 | Runtime Rust et packages | Registry de capabilities, skills textuels, packages exécutables, WASM | Preview instable, routage provider simple, recherche de skill immature |
| workweave-router | `782e4b93623f57c39e80a63b5120f308c2557ca9` | Elastic-2.0 | Proxy Go et sidecar Python | Contraintes de traduction, effort, cache, planner EV, bandit, contrat sidecar | Licence source-available, aucune copie de code dans un produit dérivé |
| Mahoraga | `d65841329df517be8dff4ef1ec730501310e25d8` | MIT | Routeur Python | Bandits contextuels, budget pacing, drift, quarantaine, mémoire épisodique | Classifieur de complexité et fallback très simples, peu de bras actifs |
| OmniRoute | `a94fe23e8923dd59426d599a5d88ba2581f23373` | MIT avec dépendances tierces | Passerelle TypeScript | Backends, stratégies, skills exécutables et statiques, sandbox, eval routing | Monolithe produit, score de skill lexical et surface de sécurité large |
| claude-code-router | `aec22a00cc9f934b8ab793522731cf1c71864d39` | MIT | Proxy TypeScript | Injection de descriptions d'agents, tag de modèle, rewrites et fallbacks | Les scripts de règles sont trop puissants, le tag généré par LLM n'est pas une preuve |
| SkillOrchestra | `be0603bc29f4656a1c301c2a5d78dc6f1d7a5bd4` | Apache-2.0 | Code de recherche | Taxonomie de skills apprise, compétence Beta par agent, coût, Pareto | Pipeline offline lourd et compétence limitée aux traces du benchmark |
| masrouter | `e005f7696fe0c0412563f6cd67f4cd3712fa2822` | Apache-2.0 | Code de recherche | Sélection jointe tâche, nombre d'agents, rôles, LLM et topologie | Modèle entraîné spécialisé, pas un runtime de production ni un catalogue dynamique |
| RouteLLM | `0b64fdafe049e596a3f5657c219329f24af24198` | Apache-2.0 | Routeur de recherche | Win-rate fort/faible, seuil calibrable, API compatible OpenAI | Routage binaire sur le dernier tour, pas de skills, agents ou efforts |
| routerbench | `cc67d1008bd8f3cf1e8040cc3ba4034d31b93c0c` | MIT | Banc d'évaluation | Performance-coût, willingness-to-pay, KNN, SVM, MLP, cascade | Benchmark ancien et schémas réduits à un nom de modèle |
| Route-To-Reason | `d34b9180eef287b12c7008e9a77d39c9617c24f8` | Pas de licence | Prototype de recherche | Prédiction conjointe réussite et tokens pour modèle et stratégie | Code monofichier fragile, fonctions dupliquées, pas réutilisable juridiquement |
| Ares | `49a33926c3cfe28cfd7fc142f65727e126be5341` | Pas de licence | Placeholder | Sujet directement pertinent, sélection adaptative d'effort | README d'une ligne, aucune implémentation publique à analyser |

## Analyse des patterns à reprendre

### 1. Un binaire comme frontière d'exécution

Busytok et subagent-model-routing démontrent la différence entre une skill et un runtime. Une skill peut expliquer quand déléguer. Elle ne doit pas posséder les processus enfants, l'annulation, les timeouts, les worktrees, les journaux, la reprise ou l'idempotence.

Décision : le binaire Rust possède la décision, l'exécution, l'état et les preuves. Les skills ne font que déclencher `route`, `plan`, `run`, `explain` ou `feedback`.

### 2. Contraintes avant score

WorkWeave et Semantic Router traitent correctement la compatibilité comme un filtre dur. Les besoins de tools, signatures de raisonnement, multimodalité, citations, structured output, cache et format du client doivent éliminer un candidat avant tout calcul coût-qualité.

Décision : le moteur retourne les candidats exclus avec un code de raison. Il ne transforme jamais une incompatibilité en simple pénalité rattrapable par un score élevé.

### 3. Séparer faits, politiques et algorithmes

La chaîne de Semantic Router est la base conceptuelle la plus propre : signal, projection, décision, algorithme, pool. WorkWeave complète cette chaîne avec un contrat sidecar où le routeur conserve les candidats et règles faisant autorité.

Décision : un classifieur ou un bandit ne reçoit jamais le droit d'inventer un modèle, un skill ou une autorisation. Il classe uniquement une liste déjà éligible et versionnée.

### 4. Résoudre un plan, pas une liste d'éléments

MasRouter et Route-To-Reason montrent l'intérêt d'une sélection jointe. Choisir séparément le meilleur modèle, puis le meilleur agent, puis le meilleur skill produit souvent une combinaison incohérente.

Décision : chaque candidat est un `ExecutionPlan` comprenant étapes, dépendances, harness, agent, modèle, effort, skills, outils, sandbox, budget et vérifications. Le solveur compare des plans complets.

### 5. Récupération hybride et progressive disclosure

Agent Arche limite le coût initial et garde les corps de skills hors contexte. Son ranking exact-termes est utile pour le bootstrap mais reste inférieur à BM25. OmniRoute, Weft et Sinan confirment que les scores lexicaux artisanaux deviennent vite difficiles à calibrer.

Décision : index Tantivy BM25 par champs, embeddings optionnels locaux, fusion RRF, puis reranking sous contraintes. Le routeur injecte les métadonnées de candidats avant de charger les corps des skills sélectionnées.

### 6. Compétence par skill, pas réputation globale

SkillOrchestra conserve une distribution Beta par couple agent-skill et apprend depuis des trajectoires contrastives. C'est plus utile qu'une note unique par agent.

Décision : les observations sont attachées à `agent × skill × task_kind × harness × model × effort`. Le cold start utilise des priors déclaratifs et les résultats réels ne deviennent actifs qu'après un volume minimal.

### 7. Apprentissage contrôlé et observable

Mahoraga apporte le bandit, la propension, le budget pacing, la détection de drift et la quarantaine. WorkWeave formalise `/outcome`, `/feedback`, le shadow mode et l'identité immuable de l'artefact de politique.

Décision : aucune politique apprise n'est activée directement. Elle passe par replay offline, shadow mode, comparaison à la politique stable, seuils de régression, canary local et rollback.

### 8. Adaptateurs natifs et dégradation explicite

CCR injecte les descriptions de modèles dans les outils Agent/Task, puis récupère un tag dans la requête dérivée. Codex Subagent Pack génère des agents et des instructions compréhensibles par le host. Subagent-model-routing utilise des shims entre CLIs. Ces mécanismes couvrent des niveaux d'intégration différents.

Décision : chaque adaptateur déclare ses capacités runtime. La dégradation suit l'ordre : spawn natif typé, plugin ou hook, proxy API, shim CLI supervisé, instruction manuelle. Le résultat indique toujours la forme réellement utilisée.

### 9. Identité, états et reprise

Busytok fournit la meilleure base pour l'identité logique, les états hot/warm/cold, les files et les erreurs typées. Subagent-model-routing ajoute DAG, reprise, worktree isolé et receipts.

Décision : persistance SQLite WAL, journal append-only, clés d'idempotence, transitions d'état validées et artefacts privés. Une sortie transport réussie n'est jamais assimilée à une tâche fonctionnellement réussie.

### 10. Sécurité des skills et extensions

AgenticX, OmniRoute et Codex Subagent Pack mettent en évidence trois classes de risques : prompt empoisonné, code exécutable et configuration d'accès. Les regex seules ne prouvent pas l'absence de risque.

Décision : provenance, hash, version, licence, trust tier, permissions déclarées, scanner statique, approbation pour code exécutable, sandbox et allowlists de chemins/réseau. Une skill textuelle et un package exécutable sont deux types différents.

## Patterns à ne pas reprendre

| Anti-pattern | Exemple observé | Raison du rejet |
| --- | --- | --- |
| BM25 ou tokens comme arbitre final | Agent Arche, OmniRoute, Sinan | Ne couvre ni compatibilité, ni risque, ni performance réelle |
| LLM libre comme unique routeur | Prompts de workflows et tags CCR | Sortie non déterministe, possibilité d'inventer une cible |
| Fallback vers le premier candidat | Weft et Mahoraga sur certains chemins | Rend les échecs silencieux et non explicables |
| Script de politique avec filesystem et réseau complets | CCR | Surface d'exfiltration et de secrets excessive |
| Tous les skills chargés au démarrage | Plusieurs packs | Coût de contexte et interférences d'instructions |
| Monolithe multi-produit | OmniRoute, AgenticX | Difficile à auditer, tester, distribuer et stabiliser |
| Apprentissage online sans propension ni shadow | Routeurs heuristiques | Impossible d'évaluer le regret et les biais de sélection |
| Tag textuel non authentifié comme preuve de route | CCR | Le contenu peut être produit ou imité par le modèle |
| Configuration comme preuve du modèle exécuté | Overlays Codex | Le runtime peut ignorer, substituer ou ne pas exposer la cible |
| Code sans licence comme dépendance | Route-To-Reason, Ares | Aucun droit de réutilisation accordé |

## Références Rust locales

### RTK

RTK montre une architecture de binaire unique efficace : modules à responsabilité réduite, modes typés, préservation des codes de sortie, sortie brute récupérable, garde `never_worse`, mesure du résultat réellement émis, hooks par client et tests de fidélité. Sa configuration deny warnings et deny unsafe convient au nouveau noyau.

À reprendre : squelette CLI, séparation core/adapters, tracking local, hooks multi-clients, sorties compactes, fallback conservateur, release LTO et binaire autonome.

### CC Copilot Bridge

CC Copilot Bridge montre la bonne frontière provider : trait async commun, contexte hors corps JSON, pipeline de stages, types `Tier`, `Phase` et `RouteDecision`, traduction par provider, streaming normalisé et configuration typée. La détection actuelle de phase par outils et le seuil de petite requête sont de bons signaux initiaux, mais pas encore un routeur complet.

À reprendre : trait d'adaptateur, normalisation des protocoles, pipeline composable, séparation plan/exec/explore/judge, health checks, SSE, erreurs sans fuite et secrets hors prompt.

### ccboard

ccboard apporte un workspace Rust multi-crates, des parsers par client, un store concurrent, des caches, des états dégradés et une observabilité multi-surface. Le pattern d'epoch monotone qui empêche un scan lent d'écraser un refresh plus récent est directement utile pour l'index des skills et la télémétrie des outcomes.

À reprendre : types dans un crate stable, parsers isolés, store thread-safe, dégradation explicite, tests sécurité et performance, cache invalide par version.

## Frontière de licence

- MIT et Apache-2.0 autorisent une réutilisation sous respect des notices et conditions.
- Elastic License 2.0 de WorkWeave impose une analyse juridique et interdit certaines formes de service managé. Les idées d'architecture peuvent être réimplémentées en clean room, sans copie du code.
- Route-To-Reason et Ares n'ont pas de fichier de licence dans le snapshot. Leur code ne doit pas être copié, distribué ou incorporé.
- Sinan et OmniRoute embarquent ou indexent du contenu tiers. Chaque agent, prompt ou skill doit conserver sa provenance et sa licence propre.

Le nouveau repository doit tenir un `THIRD_PARTY_NOTICES.md`, un SBOM, une provenance par asset et un test qui refuse les entrées sans licence déclarée lorsqu'elles doivent être redistribuées.

## Conclusion

Le produit à construire est plus proche d'un scheduler et d'un compilateur de plans agentiques que d'une skill. Les skills et hooks sont indispensables pour l'intégration, mais ils ne doivent jamais devenir la source d'autorité. Le cœur universel est un runtime Rust, local-first, explicable, durable et testable hors de tout client.
