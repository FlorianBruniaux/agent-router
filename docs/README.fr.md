# Index de la documentation

[Précontrôle des ressources F12a](bench/finite-client-qualification-2026-10-11/README.md), 2026-10-11 : refus du helper copié avec les 8 Gio demandés, contrôles locaux et gates natives restantes. NO_GO. [Composants macOS vérifiés séparément](bench/macos-component-qualification-2026-10-11/README.md) : 47 résultats et diagnostic instrumenté du refus, sans qualification native complète.

[English](README.md) | Français

Tous les guides, spécifications et pages de référence de `docs/`, classés selon
ce que vous voulez faire. Les pages sont en anglais,
sauf celles marquées (FR).

## Démarrer

[Plan de finalisation actuel](bench/finalization-plan-2026-10-07.md) (FR), 2026-10-07 : ordre des livraisons, critères de réussite, essai réel et publication. Travail planifié, sans preuve d’activation.

[Vérification de la publication](bench/verification-publication-hardening-2026-10-08.json), 2026-10-08 : 24 contrôles locaux, régressions négatives conservées et revue indépendante. La composition native et l’admission d’un fournisseur réel restent ouvertes.

| Page | Contenu |
| --- | --- |
| [Essai expérimental](experimental-release.md) (EN) | 2026-10-07 : candidat source, exemple hors ligne et limites des preuves par host ; sans annonce de publication |
| [Installation](install.fr.md) (FR) | Canaux Cargo épinglé et Homebrew local, deux binaires, prérequis et désinstallation ciblée |
| [Qualification de distribution](bench/distribution-qualification-2026-10-07.md) | 2026-10-07 : artefacts v2 actuels et preuves locales par canal ; sans publication ni preuve d'exécution provider |
| [Sortie de la CLI](cli-output.md) | Sortie lisible et `--json`, codes de sortie et codes d'erreur |
| [Formats de configuration](configuration-formats.md) | Couches de configuration JSON, TOML et YAML strictes |
| [Saisie guidée de la tâche](task-guide.md) | Compléter les faits manquants sans permission implicite |

## Router et planifier

