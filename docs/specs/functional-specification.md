# Spécifications fonctionnelles

## 1. Produit

Nom de travail : `agent-router`.

`agent-router` est un routeur local universel pour assistants de code et systèmes agentiques. À partir d'un objectif utilisateur et de l'environnement réellement disponible, il construit, explique et éventuellement exécute le meilleur plan sous contraintes.

La valeur centrale est la sélection jointe de six dimensions :

1. forme de travail ;
2. modèle et effort ;
3. agent ou rôle ;
4. skills et prompts ;
5. outils, sandbox et permissions ;
6. harness et transport.

Le produit fonctionne même sans apprentissage, embeddings, daemon ou cloud. La politique déterministe locale est le socle. Les optimisations apprises sont optionnelles et révocables.

## 2. Objectifs et non-objectifs

### Objectifs

- Automatiser un workflow de type modèle fort pour planifier, modèle plus économique pour exécuter, puis modèle indépendant pour vérifier.
- Adapter ce workflow à Codex CLI, Codex App, tâches Codex, Claude Code, Copilot CLI, OpenCode et autres CLIs supportées.
- Choisir les agents et skills pertinents sans charger tout le catalogue dans le contexte.
- Réduire coût, latence et pollution de contexte sans dégrader le taux de réussite.
- Rendre chaque exclusion, score, fallback et changement de politique explicable.
- Exécuter des tâches simples, des sous-agents natifs et des DAGs durables.
- Apprendre des résultats locaux avec une activation prudente.
- Conserver une frontière stricte entre instruction textuelle et extension exécutable.

### Non-objectifs initiaux

- Remplacer les harnesses Codex, Claude Code ou OpenCode.
- Héberger un service de proxy public multi-tenant.
- Installer silencieusement des providers, modèles, hooks ou skills.
- Modifier automatiquement le modèle parent d'une conversation si le host ne l'autorise pas.
- Promettre un support identique dans tous les clients.
- Entraîner un grand modèle de routage lors du MVP.
- Exécuter automatiquement du code tiers non approuvé.

## 3. Acteurs

| Acteur | Besoin | Autorité |
| --- | --- | --- |
| Utilisateur | Résultat fiable au bon coût | Définit objectif, budget, permissions et overrides |
| Agent parent | Plan, délégation, intégration | Décide dans les limites utilisateur et projet |
| Routeur | Candidats, contraintes, score, trace | Ne peut pas élargir les permissions |
| Adaptateur de host | Traduire et exécuter la décision | Limité aux capacités déclarées du host |
| Worker | Produire un artefact borné | Possède uniquement sa tâche et son périmètre |
| Vérificateur | Évaluer preuves et critères | Lecture seule par défaut, indépendant du worker |
| Mainteneur de catalogue | Publier modèles, agents et skills | Doit signer, versionner et déclarer la provenance |
| Policy sidecar optionnel | Reranker les candidats | Ne crée aucun candidat et ne change aucune contrainte dure |

## 4. Modes utilisateur

| Mode | Comportement | Mutation |
| --- | --- | --- |
| `inspect` | Inventorie les capacités du host, modèles, agents, skills et outils | Aucune |
| `route` | Retourne la meilleure route atomique et ses alternatives | Aucune |
| `plan` | Produit un plan multi-étapes sans l'exécuter | Aucune |
| `run` | Exécute une route ou un plan approuvé | Selon le plan |
| `explain` | Décompose signaux, exclusions, scores et fallback | Aucune |
| `shadow` | Calcule une nouvelle politique sans l'appliquer | Aucune sur l'exécution |
| `feedback` | Attache un verdict humain ou automatique à une exécution | Écrit la télémétrie locale |
| `replay` | Rejoue un corpus de décisions hors ligne | Écrit seulement les résultats d'évaluation |
| `doctor` | Vérifie installation, permissions, catalogues et adaptateurs | Aucune par défaut |

## 5. Parcours principal

```text
objectif utilisateur
  -> normalisation du contexte
  -> classification du risque et du type de tâche
  -> décomposition optionnelle
  -> récupération des agents, skills, prompts et outils
  -> filtrage par contraintes dures
  -> génération de plans complets
  -> score coût, qualité, latence, contexte et risque
  -> décision expliquée
  -> approbation si nécessaire
  -> exécution par l'adaptateur du host
  -> vérifications
  -> outcome et feedback
  -> apprentissage offline ou shadow
```

## 6. Exigences fonctionnelles

### FR-001 Inventaire des capacités

