# Mesurer Agent Router sur son propre développement

État du 10 octobre 2026 : préparation M1, M2 et M3 livrée et validée localement. Aucun comparatif réel lancé par ce lot, aucun gain économique établi.

## Base relue et fusionnée

La [PR #157](https://github.com/FlorianBruniaux/agent-router/pull/157) est fusionnée au commit `6a8de24589d0329c22d35ffc89bcb0bf641aaf94`. Son arbre est celui de la révision relue `663128c19d13cf300afb1778eace4614f316e4b4`. La commande de corrélation hors ligne et le [premier essai](../self-dogfood-b1b-2026-10-10/README.md) sont disponibles.

La revue indépendante ne relève aucun défaut sur les trois fichiers Rust. Les 414 empreintes de sources et les cinq journaux de validation conservés concordent : 1 735 résultats de tests réussis, 0 échec, 42 ignorés, formatage, Clippy, harness et Gitleaks réussis. Les cinq tests CLI ciblés ont été rejoués après la revue. Ces résultats macOS ne valent pas une validation Linux ; GitHub confirme que la CI n'a pas démarré à cause de la facturation.

## Livrables et critères

| Lot | Résultat attendu | Critère de passage |
| --- | --- | --- |
| [M1](m1/README.md) | Catalogue de campagne et disponibilité observée | Au moins deux candidats admissibles ; absence, effort incompatible et substitution détectés ; choix reproductible par la CLI |
| [M2](m2/README.md) | Bilan de tâche issu de traces déjà conservées | Snapshots dédupliqués, reprises conservées, périmètres ambigus et coûts manquants explicitement inconnus |
| [M3](m3/README.md) | Trois tâches utiles et répétition du protocole hors ligne | 18 affectations conservées, y compris échecs et interruptions ; aucun appel réel par le banc |
| M4 | Manifeste final et premier bloc réel de trois configurations | Identités, collecte, budget et accord liés au manifeste exact |
| M5 | Pilote complet et rapport régénérable | Toutes les affectations et dépenses connues conservées ; conclusion favorable, défavorable ou non concluante |

M1, M2 et M3 ont été développés en parallèle et ont passé leur revue indépendante. Le [relevé de validation](verification.json) conserve les empreintes, résultats et limites. Les modèles employés pour développer ces outils ne sont pas des bras du comparatif et ne constituent pas une mesure de gain.

## Comparaison proposée

Trois tâches encore non résolues, trois configurations (modèle puissant fixe, modèle économique fixe, Agent Router), deux répétitions : 18 affectations. Les reprises augmentent la consommation de leur affectation, jamais le dénominateur. Même hôte, instructions, outils, critères et revue ; sessions et sorties isolées, ordre scellé, exécution séquentielle dans chaque bloc. Les tests cachés restent hors du contexte des exécutants ; un engagement de hash ne prouve pas leur isolation.

Conserver séparément délai total, temps humain actif, tests locaux, consommation annoncée par l'hôte, estimation tarifée et facture. Le cache fait partie des tokens d'entrée. Les compteurs parent/enfant ne s'additionnent que si leurs périmètres sont établis disjoints. Le coût par tâche acceptée inclut les échecs ; sans tâche acceptée, le ratio est indéfini. Les poids du catalogue ne sont pas des prix.

## Frontière avant M4

Le parcours envisagé est une délégation Codex assistée, distincte du runtime natif d'Agent Router. Les qualifications natives #131, #132 et #133 restent inchangées. Un manifeste de préparation et un test avec fournisseur simulé n'autorisent aucune exécution réelle.

M4 devra sceller les modèles/efforts, tâches, contextes, règles de reprise, collecte, versions, coûts déjà engagés, unité, budget restant et mécanismes d'arrêt. Si le plafond est seulement suivi et non garanti à chaque envoi, le candidat doit le dire. Arrêt sur dérive, substitution, perte d'identité, collecte inexploitable ou appel ambigu ; ne pas rejouer pour reconstruire de la télémétrie. Un changement du protocole clôt la campagne de diagnostic et ouvre une nouvelle version, sans effacer les dépenses.

Le suivi reste dans [#5](https://github.com/FlorianBruniaux/agent-router/issues/5) pour la comparaison et [#120](https://github.com/FlorianBruniaux/agent-router/issues/120) pour la corrélation. Ces issues restent ouvertes. Trois tâches permettent une calibration exploratoire, pas une promesse d'économie généralisable.

## Résultats de cette livraison

- M1 : cinq modèles annoncés et 29 couples modèle/effort ; 12 tests, dont le routage par la CLI existante. La démonstration choisit Luna et conserve des alternatives Sol, sans preuve de gain.
- M2 : bilan régénérable depuis les exports existants ; 20 tests. Fenêtres parent/enfant et coûts restent séparés ou inconnus. Les alias du rapport ne sont pas des identités natives.
- M3 : trois tâches non résolues sur le banc Python, 18 affectations et 19 tests. Le premier bloc simulé conserve quatre tentatives, deux acceptations fictives, un rejet et 15 affectations non exécutées. Le calculateur existant retrouve les montants fictifs attendus, reprises comprises.
- Contrôles communs : 416 tests Python réussis, contrôle de syntaxe et Gitleaks réussis, sans changement des sources durant les contrôles. Les nouvelles suites sont inscrites dans le [lanceur local](../../local-ci.md). Les validations Rust sont réutilisées après comparaison des 414 empreintes ; le workspace Rust complet n'a pas été rejoué pour ce lot sans modification Rust.

La revue a corrigé des métadonnées trop permissives dans l'export M2, des statuts de test mal typés et une projection M3 qui pouvait accepter une liste d'affectations amputée. Les tests négatifs refusent maintenant ces cas. Ces corrections protègent la mesure ; elles ne prouvent aucune économie.

Pour essayer sans appel modèle, suivre les commandes des guides [M1](m1/README.md), [M2](m2/README.md) et [M3](m3/README.md). Les tâches retenues portent sur les outils du banc, pas sur toute la variété du développement Rust. Leurs solutions restent à produire pendant un futur essai autorisé. La suite est M4 : préparer le manifeste concret, résoudre la couverture des compteurs et choisir un budget représentable avant le premier bloc réel.
