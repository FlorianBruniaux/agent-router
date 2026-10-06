# Index de la documentation

[English](README.md) | Français

Tous les guides, spécifications et pages de référence de `docs/`, classés selon
ce que vous voulez faire. Les pages sont en anglais,
sauf celles marquées (FR).

## Démarrer

| Page | Contenu |
| --- | --- |
| [Installation](install.fr.md) (FR) | Prérequis, features de build, release locale, désinstallation |
| [Sortie de la CLI](cli-output.md) | Sortie lisible et `--json`, codes de sortie et codes d'erreur |
| [Formats de configuration](configuration-formats.md) | Couches de configuration JSON, TOML et YAML strictes |
| [Saisie guidée de la tâche](task-guide.md) | Compléter les faits manquants sans permission implicite |

## Router et planifier

| Page | Contenu |
| --- | --- |
| [Recherche](retrieval.md) | Recherche dans les catalogues, signaux de contexte, fusion de classements, vecteurs locaux |
| [Classification du risque](risk-classification.md) | Plancher de risque déterministe |
| [Génération de plans](plan-generation.md) | Affectations globales de plans, bornées et sous budgets |
| [Propositions de plan](plan-proposals.md) | Revue d'un plan proposé par un modèle |
| [Séparation des revues](review-separation.md) | Producteur et relecteur chez des providers distincts |
| [Backends de décision optionnels](backends.md) | Propositions sémantiques cantonnées au pool admissible |
| [Client sidecar standard](standard-sidecar.md) | Protocole du sidecar de classement local et consentement |
| [API locale limitée au routage](local-api.md) | `agent-router serve` sur un socket Unix privé |

## Catalogues et confiance

| Page | Contenu |
| --- | --- |
| [Catalogues signés](signed-catalogs.md) | Catalogues communs signés et règles de provenance |
| [Autorité du catalogue courant](catalog-store.md) | Sélection persistante du catalogue courant et révocation |
| [Routage signé](signed-routing.md) | Contraintes du catalogue signé appliquées avant le scoring |
| [Préparation signée](signed-preparation.md) | Revalidation du catalogue et des grants autour de l'assemblage |
| [Garde-fous de politique éditeur](catalog-active-cli.md) | Garde-fous de bibliothèque ; il n'existe pas de CLI `catalog-active` |
| [Contrats d'assets](asset-contracts.md) | Contrats d'assets liés et gates |
| [Importeur inerte](importer.md) | Des agents et skills Markdown vers un candidat non signé |

## Exécuter et vérifier

| Page | Contenu |
| --- | --- |
| [Préparation](preparation.md) | Préparer un nœud de workflow |
| [Exécution par la CLI](cli-execution.md) | Approuver, lancer, vérifier, conserver et purger les sorties |
| [Runtime](runtime.md) | Exécution en sandbox, reprise après crash, limites par plateforme |
| [Coordinateur](coordinator.md) | Vagues de workflow durables et transmission d'artefacts |
| [Escalade](escalation.md) | Escalade à partir de tentatives durables |
| [Admission partagée](admission.md) | Concurrence réservée entre propriétaires de runtime |
| [Budgets de session](session-budgets.md) | Allocations prudentes de coût et de tokens |
| [Validation structurée](structured-validation.md) | Contrats de validation typés et rapports |
| [Autres hosts](other-hosts.md) | Transports supervisés pour OpenCode et Copilot |
| [Transport provider](provider-transport.md) | Transport loopback vers le provider, machines mono-utilisateur uniquement |

## Backends, évaluation et feedback

| Page | Contenu |
| --- | --- |
| [Promotion de backend](backend-promotion.md) | Preuves de promotion signées et gates |
| [Store d'activation des backends](backend-store.md) | Activation durable, rollback et révocation |
| [Exécution d'un backend](backend-execution.md) | Exécuter une décision de backend signée |
| [Évaluation](evaluation.md) | Replay, cohortes, compromis et revue indépendante |
| [Protocole de mesure de la valeur du router](bench/router-value-protocol.md) | Banc préenregistrable du router contre des bras fixes |
| [Playbook du banc](bench/playbook.md) | Relancer et vérifier une campagne, journal des campagnes, résultats du pilote (en anglais) |
| [Candidat de calibration v2](bench/calibration-v2/README.md) (FR) | 2026-10-05 : protocole de collecte et modèle de préparation incomplet, sans autorisation d'exécution |
| [Diagnostic d'intégration native Codex](bench/native-integration-u02/README.md) | 2026-10-05 : paramètres et sortie vérifiés sur app-server local, intégration automatique CLI non admise |
| [Feedback](feedback.md) | Feedback local donné avec consentement, politiques hors ligne |

## Diagnostic, distribution et sécurité

| Page | Contenu |
| --- | --- |
| [Doctor](doctor.md) | Diagnostic en lecture seule et audit expurgé |
| [Distribution](distribution.md) | Distributions source et preuves de licence |
| [Distribution binaire](binary-distribution.md) | Export d'un binaire natif local |
| [Release locale](release-local.md) | Procédure de release locale vérifiable |
| [Audit des dépendances](dependency-audit.md) | Audit RustSec des dépendances verrouillées |
| [Fuzzing](fuzzing.md) | Fuzzing borné des parseurs |
| [Correctif SQLite WAL](sqlite-wal-fix.md) | Correction du reset WAL du SQLite embarqué |
| [Performance](performance.md) | Observations de performance en local |
| [Analyse du démarrage](startup-investigation.md) | Analyse du temps de premier lancement |

## Référence et état du projet

| Page | Contenu |
| --- | --- |
| [Contrats JSON](reference/json-contracts.md) | Contrats d'entrée et de sortie versionnés, règles de confiance |
| [Décisions d'architecture](adr/README.md) | ADR 0001 à 0019, avec frontmatter et index |
| [Registre d'avancement](completion.md) | Ce qui est livré, avec les preuves |
| [Acceptation restante](remaining-acceptance.md) (FR) | Exigences ouvertes et gates de preuve |
| [Spécification fonctionnelle](specs/functional-specification.md) (FR) | Exigences produit |
| [Spécification technique](specs/technical-specification.md) (FR) | Conception technique |
| [Comparaison avec des projets similaires](comparison.md) (EN) | Comparaison sourcée avec d'autres routeurs et outils pour agents de code |
| [Comparaison avec Omnigent](omnigent-comparison.md) (FR) | Comparaison et éléments de roadmap qui en découlent |
| [Point d'étape du 27/09/2026](checkpoints/2026-09-27-paused-work.md) | État du travail mis en pause |

## Contribuer

| Page | Contenu |
| --- | --- |
| [Harness de développement](harness.md) | Agents, skills et hooks du projet pour Claude Code et Codex |
| [Validation locale](local-ci.md) | Contrôles reproductibles, preuves conservées et limites sans budget Actions |
| [Preuves du harness](harness-evidence.md) | Preuves de validation du harness |
| [Grille de revue](review-grid.md) | Checklist utilisée pour la revue de code |
