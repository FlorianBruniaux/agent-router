# Premier essai personnel Codex CLI : candidat de protocole

Ce document prépare un essai Agent Router sur Codex CLI macOS.
Il ne lance aucune tâche et ne constitue ni un accord ni une preuve de gain.
Le [plan de livraison](../status-and-action-plan-2026-10-05.md) définit U08-U10.

## Trois décisions de Florian

1. **Périmètre et tâche** : retenir une petite tâche utile, ses critères
   d'acceptation et ses chemins d'écriture. Proposition : tests Rust ou docs
   Agent Router. Le choix exact reste à confirmer.
2. **Abonnement/API et plafond** : choisir le mode, le plafond global et son
   unité, puis préciser la dépense antérieure et les quotas observables.
   Un budget suivi ne garantit pas un plafond strict ; cette limite doit être
   acceptée explicitement si ce trajet est proposé.
3. **Accord final au protocole** : approuver le candidat concret et le digest
   du manifeste final, avec modèles/efforts, collecte, conservation, règles
   d'arrêt et trajet réellement admis. Cet accord reste manquant.

Les versions, fichiers, hashes, configurations, ordre, identité et preuves
techniques seront préparés par l'agent pour revue, sans questionnaire par champ.
Le [relevé existant](missing-inputs.json) conserve les entrées manquantes exactes.
Réutiliser le [template](manifest.template.json) et sa [forme v2](README.md#forme-renseignée-et-vocabulaire-conservé).
Aucun nouveau runner, collecteur ou stockage n'est nécessaire à ce protocole.

## Choisir et sceller une tâche réelle

Choisir avant toute solution du modèle. Figer le SHA Git complet, un départ
propre, le statement, les chemins autorisés, le test local et la revue humaine.
Les tests cachés restent hors des entrées des modèles et hors des autres bras.
L'agent vérifie provenance et séparation sémantique des futures tâches tenues
à l'écart. Les fixtures synthétiques éprouvent la collecte, pas la valeur réelle.

Exemples à revalider, aucune tâche réservée : actualiser une livraison dans
le plan/playbook si cette actualisation reste nécessaire ; ajouter un cas de
régression Rust après reproduction d'un défaut ; documenter un rapport incomplet
à partir d'une fixture existante. Une exploration complète précède toute
affirmation de test manquant. Ne pas inventer un bug pour constituer le corpus.
Un travail du dépôt ne satisfait pas automatiquement la provenance de PR
fusionnées exigée pour le corpus final.

## Étapes et admission

1. Préparer les entrées techniques du manifeste et leurs preuves locales.
   U02b/U06/U07 doivent prouver déclenchement utile unique, décision appliquée
   ou refus, panne sans boucle et retrait en nouvelle session. U04b/U05 doivent
   conserver les tentatives et qualifier usage/coût sur le trajet retenu.
2. Sceller fichiers/hashes, configurations, critères, seed, ordre et répétitions.
   Exécuter le [validateur hors ligne](README.md#validation-hors-ligne-u08),
   relire les preuves, puis obtenir l'accord final lié au digest exact.
   Une préparation hors ligne valide ne prouve ni admission ni autorisation.
3. Après admission et accord, créer la ligne d'affectation avant l'envoi.
   Exécuter une tâche A-v2, vérifier localement puis faire la revue humaine.
   Aucun appel modèle supplémentaire ne sert seulement à vérifier le résultat.
4. Si l'ordre apparié a été scellé avant cet appel, conserver ce run comme
   première affectation de calibration ; sinon le qualifier essai d'utilisabilité
   hors comparaison. Sa consommation demeure dans le budget global.
5. Conserver les résultats, effectuer le retrait vérifié en nouvelle session,
   puis exporter les métriques nettoyées dans `docs/bench/runs/<date>-<campagne>/`
   et les référencer dans le [playbook](../playbook.md). Aucun prompt privé,
   réponse brute, chemin personnel ou secret dans l'export versionné.

## Critères et mesures

Réussite du premier essai : tâche acceptée par test et revue humaine, décision
et application reliées, invocation utile sans doublon, bilan avec provenance,
et retrait observé. Cela prouve l'utilisabilité de ce scénario, pas une économie.

| Mesure | Règle de recueil |
| --- | --- |
| Identité/application | Tâche, bras, répétition, tentative, décision et IDs natifs observés ; recommandé/invoqué/refusé distincts ; doublons et interventions visibles. |
| Qualité | Test local et verdict humain distincts ; accepté/rejeté/UNKNOWN, scope respecté, défauts restants et motif. Succès de transport seul insuffisant. |
| Reprises | Toutes tentatives, erreurs, interruptions et escalades ; conserver chaque affectation ; dédupliquer snapshots seulement. |
| Temps | Machine, attente, préparation/revue/correction humaines séparées ; bornes fiables ou UNKNOWN ; pauses et simultanéité qualifiées ; aucun taux horaire inventé. |
| Tokens/coût | Catégories réellement observées, prix datés et portée ; sondes/tentatives incluses ; usage/prix manquant donne total UNKNOWN, sous-total connu et couverture ; zéro accepté donne ratio indéfini. |
| Abonnement | Quotas/crédits observables avec unité/portée ; concurrence qualifiée ; estimation API, quota et facture distincts ; coût non observé ne signifie pas zéro. |

Une capture native externe garde cette provenance ; aucun reçu AR fabriqué.
Une panne de télémétrie ne justifie aucune relance d'une tâche déjà exécutée.

## Plafond strict non prouvé et arrêts

Le [trajet U06](../native-integration-u06/README.md) a été observé face à un
fournisseur simulé. Session unique, estimation USD, timeout, tokens bornés ou
abonnement ne prouvent pas une borne sur chaque envoi physique réel.
Le [runner historique](../campaign-next/README.md) reste désactivé pour les appels
réels et conserve son plafond historique ; ne pas traiter celui-ci comme crédit neuf.
Un plafond strict exige contrôle d'envoi/réservation vérifié et coûts couverts.
Sinon un essai natif à budget suivi requiert une proposition et un accord propres,
sans contourner ce runner ni déclarer `send_control_verified` vérifié.

Arrêter tout nouvel appel sur dérive de hash/version, modèle inattendu, identité
perdue, résultat ambigu ou coût inconnu empêchant le plafond. Conserver la ligne,
réconcilier la dépense avant nouvel accord, sans retry automatique ambigu.
Pour le retrait, garder les preuves hors des fichiers retirés ; restaurer seulement
les préimages possédées et retirer les nouveaux fichiers inchangés en excluant
les auteurs concurrents. Toute divergence interdit l'écrasement.

## Calibration distincte de confirmation

Calibration appariée : A-v2/E sur mêmes tâches/départs, ordre consigné ; D si
l'usage habituel diffère, S seulement après hypothèse fixée avant résultats.
La collecte et les configurations sont figées avant le bloc ; rapport descriptif.
Après calibration, Florian fixe les marges qualité/coût/temps et approuve la
confirmation sur tâches distinctes, sans retirer un comparateur parce qu'il gagne.
Sceller taille, répétitions et règle unique avant résultats ; valider toute règle
statistique sur simulations. Sans règle validée, adoption inconclusive.
Aucun cas, patch ou solution de calibration ne rejoint la confirmation.
