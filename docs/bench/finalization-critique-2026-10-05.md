# Critique architecture et plan : Agent Router, 5 octobre 2026

Verdict : le plan donne une direction cohérente mais n'est pas exécutable tel quel.
Deux contrats manquants bloquent son parcours quotidien ; quatre défauts de découpage
et de validation risquent de prolonger le chantier. Ce sont des constats sur le plan,
pas la preuve que le produit est impossible ou que tous les comportements sont défectueux.

Revue par l'agent principal avec `critique-arch` et `critique-plan`. Elle porte
sur son propre plan antérieur et n'est pas une revue indépendante.
Aucun pourcentage de solidité ou de préparation n'est attribué : aucun barème
calibré ne permettrait de traiter ce chiffre comme une mesure.

## Périmètre et version auditée

Plan antérieur : `status-and-action-plan-2026-10-05.md`, 446 lignes,
SHA-256 `2b71b267a659f132ea9642502293bb68ecc86b51aa929362f60f83801ed6249a`.
Les passages cités ci-dessous identifient cette version, remplacée par le
[plan consolidé v3](status-and-action-plan-2026-10-05.md).
Sources du worktree au HEAD `9d77f5f7e1db8a7e5a45b2c5c5b3b9c3a619a981`
avec changements réseau locaux préservés. Aucun test produit ni appel fournisseur
n'a été exécuté pendant cette critique.

## Architecture résumée

Deux chemins : conseil et exécution native sous les permissions du host ; puis
supervision Agent Router avec contrôleur, broker et exécutant isolé. Les deux
peuvent partager une forme de rapport, mais leurs preuves n'ont pas la même origine.

Deux décisions sont à conserver : les contraintes filtrent avant classement ;
l'ADR 0022 impose une frontière entre outils et accès fournisseur. Le journal
existant reste la source des exécutions AR. Aucun nouveau daemon, proxy ou magasin
d'événements n'est nécessaire à la première expérience utilisateur.

## Constats hiérarchisés

### F1. Bloquant pour A3 : le mécanisme d'application n'est pas choisi

A3 promet « vérifier l'événement natif adapté par surface » puis « appliquer ou
décliner la recommandation ». Un événement, une surface et un contrat de délégation
ne sont pourtant pas épinglés. Le [hook actuel](../../harness/session-start.sh)
lance le harness, pas une décision produit. La [validation de cible](../../crates/router-adapter-api/src/execution.rs)
refuse `ParentConversation` à la ligne 651 ; l'[adaptateur Codex](../../crates/adapter-codex/src/execution.rs)
n'accepte que `HostPolicy` et prépare `codex exec` aux lignes 47 à 82.

Conséquence : construire la skill avant ce test peut produire des recommandations
sans mécanisme fiable pour appliquer le choix, contrairement au critère de fini.

Correction : U02 prouve un seul couple host/surface/version et les capacités
déclenchement, modèle/effort, corrélation, résultat et retrait. Un succès CLI ne
valide pas l'App. Si le mécanisme manque, garder le diagnostic en lecture seule
et marquer l'automatisation non disponible ; ne pas inventer une commutation.

### F2. Bloquant pour la mesure M1 : les preuves du parcours natif sont sans source définie

Le plan veut que « le journal/feedback reste la source d'exécution » tout en
évaluant un parcours natif hors du runtime AR. Le [writer feedback](../../crates/agent-router-cli/src/execution_feedback.rs)
lignes 21 à 85 exige un `RunCompletion`, une décision et un contexte validés ;
il lie le reçu, la définition, l'approbation et le run. Les mesures viennent du
completion courant, lignes 298 à 311. Ces objets ne sont pas automatiquement
produits par une recommandation dans une conversation ordinaire.

Conséquence : U07 ne peut pas promettre un coût ni une identité issus du runtime
AR pour une exécution que ce runtime n'a pas supervisée.

