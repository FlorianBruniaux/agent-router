# Index de la documentation

[English](README.md) | Français

Toutes les pages de `docs/`, classées par besoin. Les pages sont en anglais,
sauf celles marquées (FR).

## Démarrer

| Page | Contenu |
| --- | --- |
| [Installation](install.fr.md) (FR) | Prérequis, options de compilation, release locale, désinstallation |
| [Sortie de la CLI](cli-output.md) | Sortie humaine et `--json`, codes de sortie et codes d'erreur |
| [Formats de configuration](configuration-formats.md) | Couches de configuration JSON, TOML et YAML strictes |
| [Saisie guidée d'une tâche](task-guide.md) | Compléter les faits manquants sans permission implicite |

## Router et planifier

| Page | Contenu |
| --- | --- |
| [Recherche](retrieval.md) | Recherche dans les catalogues, signaux de contexte, fusion de classements, vecteurs locaux |
| [Classification du risque](risk-classification.md) | Plancher de risque déterministe |
| [Génération de plans](plan-generation.md) | Affectations de plans bornées sous budget |
| [Propositions de plans](plan-proposals.md) | Revue d'un plan proposé par un modèle |
| [Séparation des revues](review-separation.md) | Producteur et relecteur chez des fournisseurs distincts |
| [Backends de décision optionnels](backends.md) | Propositions sémantiques gardées dans le pool admissible |
| [Client sidecar standard](standard-sidecar.md) | Protocole du sidecar de classement local et consentement |
| [API locale de routage](local-api.md) | `agent-router serve` sur un socket Unix privé |

## Catalogues et confiance

| Page | Contenu |
| --- | --- |
| [Catalogues signés](signed-catalogs.md) | Catalogues communs signés et règles de provenance |
| [Autorité du catalogue courant](catalog-store.md) | Sélection persistante du catalogue courant et révocation |
| [Routage signé](signed-routing.md) | Contraintes du catalogue signé avant le score |
| [Préparation signée](signed-preparation.md) | Revalidation du catalogue et des grants autour de l'assemblage |
| [Gardes de politique éditeur](catalog-active-cli.md) | Gardes de bibliothèque ; il n'existe pas de CLI `catalog-active` |
| [Contrats d'assets](asset-contracts.md) | Contrats d'assets bornés et gates |
| [Importeur inerte](importer.md) | Agents et skills Markdown vers un candidat non signé |

## Exécuter et vérifier

| Page | Contenu |
| --- | --- |
| [Préparation](preparation.md) | Préparer un nœud de workflow |
| [Exécution par la CLI](cli-execution.md) | Approuver, lancer, vérifier, conserver et purger les sorties |
| [Runtime](runtime.md) | Exécution confinée, reprise après crash, limites par plateforme |
| [Coordinateur](coordinator.md) | Vagues de workflow durables et passage d'artefacts |
| [Escalade](escalation.md) | Escalade depuis des tentatives durables |
| [Admission partagée](admission.md) | Concurrence réservée entre propriétaires de runtime |
| [Budgets de session](session-budgets.md) | Allocations prudentes de coût et de tokens |
| [Validation structurée](structured-validation.md) | Contrats de validation typés et rapports |
| [Autres hôtes](other-hosts.md) | Transports supervisés OpenCode et Copilot |
| [Transport fournisseur](provider-transport.md) | Transport loopback, hôtes mono-utilisateur uniquement |

## Backends, évaluation et feedback

| Page | Contenu |
| --- | --- |
| [Promotion de backend](backend-promotion.md) | Preuves de promotion signées et gates |
| [Store d'activation](backend-store.md) | Activation durable, rollback et révocation |
| [Exécution d'un backend](backend-execution.md) | Exécuter une décision de backend signée |
| [Évaluation](evaluation.md) | Replay, cohortes, compromis et revue indépendante |
| [Feedback](feedback.md) | Feedback local consenti et politiques hors ligne |

## Diagnostic, distribution et sécurité

| Page | Contenu |
| --- | --- |
| [Doctor](doctor.md) | Diagnostic en lecture seule et audit expurgé |
| [Distribution](distribution.md) | Distributions de sources et preuves de licences |
| [Distribution binaire](binary-distribution.md) | Export d'un binaire natif local |
| [Release locale](release-local.md) | Recette de release locale vérifiable |
| [Audit des dépendances](dependency-audit.md) | Audit RustSec des dépendances verrouillées |
| [Fuzzing](fuzzing.md) | Fuzzing borné des parseurs |
| [Correctif SQLite WAL](sqlite-wal-fix.md) | Correction du reset WAL de SQLite embarqué |
| [Performance](performance.md) | Observations de performance locales |
| [Premier lancement](startup-investigation.md) | Analyse du temps de premier lancement |

## Référence et état du projet

| Page | Contenu |
| --- | --- |
| [Contrats JSON](reference/json-contracts.md) | Contrats d'entrée et de sortie versionnés, règles de confiance |
| [Décisions d'architecture](adr/) | ADR 0001 à 0017 |
| [Registre de complétion](completion.md) | Ce qui est livré, avec les preuves |
| [Acceptation restante](remaining-acceptance.md) (FR) | Exigences ouvertes et gates de preuve |
| [Spécification fonctionnelle](specs/functional-specification.md) (FR) | Exigences produit |
| [Spécification technique](specs/technical-specification.md) (FR) | Conception technique |
| [Comparaison Omnigent](omnigent-comparison.md) (FR) | Comparaison et éléments de roadmap |
| [Point d'étape du 27/09/2026](checkpoints/2026-09-27-paused-work.md) | Instantané du travail en pause |

## Contribuer

| Page | Contenu |
| --- | --- |
| [Harness de développement](harness.md) | Agents, skills et hooks du projet pour Claude Code et Codex |
| [Preuves du harness](harness-evidence.md) | Preuves de validation du harness |
| [Grille de revue](review-grid.md) | Checklist de revue de code |