| Page | Contenu |
| --- | --- |
| [Parcours des suggestions de skills](skill-selection.md) (FR) | Qui déclenche la recherche, filtrage, chargement et essai hors ligne |
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
| [Coûts de la révision acceptée](accepted-changes.md) | 2026-10-07 : calcul hors ligne, révocation et coûts incomplets ; données fournies, sans collecte native |
| [Corrélation des tentatives](attempt-correlation.md) (EN) | 2026-10-10 : CLI hors ligne et projection v2 complète ; données fournies, sans collecte native |
| [Évaluation](evaluation.md) | Replay, cohortes, compromis et revue indépendante |
| [Protocole de mesure de la valeur du router](bench/router-value-protocol.md) | Banc préenregistrable du router contre des bras fixes |
| [Playbook du banc](bench/playbook.md) | Relancer et vérifier une campagne, journal des campagnes, résultats du pilote (en anglais) |
| [Agent Router utilisé sur son propre code](bench/self-dogfood-b1b-2026-10-10/README.md) (EN) | 2026-10-10 : essai B1b guidé par le routage ; statut dans le rapport, aucun gain comparatif établi |
| [Bilan mesuré M4 v2](bench/measurement-m4-v2-result-2026-10-11/README.md) | 156 434 tokens hôte ; un changement unique accepté et intégré via [le suivi #164](bench/m4-fix-integration-2026-10-11.md) ; aucun gain ni qualification native établis |
| [Premier protocole M4](bench/measurement-m4/README.md) | Protocole historique v1 conservé après l'essai interrompu ; scellement inchangé |
| [Préparation des mesures M1–M3](bench/measurement-m123/README.md) | Catalogue de campagne, bilan des traces et répétition hors ligne ; aucun gain établi |
| [Candidat de calibration v2](bench/calibration-v2/README.md) (FR) | 2026-10-05 : protocole de collecte et modèle de préparation incomplet, sans autorisation d'exécution |
| [Décision sur l'entrée automatique](bench/owned-entry-2026-10-07/README.md) | 2026-10-07 : app-server direct et ACP non admis ; contrôle par envoi et comparaison comportementale encore incomplets |
| [Diagnostic d'intégration native Codex](bench/native-integration-u02/README.md) | 2026-10-05 : paramètres et sortie vérifiés sur app-server local, intégration automatique CLI non admise |
| [Feedback](feedback.md) | Feedback local donné avec consentement, politiques hors ligne |

## Diagnostic, distribution et sécurité

| Page | Contenu |
| --- | --- |
| [Doctor](doctor.md) | Diagnostic en lecture seule et audit expurgé |
| [Diagnostic de supervision native](bench/owned-supervision-feasibility-2026-10-08/README.md) | 2026-10-08 : diagnostic explicite sans fournisseur ; composition NO_GO, limites du broker, de mémoire et de nettoyage conservées |
| [Observation passive d’une exécution](run-observation.md) | 2026-10-11 : v3 pour la durée locale de capture du superviseur, v2 par tentative et v1 par défaut conservées ; durées natives/fournisseur et F04b/F05a complets restent ouverts |
| [Validation de l’observation](bench/verification-observation-2026-10-07.json) | 21 contrôles sélectionnés réussis : 1 633 résultats Rust workspace + 1 cœur, 290 Python ; sommes des lignes de résultats, 42 tests natifs opt-in ignorés ; sans équivalence avec toute la CI |
| [Validation de l’observation v2](bench/verification-observation-v2-2026-10-08.json) | 24 contrôles locaux après corrections de l’index ADR et du faux positif sur un hash de fixture, revue indépendante et compatibilité du lecteur de mesure ; faits natifs inconnus |
| [Validation des durées locales](bench/verification-local-capture-timing-2026-10-11.json) | V3 explicite : sept contrôles locaux réussis sur le code combiné, inconnues historiques conservées et revue indépendante ; durées natives/fournisseur et bout en bout encore non qualifiées |
| [Revue des fichiers du client natif](bench/verification-finite-client-assets-review-2026-10-11.json) | PR #163 fusionnée après revue indépendante et contrôles locaux macOS/Linux ; préparation seulement, F12a reste NO_GO |
| [Annulation du broker absent au lancement](bench/physical-send-feasibility-2026-10-07/outage-cancellation/README.md) | Broker absent avant lancement, interruption/terminal corrélés et nettoyage physique, zéro provider ; preuve de panne au démarrage ; voir la qualification ultérieure du client dédié ; admission globale encore ouverte |
| [Faisabilité du contrôle des envois](bench/physical-send-feasibility-2026-10-07/README.md) | 2026-10-07 : six cas natifs passent, absence du broker en timeout ; NO-GO global, sans admission provider réel |
| [Qualification du client Codex dédié](bench/owned-entry-qualification-2026-10-08/README.md) (EN) | 2026-10-08 : choix borné pour le patch simulé ; trois scénarios natifs locaux, sans admission fournisseur réel |
| [Validation du client dédié](bench/verification-owned-client-2026-10-08.json) | Contrôles locaux liés aux sources, revues indépendantes et limites du diagnostic natif |
| [Distribution](distribution.md) | Distributions source et preuves de licence |
| [Distribution binaire](binary-distribution.md) | Export d'un binaire natif local |
| [Release locale](release-local.md) | Procédure de release locale vérifiable |
| [Qualification de release, 2026-10-07](bench/release-qualification-2026-10-07.md) (EN) | Artefacts exacts, installation/retrait locaux, correction de release, inventaire sécurité et limites de publication |
| [Audit des dépendances](dependency-audit.md) | Audit RustSec des dépendances verrouillées |
| [Fuzzing](fuzzing.md) | Fuzzing borné des parseurs |
| [Correctif SQLite WAL](sqlite-wal-fix.md) | Correction du reset WAL du SQLite embarqué |
| [Performance](performance.md) | Observations de performance en local |
| [Analyse du démarrage](startup-investigation.md) | Analyse du temps de premier lancement |

## Référence et état du projet

| Page | Contenu |
| --- | --- |
| [Contrats JSON](reference/json-contracts.md) | Contrats d'entrée et de sortie versionnés, règles de confiance |
| [Décisions d'architecture](adr/README.md) | Décisions ADR, avec frontmatter et index |
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