Correction : U03 définit deux provenances dans la projection expérimentale :
exécution AR liée à ses preuves ; exécution native externe liée à sa capture
autorisée et à son identité de session. Une jointure impossible donne UNKNOWN.
Le manifeste et l'export nettoyé du banc ne deviennent pas un second registre
d'autorité. Aucun faux reçu ni faux `RunCompletion`.

### F3. Majeur : A1 est une barrière trop large, A2 et A5 sont des ensembles de chantiers

Le plan dit « A1 débloque les implémentations » après réconciliation de chaque
exigence du projet. A2 mêle projection, timing, coûts et interface ; A5 mêle
protocole, broker, processus, OS, budget, reprise et validation du patch.
Un correctif de coût risque d'attendre un audit général ; une PR runtime peut
rester impossible à revoir indépendamment.

Correction : U01 fixe seulement les préimages, fichiers et critères du prochain
scénario. L'inventaire exhaustif continue séparément. U03 livre un rapport complet
minimal ; U04 et U05 ferment deux invariants financiers distincts. U13, U14 et U15
séparent composition locale, frontière d'envoi et seconde intégration native.
Chaque unité a une preuve observable et des dépendances explicites.

### F4. Majeur : « une session admissible » est une décision de conception, pas un raccord établi

Le plan autorise A-v2 à choisir une session sans expliquer comment cette décision
traverse le produit. Le preset [DirectSafe](../../crates/router-workflow/src/plan.rs)
lignes 74 à 94 crée `execute` ET `verify` : le nom ne prouve donc pas une
session modèle unique. Un test extérieur utilisant une session ne prouve pas que
le routeur peut sélectionner puis appliquer ce parcours.

Correction : U06 fait traverser au choix mono-session la décision, le lancement
natif autorisé et la mesure. Une vérification locale déterministe ne doit pas
introduire subrepticement un second appel modèle. La forme de décision est un
contrat à définir dans ce lot, pas un nouveau champ affirmé existant.

### F5. Majeur : protocole trop large avant preuve que la collecte est exploitable

Quatre comparateurs principaux, deux hôtes, coût, qualité, temps humain et deux
corpus peuvent multiplier les runs avant d'avoir une ligne fiable. Le plan
prévoit calibration et UNKNOWN, mais ne distingue pas suffisamment diagnostic
des composants, adoption personnelle et preuve statistique destinée à publication.

Correction : le premier bloc vérifie le trajet d'une tâche et son rapport.
La calibration compare routeur et mono-session E ; D n'est ajouté que s'il est
différent, S si une hypothèse économique précise est testée. La confirmation
scelle les comparateurs avant résultats. Aucun gain n'est conclu sur cette
calibration. Labels retrieval et acceptation de tâches restent deux corpus distincts.

### F6. Majeur : les modes dégradés et la restauration ne couvrent pas le runtime et les données

