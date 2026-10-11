# M4 v2 : trois réponses de patch, vérification locale

**Proposition à approuver. Aucun participant v2 n'a été lancé.** Cette version remplace le dispositif qui a consommé 279 563 tokens avant toute configuration, selon le [bilan M4](../measurement-m4-result-2026-10-10/README.md). Elle conserve la tâche T1 et les trois comparateurs, mais change explicitement la procédure : une réponse de génération de patch par configuration, puis tests locaux et revue humaine. Elle ne mesure pas un cycle autonome complet de développement.

## Configurations et budget

| Ordre | Configuration | Modèle / raisonnement | Allocation |
| --- | --- | --- | ---: |
| 1 | Agent Router, phase d'exécution sélectionnée | `gpt-6-luna / medium` | 100 000 tokens |
| 2 | Sol fixe | `gpt-6.1-sol / medium` | 100 000 tokens |
| 3 | Luna fixe | `gpt-6-luna / medium` | 100 000 tokens |
| | **Enveloppe proposée** | **3 participants, 1 réponse chacun** | **300 000 tokens** |

Chaque participant est créé directement depuis la conversation de suivi, avec rôle `default` et `fork_turns=none`. Aucun coordinateur modèle supplémentaire, aucune revue par un autre modèle, aucun retry, aucune escalade et aucun transfert de reliquat. L'ordre de la v1 est conservé. Les configurations restent séquentielles.

Router utilise seulement la phase `execute` de la [décision conservée](router-decision.json), sans exécuter ses phases `plan` et `verify`. Ce n'est donc pas une validation du plan complet du routeur. Router et Luna fixe sont identiques : toute variation entre eux ne prouve pas un meilleur choix de modèle. Les noms des comparateurs ne présument ni leur prix ni leur qualité.

Les 100 000 tokens par réponse sont un seuil proposé à partir des premières réponses déjà observées, dont celle du coordinateur à 50 825 tokens. Ce n'est pas une prévision ni un plafond garanti. L'unité reste entrée + sortie, cache inclus, raisonnement inclus. Un dépassement en cours de réponse reste possible et intégralement conservé. Les 279 563 tokens de la v1 restent une dépense antérieure distincte ; ils ne deviennent pas gratuits. Le suivi de cette conversation, la préparation, les tests et la revue humaine sont rapportés séparément, avec `UNKNOWN` lorsque non attribuables. Aucun prix en euros ou dollars n'est inventé.

## Réduction concrète du contexte

Le [prompt identique](prompt.md), de 7 648 octets, contient la tâche et l'extrait exact de `Pilot.report()`. Il ne contient aucun résultat d'un autre participant ni solution. La limite est de 16 384 octets pour le prompt de tâche et pour le patch retourné. Aucun historique de cette conversation n'est transmis.

Cette réduction porte sur les données de tâche. Les instructions et schémas d'outils ajoutés par la plateforme restent présents ; leur taille totale et une éventuelle réduction ne sont pas prouvées. La v2 réduit surtout le nombre de réponses répétant ce contexte.

L'approbation de cette version inclura une exception limitée aux trois participants : aucun outil ni chargement de skill depuis le disque, aucune commande, aucun sous-agent, une seule réponse finale contenant un diff unifié, sans footer. Cela ne modifie aucune configuration globale. Les instructions et la collecte du coordinateur humain restent applicables. Si l'hôte ne permet pas de respecter cette exception, arrêter au lieu d'improviser un autre transport.

## Admission et arrêt

Le [registre local](gate.py) vérifie le scellement avant toute transition, réserve une seule affectation et conserve son état sur disque. Une réservation reste bloquante après redémarrage tant que son résultat n'est pas enregistré. Un état arrêté ne peut pas être repris ni réinitialisé dans ce registre.

Après la réponse, le [collecteur](capture.py) vérifie la liaison session/parent/nom de participant, le modèle et l'effort, les compteurs par réponse, leurs cumuls et leur événement final. Les doublons identiques ne sont comptés qu'une fois. Les données manquantes ou incohérentes ne deviennent jamais zéro. L'absence d'un champ d'historique reste `not_declared` ; la demande `fork_turns=none` est une preuve distincte à conserver.

La configuration suivante est refusée si les compteurs sont inconnus, si le participant dépasse 100 000 tokens, si l'identité diffère, si un outil est appelé, si plusieurs réponses sont émises ou si la session n'est pas terminée. Un cas rejeté par les tests fonctionnels reste dans le bilan ; il n'empêche pas le comparateur suivant lorsque la collecte et le protocole sont valides.