Le système détecte, pour chaque host : version, surface de plugins, hooks, skills, sous-agents, modèles, efforts, outils, sandbox, permissions, streaming, reprise et métadonnées runtime.

Critères d'acceptation :

- chaque capacité est `supported`, `unsupported` ou `unknown` ;
- `unknown` n'est jamais traité comme supporté ;
- la source et l'heure de la preuve sont enregistrées ;
- un probe n'effectue aucune mutation non annoncée.

### FR-002 Normalisation de la tâche

Le système transforme la requête en `TaskContext` avec objectif, livrables, contraintes, scope, risque, données sensibles, coûts, délais, dépendances, mutations permises et critères d'acceptation.

Une ambiguïté qui modifie l'autorité, le coût ou la sécurité produit une question. Les ambiguïtés mineures deviennent des hypothèses visibles.

### FR-003 Décomposition bornée

Le planificateur peut produire un DAG de tâches. Chaque tâche a un propriétaire, un périmètre de fichiers, des entrées, des sorties, des dépendances, un budget, un stop rule et une vérification.

Contraintes :

- aucun cycle ;
- aucune double propriété d'un fichier en écriture dans une même vague ;
- profondeur et largeur maximales configurables ;
- la parallélisation est réservée aux branches indépendantes ;
- un plan trop coûteux doit proposer une version simplifiée.

### FR-004 Recherche des ressources

Le moteur recherche séparément agents, skills, prompts, outils et modèles. Il utilise des champs et boosts distincts : nom, description, exemples, tags, task kinds, chemins, langages, outils requis, anti-triggers et historique de réussite.

La sortie contient les top-k, les scores de chaque retriever, la fusion, les termes ou exemples responsables et les éléments exclus.

### FR-005 Filtrage dur

Avant le ranking final, le système élimine tout plan incompatible avec :

- le host et sa version ;
- le provider et l'auth disponibles ;
- les tools, formats, vision, audio, fichiers, citations ou structured output requis ;
- le contexte maximal ;
- l'effort supporté ;
- les permissions, le sandbox et le réseau ;
- les licences et trust tiers ;
- le budget et les politiques de données ;
- le niveau de récursion ou de concurrence ;
- les contraintes de cache et de signatures de raisonnement.

Chaque exclusion possède un code stable et un message humain.

### FR-006 Sélection modèle et effort

Le système choisit le modèle et l'effort pour chaque étape, pas seulement pour la conversation entière.

Politique cold start recommandée :

| Type de travail | Modèle Codex initial | Effort initial | Escalade |
| --- | --- | --- | --- |
| Recherche ciblée et lecture mécanique | Luna | low ou medium | Terra si ambigu ou multimodal |
| Modification bornée déjà conçue | Terra | medium | Sol high après deux échecs distincts |
| Tests, reproduction, logs | Luna ou Terra | low à high selon diagnostic | Sol si cause systémique |
| Plan multi-fichiers ou architecture | Sol | high | Sol xhigh ou max si blast radius élevé |
| Sécurité, auth, migration, données | Sol | high ou xhigh | Vérificateur Sol indépendant |
| Coordination récursive | Terra ou Sol selon risque | high | Parent Sol conserve l'autorité |
| Revue indépendante | Terra high par défaut | Sol high pour risque élevé | Humain si preuve insuffisante |

Cette table est une politique initiale configurable, pas une vérité universelle. Le catalogue runtime remplace les alias par les identifiants exacts supportés.

### FR-007 Sélection d'agent

Un agent est sélectionné sur rôle, scope, sandbox, compétences, outils, historique, coût et contrat de sortie. Le système distingue agent d'identité, worker générique et simple prompt de rôle.

Si le host ne peut pas instancier le nom choisi, l'adaptateur utilise un worker générique avec un prompt d'identité borné et enregistre la dégradation.

### FR-008 Sélection et chargement de skills

Le système indexe les métadonnées sans injecter les corps de skills. Après décision, il charge seulement les skills requises pour l'étape courante.

Les skills ont : version, provenance, licence, hash, trust tier, triggers, anti-triggers, exemples, outils requis, permissions, compatibilité host, budget de contexte et type `instruction` ou `executable`.

Une skill exécutable exige une politique et un sandbox distincts.

### FR-009 Sélection de prompt

Les prompts sont des assets versionnés avec variables typées, host cible, modèles compatibles, budget, contrat de sortie et tests. Le système refuse une variable manquante et évite toute concaténation non bornée d'instructions.

### FR-010 Planification plan puis exécution

Le preset `plan-execute-verify` doit :

