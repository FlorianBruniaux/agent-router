# Calibration v2 : vérifier la collecte avant de comparer

Ce candidat prépare U08 du [plan de livraison](../status-and-action-plan-2026-10-05.md).
Il ne lance aucune tâche. Le [manifeste à compléter](manifest.template.json)
est un document de préparation, pas un format accepté par la CLI ou par le
runner historique. Ses valeurs `null` restent des décisions manquantes.

Le [protocole du premier essai personnel](personal-trial.md) précise les trois
décisions humaines, les critères de réussite et les mesures à recueillir.
Il reste un candidat sans autorisation d'exécution.

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

Le retrait du protocole initial consiste à retirer ses deux fichiers s'ils sont
toujours identiques à la version écrite et sans auteur concurrent, puis son
lien dans le playbook. Conserver la référence et les preuves de validation
hors des fichiers supprimés ; ne supprimer aucun résultat d'une campagne.


## Validation hors ligne U08

Le validateur stdlib [validate_manifest.py](validate_manifest.py) lit un manifeste
et vérifie ses fichiers locaux, sans exécuter un test d'acceptation, appeler un
fournisseur, lire des credentials ou modifier le runner. La fixture
[synthetic/manifest.json](fixtures/synthetic/manifest.json) contient uniquement
des identités, commits, modèles, critères et fichiers synthétiques. Ses montants,
consentements et preuves d'admission restent `null` ou `UNKNOWN`.

Depuis la racine du dépôt :

```sh
python3 -m unittest discover -s docs/bench/calibration-v2/tests -v
python3 docs/bench/calibration-v2/validate_manifest.py docs/bench/calibration-v2/manifest.template.json
python3 docs/bench/calibration-v2/validate_manifest.py docs/bench/calibration-v2/fixtures/synthetic/manifest.json
```

Code de sortie : `2` pour un format ou un scellement invalide, `1` pour une
préparation valide avec entrées manquantes, `0` pour une préparation renseignée
et vérifiée hors ligne. Les deux commandes de validation ci-dessus rendent `1`.
Le [relevé machine des entrées du modèle initial](missing-inputs.json) est
régénérable avec la première commande de validation, en redirigeant sa sortie.
Il ne contient aucun choix humain inventé.

`offline_preparation_ready` constate la présence des déclarations et les hashes
locaux. Il ne vérifie ni le contenu probant d'un fichier, ni son auteur, ni la
fraîcheur de capacités natives, ni une autorisation humaine. `executable_ready`
reste toujours `false` ; `native_admission`, `authorization` et `benefit` restent
`UNKNOWN`. Aucun code de sortie ne débloque le runner historique, son plafond
ou ses contrôles. Une dépense et une installation globale exigent leurs propres
accords liés au candidat exact.

### Forme renseignée et vocabulaire conservé

Le modèle v1 reste inchangé et inerte. La forme renseignée v2 conserve chacun
de ses champs et de ses contrats de coût, d'identité perdue, d'arrêts et
d'analyse descriptive. La fixture montre les ajouts suivants :

| Champ | Préparation vérifiée |
| --- | --- |
| `document_type`, `template_version`, `status` | `campaign_preparation_manifest`, `2`, `sealed_preparation_not_authorized_to_run` |
| `synthetic` | `true` interdit une préparation réelle complète ; `false` reste une déclaration |
| `input_files` | Même clés que `input_hashes`, fichiers relatifs au répertoire du manifeste |
| `tasks`, `confirmation_tasks` | Champs v1, chemins et hashes du statement, script d'acceptation et tests cachés ; IDs et statements distincts entre groupes |
| `arm_model_effort_configurations` | Une entrée par bras, avec `model`, `effort`, `configuration_sha256` lié à la configuration nettoyée scellée |
| `sealed_order` | Une ligne `task_id`, `arm`, `repetition` par affectation, sans manque ni doublon |
| `preconditions` | U02b, U04b, U05, U06, U07, `send_control_verified`, `provenance_review` : `status`, `evidence_path`, `evidence_sha256` |
| `spending` | Champs v1, `estimated_remaining_usd` et `enforcement_evidence_path` ; coûts antérieurs inconnus bloquants |

Une précondition `UNKNOWN` conserve deux références `null`. Une déclaration
`verified` exige un fichier et son hash ; cette vérification locale ne constitue
pas une revue du contenu probant. Le consentement reste dans `collection_consent`.
Les refs de départ sont des SHAs Git complets ; leur existence, l'état propre et
le périmètre réel du dépôt doivent encore être vérifiés sur le trajet retenu.
Les tests cachés ne peuvent être désignés comme fichiers exposés, ni réutiliser
leur contenu exact. La séparation sémantique des solutions exige encore une
revue humaine. Réserver des tâches de confirmation dans ce document ne scelle
pas une règle statistique U10.

Le digest retourné est SHA-256 du JSON UTF-8 trié, sans espaces, avec
`spending.approved_manifest_sha256` mis à `null`. Renseigner ensuite ce champ
avec le digest approuvé rend la liaison vérifiable sans hash circulaire.
Un digest égal ne prouve pas qu'une personne l'a approuvé. Tout autre changement
invalide cette liaison. Aucun accord synthétique n'est conservé dans la fixture.

Le lecteur refuse clés inconnues ou dupliquées, nombres non finis, chemins
absolus, traversées et liens symboliques dans les fichiers scellés. Il borne le
JSON à 1 MiB, les fichiers non binaires à 16 MiB, les références
`input_files.host_binary` et `input_files.router_binary` à 256 MiB chacune,
et les lectures de fichiers à 512 MiB au total,
chaque groupe à 256 tâches, les répétitions à 32 et l'ordre à 8 192 affectations.
Les fichiers sont lus comme données et jamais exécutés. Les erreurs exposent
uniquement des champs et codes fixes, sans recopier chemins privés ou payloads.
Les hashes des binaires sont calculés par blocs de 64 KiB. Les montants
sont bornés à 9 223 372 036 854 USD, dont la valeur en microdollars tient
dans un entier signé de 64 bits ; cette limite de représentation ne constitue
aucun budget approuvé. Le répertoire du manifeste est ouvert une fois puis
conservé par descripteur pour lire le manifeste et ses fichiers scellés.
La résolution de ses ancêtres reste celle du système lors de cette ouverture
initiale. Le manifeste est ouvert sans suivre de lien et sans bloquer sur un
FIFO ; son descripteur doit désigner un fichier ordinaire. Les composants
relatifs des fichiers scellés ne peuvent être des liens symboliques.
Ces bornes conviennent aux exports et références de binaires scellées ; les
fichiers plus gros exigent un contrat révisé, jamais un contournement silencieux.
Les assertions d'export nettoyé ne remplacent pas une revue de confidentialité.

Pour retirer cet ajout, conserver les rapports de vérification, comparer ses
préimages et retirer seulement le script, ses tests, sa fixture et le relevé
machine si aucun auteur concurrent ne les a modifiés. Restaurer cette section
du README depuis sa préimage vérifiée. Ne supprimer aucun résultat de campagne.