Le rollback détaillé concerne surtout la configuration globale. La reprise d'une
campagne, un schéma de rapport ajouté, l'échec d'une écriture feedback et une
réponse fournisseur perdue n'ont pas de règle opérationnelle assez précise.
Le writer actuel [signale l'échec du feedback](../../crates/agent-router-cli/src/execution_feedback.rs)
lignes 64 à 75 sans encourager le rejeu de l'opération native : il faut conserver
ce comportement.

Correction : collecte perdue = run conservé, mesure UNKNOWN et diagnostic ;
jamais relancer une action pour reconstruire son coût. Une tentative ambiguë
reste comptée/réservée selon la preuve disponible. Évolution additive des
rapports et lecture des anciens formats ; version précédente conservée ; aucun
downgrade d'une base privée non compatible. Suspendre les nouveaux envois avant
retrait d'un composant et ne toucher qu'aux processus possédés par ce run.

## Risques financiers déjà identifiés, confirmés par lecture

- `report.rs:386-405` déduplique au dernier événement par run. Ce n'est pas en soi
  une erreur ; le coût de tentatives distinctes doit être démontré sans compter
  plusieurs fois les snapshots.
- `report.rs:296-309` applique le prix input ordinaire aux écritures cache.
  `TokenPrice` dans `router-eval/src/replay.rs:48-52` a trois tarifs et aucun
  tarif cache-write distinct. La capture des tokens n'implique pas le prix exact.
- `replay_export.rs:44-73` exclut rejets et non-labellisés. Un bilan construit
  seulement depuis cet export ferait disparaître des échecs.

Ces limites requièrent des fixtures RED/GREEN avant changement. Les tests du
rapport doivent rester liés au writer et à la provenance, pas seulement vérifier
une formule isolée.

## Modes de panne et test opérationnel

| Scénario | Détection et diagnostic | Atténuation puis restauration |
| --- | --- | --- |
| CLI de conseil absente, lente ou invalide | Résultat d'intégration borné et motif visible, id de tâche si disponible | Continuer l'hôte sans changement de modèle ; désactiver l'intégration si défaut répété |
| Événement délivré deux fois | Même identité stable, compteur de déclenchements | Une invocation effective ; reprise sans rejouer l'effet |
| Résultat host sans usage ou sans identité | Couverture et provenance explicites | Garder le travail, ne pas conclure sur le coût ou le modèle |
| Feedback plein, inaccessible ou consentement retiré | Motif de non-enregistrement, preuve minimale permise | Ne pas refaire l'action ; marquer la collecte indisponible |
| Broker ou flux interrompu après envoi | Tentative commencée sans preuve de fin | Aucun retry caché ; réserver conservativement ; autorisation et tentative distinctes pour toute reprise |
| Mise à jour du host | Hash/version différents du contrat admis | Invalider la preuve concernée, repasser le probe, pas de repli vers des permissions plus larges |

La charge cible initiale est un utilisateur et une tâche active. Le premier
goulot à mesurer est le temps ajouté au déclenchement et le contexte répété,
pas le débit d'un service distribué. Les bornes temporelles exactes du hook
sont fixées depuis le contrat natif en U02 ; aucune latence n'est déclarée atteinte.
La concurrence est testée pour l'intégrité, pas vendue comme une capacité mesurée.

## Alternatives et arbitrages

| Option | Gain | Sacrifice et condition |
| --- | --- | --- |
| Conserver le modèle courant | Pas de fragmentation ni lancement supplémentaire | Aucun gain de sélection lorsque la route ne change pas ; mesurer l'overhead du conseil |
| Délégation native vérifiée | Premier usage sans construire le broker de production | Dépend du host ; ses permissions restent autoritaires ; aucun plafond financier AR garanti |
| Trois rôles de l'ADR 0022 | Frontière contrôlable entre fournisseur et outils | Protocole, IPC, OS et maintenance par version ; n'est requis que pour annoncer la supervision AR correspondante |
| Ajouter un compagnon propriétaire ou un daemon | Peut centraliser certaines interfaces | À refuser tant qu'une primitive native testée répond au scénario ; nouvelle surface d'installation et de panne |

Les changements de prix exigent une nouvelle table versionnée pour les campagnes
futures ; ils ne réécrivent pas les résultats historiques. La dépendance aux
interfaces expérimentales du host exige une matrice de versions et un refus
explicite en cas de dérive, pas un fallback silencieux.

## Verdict et trois premières actions

Architecture : exploitable sous conditions ; automatisation native et origine
des preuves restent à démontrer. Plan : à découper avant implémentation large.

1. Figer le scénario et prouver le contrat natif de déclenchement/application.
2. Livrer un rapport sans fournisseur qui conserve succès, échec et UNKNOWN.
3. Fermer les coûts par tentative et cache avant calibration payante.

La question structurante est : pour la surface réellement utilisée par Florian,
peut-on prouver à la fois le déclenchement, l'application d'un choix autorisé et
la collecte de son résultat ? Sans cette preuve, continuer à construire le
moteur ou le broker ne résout pas l'expérience quotidienne demandée.