1. demander à un modèle fort de produire un plan et des critères ;
2. faire valider le plan par le parent ou l'utilisateur selon le risque ;
3. distribuer les étapes à des modèles adaptés ;
4. intégrer les résultats ;
5. lancer une vérification indépendante ;
6. conclure seulement sur preuve.

Dans Codex, un preset peut par exemple utiliser Sol high pour le plan, Terra medium pour les modifications et Terra ou Sol high pour la revue. L'adaptateur ne doit jamais prétendre avoir changé le modèle parent si seule la cible d'un sous-agent a été contrôlée.

### FR-011 Exécution atomique et DAG

Le runtime supporte :

- exécution synchrone et asynchrone ;
- concurrence bornée ;
- files par provider et agent logique ;
- retry seulement sur classes autorisées ;
- timeout, annulation et kill du groupe de processus ;
- reprise d'un DAG après interruption ;
- worktree isolé pour les tâches d'écriture ;
- artefacts de résultat, diff, stdout et stderr ;
- intégration manuelle ou approuvée, jamais implicite.

### FR-012 Vérification

Trois preuves sont distinctes :

1. transport terminé ;
2. commande de validation passée ;
3. objectif fonctionnel satisfait.

Le système ne déduit jamais 3 de 1. Les vérifications sont des commandes argv, des assertions structurées, une revue agent indépendante ou un verdict humain. La provenance de chaque preuve est enregistrée.

### FR-013 Explicabilité

`explain` montre :

- les signaux observés ;
- les hypothèses ;
- les candidats récupérés ;
- les contraintes et exclusions ;
- les composantes de score normalisées ;
- la politique et sa version ;
- le plan retenu et les alternatives ;
- les fallbacks possibles ;
- la confiance et les preuves manquantes.

Les sorties doivent être lisibles et disponibles en JSON stable.

### FR-014 Overrides et précédence

Ordre de précédence :

1. instruction utilisateur explicite pour la tâche ;
2. politique de sécurité ou d'organisation plus restrictive ;
3. configuration projet ;
4. preset sélectionné ;
5. politique apprise active ;
6. politique déterministe par défaut.

Un niveau supérieur peut restreindre un niveau inférieur. Aucun niveau ne peut élargir une interdiction de sécurité sans approbation dédiée.

### FR-015 Feedback

Le feedback accepte : réussite, échec, qualité, coût perçu, latence, correction humaine, motif d'échec et commentaire. Les outcomes automatiques restent distincts du jugement humain.

Le système doit supporter l'effacement de données locales et l'opt-out total de l'apprentissage.

### FR-016 Apprentissage

Le moteur apprend au minimum :

- compétence Beta par couple agent-skill ;
- coût, latence et tokens par modèle-effort-harness ;
- taux d'échec par transport ;
- préférence de plan sous contexte ;
- dérive et quarantaine.

Les politiques passent par offline replay, shadow, canary et activation explicite. La propension du choix est journalisée pour les évaluations contrefactuelles.

### FR-017 Catalogues

Le système gère des catalogues de modèles, agents, skills, prompts, tools, hosts et presets. Les entrées sont versionnées, validées, hashées et adressables par identifiant stable.

Une mise à jour invalide les index et caches concernés sans écraser les modifications utilisateur.

### FR-018 Sécurité et confiance

Avant activation d'un asset tiers, le système affiche source, commit ou version, licence, hash, permissions et findings du scanner. Les niveaux proposés sont `builtin`, `trusted-local`, `verified-third-party`, `untrusted` et `blocked`.

`untrusted` peut être indexé mais pas exécuté. `blocked` ne doit pas entrer dans le prompt.

### FR-019 Doctor et diagnostics

`doctor` vérifie : fichiers, schémas, permissions, secrets, chemins, symlinks, modèles, CLIs, auth sans révélation, versions de host, hooks, plugins, DB, index, processus orphelins et dérive des fichiers gérés.

Le diagnostic différencie configuration déclarée et comportement runtime réellement vérifié.

### FR-020 Export et audit

Le système exporte un bundle d'audit redacted contenant config effective, catalogue, décision, événements, preuves, hashes et versions. Le bundle exclut prompts, secrets et contenu privé par défaut.

## 7. Matrice des surfaces