**Ce registre ne lance ni n'interrompt de modèle.** Il ne peut pas empêcher un opérateur d'appeler directement l'outil de délégation. Son refus est déterministe et testé ; le respect de ce refus par l'opérateur reste une condition du protocole assisté. L'interruption pendant une réponse reste manuelle via l'hôte, sans garantie de plafond ni d'arrêt instantané. Aucun raccord automatique à `collaboration` n'est annoncé.

Le parent consulte les fichiers de collecte localement, sans envoyer au participant de messages de suivi, d'attente ou d'arrêt. Après sa fin, il attend au plus 30 secondes pour la dernière écriture de compteurs ; si elle manque, le bloc s'arrête. Un appel inattendu se traite par interruption quand possible et conservation de son coût, sans relance destinée à récupérer la télémétrie.

## Exécution proposée après accord

1. Revalider le scellement, les modèles/efforts disponibles, les permissions de l'hôte et la liaison exacte aux traces. Préparer trois worktrees propres sur `e65a22ecb0a294b67a18e7e7ba867b506aba427f` et une copie de confiance du vérificateur hors de ces worktrees. Les fichiers de v1 restent inchangés.
2. Créer un registre privé neuf lié au parent et à l'empreinte approuvée. Réserver Router. Conserver les arguments exacts de création, puis déléguer uniquement le contenu de `prompt.md`, avec le modèle et l'effort réservés. Aucun ajout propre à un comparateur.
3. Conserver la réponse sans correction. Résoudre le nouvel identifiant dans la base locale en lecture seule, avec parent et nom exacts, puis enregistrer la trace terminale. Une réservation seule n'est jamais une autorisation native.
4. Si l'admission reste valide, vérifier que le diff est borné, applicable sans réparation et limité à `Pilot.report()`. L'opérateur applique exactement ce patch dans le worktree correspondant. Un diff absent, hors périmètre ou inapplicable est un rejet, sans réparation ni second essai.
5. Lancer les quatre cas publics gelés et les 17 tests existants. Conserver les sorties, le diff et son SHA-256. Aucun lancement du pilote fournisseur. Répéter avec Sol fixe puis Luna fixe, uniquement si le registre autorise l'affectation suivante.
6. Florian relit le patch exact de chaque configuration et consigne le verdict. Les tests verts donnent une réussite mécanique ; l'acceptation reste en attente de cette revue. La revue n'est pas aveugle et aucun autre modèle n'est budgété pour la remplacer.

Les agents retournent du texte ; ils n'appliquent pas le patch et n'exécutent pas les tests. La nouvelle portée d'écriture est la seule méthode `Pilot.report()`, sans nouveau fichier de tests. Les quatre cas de référence sont publics et le vérificateur est détenu par l'opérateur ; aucune isolation OS n'est déduite de ces conventions.

## Mesure et interprétation

Le [manifeste](manifest.json) fixe les trois affectations avant tout résultat. Chacune doit apparaître dans le bilan, y compris `not_started`, interrompue ou rejetée. Conserver identité demandée/observée, usage complet, cache, empreinte du patch, tests, revue, durée des tests et temps humain s'ils sont effectivement mesurés. Les dépenses des échecs participent au numérateur ; avec zéro patch accepté, le ratio reste indéfini. Une réponse rejetée n'est jamais remplacée dans le dénominateur.

Le résultat répondra à une question bornée : sur cette correction publique, une réponse de chaque configuration fournit-elle un patch acceptable, et quelle consommation l'hôte rapporte-t-il ? Une tâche, une répétition et deux configurations identiques ne démontrent ni économies générales ni avantage de sélection. L'intégration native F08/F09, les factures fournisseur et la qualification OS/réseau restent séparées.

## Contrôles reproductibles sans modèle

Depuis la racine du dépôt :

```sh
python3 -B -m unittest discover -s docs/bench/measurement-m4-v2/tests -p 'test_*.py' -v
python3 -B docs/bench/measurement-m4-v2/gate.py --help
```

Le registre accepte `init`, `reserve` et `finish`, avec `--state` dans un dossier privé et `--approval-seal` fourni par l'opérateur après l'accord réel. `init` exige `--parent-thread-id` ; `finish` exige le chemin exact `--trace` et `--thread-id`. La connaissance du digest ne constitue pas un consentement : le message de Florian lié à ce digest doit être conservé séparément. Ne pas initialiser le registre réel pendant cette préparation.

Le [relevé de validation](verification.json) décrit seulement les contrôles réellement exécutés. [APPROVAL.md](APPROVAL.md) contient l'empreinte finale proposée. L'accord sur M4 v1 ne vaut pas accord sur cette version, qui change la procédure, la revue et le budget. La demande actuelle impose de présenter cette version avant toute relance.
