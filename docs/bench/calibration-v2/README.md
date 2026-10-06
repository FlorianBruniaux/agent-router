# Calibration v2 : vérifier la collecte avant de comparer

Ce candidat prépare U08 du [plan de livraison](../status-and-action-plan-2026-10-05.md).
Il ne lance aucune tâche. Le [manifeste à compléter](manifest.template.json)
est un document de préparation, pas un format accepté par la CLI ou par le
runner historique. Ses valeurs `null` restent des décisions manquantes.

## Premier bloc

Comparer A-v2, qui peut choisir une seule session, à E, une session avec le
modèle de référence et un effort fixé. Inclure D si l'usage habituel de Florian
diffère de E. Ajouter une règle économique S seulement après avoir écrit son
hypothèse, avant les résultats du bloc concerné. Chaque tâche part du même
commit propre et reçoit tous les bras retenus dans un ordre tiré avec une seed
consignée. Les configurations de chaque bras sont figées avant le bloc.

Le [candidat existant](../campaign-next/README.md) contient trois tâches
synthétiques utilisables pour éprouver la collecte. Elles ne prouvent ni la
représentativité de l'usage quotidien, ni la provenance exigée pour le corpus
final. Sélectionner ensuite des tâches réelles avant d'en voir les solutions,
avec commit de départ, périmètre d'écriture, test d'acceptation et revue humaine.
Conserver les tests cachés hors des entrées fournies aux modèles.

Le pilote A-v1 et ses résultats restent inchangés. La calibration décrit les
résultats observés et les données manquantes. Elle ne décide pas d'une adoption
statistique et n'applique ni p-value ni règle de chevauchement d'intervalles.
Cette règle remplace, pour ce candidat v2 seulement, les deux formulations
contradictoires du [protocole historique](../router-value-protocol.md).

## Conditions avant un appel réel

1. U02/U06/U07 doivent prouver le parcours natif retenu. U03/U04/U05 doivent
   vérifier collecte, conservation des tentatives et qualification des coûts.
2. Florian valide le périmètre du bloc, les modèles/efforts et un plafond
   global lié au manifeste final. L'ancien candidat conserve un plafond de
   5 USD et refuse les appels réels faute de borne vérifiée ; ce document
   n'augmente pas ce plafond et ne constitue pas une nouvelle approbation.
3. Sceller les hashes du corpus, des tests, de la politique, des prix datés,
   des binaires, de la configuration nettoyée et du code d'analyse. Enregistrer
   versions d'hôtes, ordre des essais et répétitions. Toute modification
   produit un nouveau manifeste et un nouveau bloc.
4. Vérifier consentement à la collecte et provenance des données. Une capture
   native externe ne devient pas un reçu du runtime AR. Ne pas exporter de
   prompt privé, réponse brute, chemin personnel ou secret dans le dépôt.

Un plafond estimé ne prouve pas une borne de dépense. Le runner existant reste
désactivé pour les appels réels jusqu'à validation de son contrôle des envois.

## Une ligne assignée reste dans le bilan

| Donnée | Règle |
| --- | --- |
| Identité | Campagne, tâche, bras, répétition et tentative ; identité native ou AR avec provenance, sinon `null` |
| Résultat | Accepté, rejeté, inconnu, annulé ou invalide ; test et verdict humain restent distincts du succès de transport |
| Consommation | Inclure chaque tentative, son échec et les sondes payantes ; dédupliquer les snapshots, pas les tentatives |
| Coût | Prix versionné et portée explicite ; usage/prix absent rend le total complet UNKNOWN, avec sous-total connu et couverture |
| Ratio | Coût de toutes les tentatives du bras divisé par ses tâches acceptées ; zéro accepté donne un ratio indéfini |
| Temps | Temps machine, attente et temps humain séparés ; une durée sans bornes fiables reste UNKNOWN |
| Conseil | Recommandation et invocation observée séparées ; refus ou absence d'application conservés dans le bras assigné |

Pas de reprise automatique d'un appel au résultat ambigu. Consigner la tentative,
arrêter le bloc et réconcilier la dépense avant toute nouvelle autorisation.
Une panne de télémétrie ne justifie jamais de relancer une tâche déjà exécutée.
Arrêter aussi sur dérive de version/hash, modèle inattendu, identité perdue ou
dépense inconnue empêchant de respecter le plafond. Un échec fonctionnel reste
un résultat, pas une raison d'effacer la ligne.

## Passage à une confirmation

Après calibration, choisir les comparateurs et les conserver, y compris si E,
D ou S a gagné. Florian fixe les marges de qualité/coût et valide le corpus tenu
à l'écart. Sceller taille, analyse et règle de décision unique avant les appels.
Une règle statistique nécessite sa propre validation sur données simulées ;
tant qu'elle manque, le verdict d'adoption reste inconclusif. Aucun cas de
calibration ne rejoint le corpus de confirmation.

## Conservation et retrait

Les exports nettoyés d'un bloc vont dans `docs/bench/runs/<date>-<campagne>/`,
avec manifeste scellé, résultats, hashes et entrée du [playbook](../playbook.md). Le journal privé
existant garde l'autorité de ses propres exécutions. Un export est régénérable
depuis ses sources, jamais par un nouvel appel fournisseur.

Le retrait de ce candidat consiste à retirer ses deux fichiers s'ils sont
toujours identiques à la version écrite et sans auteur concurrent, puis son
lien dans le playbook. Conserver la référence et les preuves de validation
hors des fichiers supprimés ; ne supprimer aucun résultat d'une campagne.