| Surface | Niveau cible | Mécanisme principal | Limite à afficher |
| --- | --- | --- | --- |
| Codex CLI | Fort | agents natifs, skills, AGENTS.md, plugin, CLI | modèle parent contrôlé par le lancement ou l'utilisateur |
| Codex App | Fort à moyen | mêmes agents et skills si exposés par le host | certaines combinaisons de fork et rôle peuvent être refusées |
| Tâches Codex | Moyen | handoff autonome et adaptateur de tâche si API disponible | créer une nouvelle tâche est une action utilisateur distincte |
| Claude Code | Fort | plugin, hooks, native Agent/Workflow, proxy ou shim | événements et modèles disponibles varient par version |
| GitHub Copilot CLI | Moyen | plugin et shims de providers | orchestration native moins riche selon version |
| OpenCode | Moyen | plugin/config et CLI supervisée | sécurité des providers et permissions à sonder |
| API compatible OpenAI/Anthropic | Fort sur transport | proxy et traduction de protocoles | ne fournit pas seul un agent avec workspace et tools |
| Client inconnu | Minimal | plan JSON et instructions manuelles | aucune exécution automatique revendiquée |

Le support final doit être calculé par probe, pas codé comme une promesse permanente.

## 8. Presets initiaux

| Preset | Forme | Cas d'usage |
| --- | --- | --- |
| `direct-safe` | un agent, un modèle, vérification locale | tâche petite et bornée |
| `plan-execute-verify` | plan fort, workers économiques, reviewer indépendant | équivalent universel d'opusplan |
| `explore-then-build` | explorateurs read-only, intégration, worker | codebase inconnue |
| `fanout-review` | plusieurs reviewers, synthèse parent | revue à fort enjeu |
| `incident` | collecte parallèle, coordinateur, stop rules | diagnostic urgent sans mutation implicite |
| `migration` | inventaire, plan, simulation, exécution séquentielle, rollback | changements irréversibles |
| `security-readonly` | agents et tools read-only, modèle fort | auth, secrets, permissions |
| `bulk-mechanical` | lots indépendants, faible contexte, validation déterministe | transformations répétitives |

## 9. Exigences non fonctionnelles

| ID | Exigence | Cible MVP |
| --- | --- | --- |
| NFR-001 | Latence de décision déterministe | p95 inférieur à 100 ms hors embeddings |
| NFR-002 | Latence de récupération locale | p95 inférieur à 50 ms pour 10 000 assets |
| NFR-003 | Démarrage CLI | inférieur à 150 ms sur machine de développement |
| NFR-004 | Crash safety | aucun état de tâche partiellement réécrit |
| NFR-005 | Reprise | DAG reprenable après kill du processus |
| NFR-006 | Sécurité fichiers | répertoires privés 0700, artefacts sensibles 0600 sur Unix |
| NFR-007 | Portabilité | macOS et Linux au MVP, Windows ensuite |
| NFR-008 | Observabilité | 100 % des décisions ont policy id et trace id |
| NFR-009 | Déterminisme | même contexte et même snapshot donnent la même route en mode stable |
| NFR-010 | Compatibilité | aucune entrée inconnue traitée comme supportée |
| NFR-011 | Confidentialité | aucun réseau pour le routing local par défaut |
| NFR-012 | Qualité Rust | unsafe interdit, warnings et clippy refusés en CI |

## 10. Mesures de succès

- taux de réussite fonctionnelle par classe de tâche ;
- regret qualité-coût face au meilleur plan observé ;
- coût et latence par succès, pas par appel ;
- précision@k et rappel@k des skills utiles ;
- taux de routes annulées par incompatibilité après dispatch ;
- stabilité des routes entre versions ;
- taux d'acceptation humaine du premier plan ;
- fréquence des fallbacks et escalades ;
- faux positifs du scanner de skills ;
- part de décisions avec preuve runtime complète ;
- overhead du routeur face au temps de travail agentique.

## 11. Critères de sortie MVP

Le MVP est prêt quand :

1. Codex CLI et Claude Code passent un probe réel sur une matrice versionnée ;
2. la politique déterministe produit un plan explicable pour 100 fixtures ;
3. BM25 retrouve les agents et skills attendus avec rappel@5 au-dessus du seuil défini par le corpus humain ;
4. les contraintes dures ont zéro violation sur les fixtures de sécurité ;
5. un workflow interrompu reprend sans réexécuter une étape réussie ;
6. les worktrees ne sont jamais intégrés sans action explicite ;
7. le bundle d'audit ne révèle aucun secret des fixtures ;
8. le produit fonctionne sans embeddings, cloud ou daemon ;
9. les affirmations de modèle exécuté reposent sur métadonnées runtime ou restent `unknown` ;
10. les licences et notices de tous les assets distribués sont validées.
