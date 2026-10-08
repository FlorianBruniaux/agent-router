# Finaliser Agent Router et mesurer son usage

Plan ouvert le 7 octobre 2026, mis à jour le 8 octobre. Statut : F04a, schéma d’observation et publication d’un patch simulé disponibles ; composition native, admission réelle et mesures restantes ouvertes.
Responsable de coordination : agent principal ; arbitrages de produit et acceptation finale : Florian.

La référence livrée est `5ba1a694daae90ddb03552f445f31465a64e8ed6` sur `main`, issue de la [PR #144](https://github.com/FlorianBruniaux/agent-router/pull/144). Elle livre la publication d’une correction simulée dans une révision Git dédiée. La PR #140 a livré le choix borné du client dédié ; la PR #139 a livré F04a et les diagnostics natifs. La [PR #138](https://github.com/FlorianBruniaux/agent-router/pull/138) a livré précédemment F07a. Le présent plan prolonge le [registre des preuves](status-and-action-plan-2026-10-05.md) et conserve les exigences de l'[acceptation complète](../remaining-acceptance.md). Il ordonne le travail sans annoncer son exécution ni accorder de nouvelles permissions.

Le [suivi d’annulation du relais absent](physical-send-feasibility-2026-10-07/outage-cancellation/README.md) teste un contrat distinct : le broker est indisponible avant le lancement natif, puis le contrôleur interrompt au premier signal de reconnexion corrélé. L’acquittement et le terminal `interrupted` sont corrélés au même thread/tour ; l’arrêt physique, les sorties, les handlers et les sockets sont vérifiés séparément. Zéro requête et zéro connexion fournisseur sont observés. Le verdict initial v2 et l’admission globale restent `NO_GO`. La perte du broker pendant une session, le plafond financier et la composition de production restent non qualifiés.

## Qualification locale du 8 octobre

Le [client dédié](owned-entry-qualification-2026-10-08/README.md) lie tâche, thread, tour, modèle, effort et workspace. Trois scénarios avec le vrai exécutable Codex épinglé et un fournisseur simulé passent : un envoi autorisé, un refus avec zéro envoi fournisseur et une annulation du relais absent avec nettoyage contrôlé. Les défauts de parsing, de liaison et de chronologie trouvés en revue ont des régressions ; les échecs initiaux sont conservés. Le [rapport de vérification](verification-owned-client-2026-10-08.json) lie les sources, contrôles et revues.

[ADR 0026](../adr/0026-owned-client-simulated-entry.md) retient ce client avec broker pour F02a simulé. Les neuf observations sur composants ACP épinglés montrent notamment une perte de JSON invalide ; elles ne constituent pas neuf sessions ACP natives équivalentes. #131 reste ouvert pour cette comparaison complète. Ce choix borné ne requiert pas d’attendre l’admission OS et financière finale, qui reste NO_GO. Le schéma `run-observation` couvre la forme v1 existante ; provenance native, durées et collecte restent à implémenter.

## Trois résultats à atteindre

1. **Essai personnel utilisable** : ouvrir une entrée dédiée une fois, soumettre une tâche utile sans commande Agent Router à chaque prompt, obtenir un patch vérifié et lire son bilan. La conversation Codex déjà ouverte ne change pas de modèle par cette intégration.
2. **Valeur mesurée** : comparer Agent Router au workflow fixe de référence sur les mêmes tâches et départs. Conserver les échecs, les reprises, les corrections humaines et les données manquantes. Un gain nul, une régression ou un résultat inconclusif sont des conclusions recevables.
3. **Livraison publiable** : installer, utiliser et retirer les artefacts exacts par Homebrew et Cargo, avec documentation et périmètre de support vérifiés. La clôture du projet complet exige aussi les critères MVP et le registre des exigences restantes ; une publication expérimentale ne les clôt pas.

## Point de départ et correction du plan précédent

Mise à jour de cette passe : [F04a](../run-observation.md) est implémenté, testé et revu localement : 12 tests runtime et 3 tests CLI ciblés passent, ainsi que 19 probes indépendantes sans blocage confirmé. Le [rapport lié aux sources](verification-observation-2026-10-07.json) enregistre 21 contrôles de qualité sélectionnés réussis, 1 633 résultats Rust workspace plus un contrôle du cœur minimal et 290 tests Python. Les nombres Rust additionnent les lignes de résultats, sous-processus compris, pas des définitions uniques ; les tests ciblés sont inclus. Les 42 tests natifs opt-in ignorés ne sont pas des succès et cette sélection ne vaut pas équivalence avec toute la CI. F04b et F05a complets restent ouverts ; les 20 références d’état affichées ne constituent pas une timeline native complète.

Le [diagnostic F03a](physical-send-feasibility-2026-10-07/README.md), revu indépendamment, conserve son NO-GO global : six cas natifs passent ; broker absent, zéro HTTP et timeout à 15 secondes laissent le résultat terminal UNKNOWN. Le verdict corrigé passe 12 tests et 4 probes ; 84 mutations de verdict ont été vérifiées. Le suivi d’annulation séparé permet maintenant de commencer F01r sur un client app-server dédié et épinglé, un broker HTTP et une annulation contrôlée. La qualification de ce candidat reste nécessaire avant un choix positif ; l’admission réelle et les gains ne sont pas déduits de ces résultats.

Le routage et l'explication hors ligne fonctionnent. Le [calcul par révision acceptée](../accepted-changes.md) est livré : il additionne les montants fournis, conserve les tentatives et les coûts incomplets, sans collecter lui-même la consommation native. Le [rapport local](verification-owned-entry-2026-10-07.json) enregistre 1 619 résultats Rust réussis, dont un contrôle du cœur minimal, et 290 tests Python réussis. Ce nombre Rust inclut des invocations de sous-processus, pas seulement des définitions uniques. Les 42 tests natifs optionnels ignorés ne sont pas des succès. Les 11 contrôles associés à la PR #138 ont été observés réussis lors du récap précédant ce plan.

L'[enquête sur l'entrée native](owned-entry-2026-10-07/README.md) conclut à un NO-GO pour app-server direct et ACP inchangés sous le contrat de contrôle de chaque envoi fournisseur. Les neuf scénarios comparatifs n’étaient pas exécutés dans cette enquête du 7 octobre. La comparaison par composants du 8 octobre apporte un contre-exemple ACP, sans compléter la parité de sessions. Un résultat négatif termine une enquête, pas la condition positive du premier essai.

La critique du plan précédent révèle quatre corrections nécessaires. Cette critique vient de l'agent principal, pas d'une revue indépendante ; aucun score de préparation n'est attribué.

- **Dépendance circulaire à retirer** : la faisabilité du contrôle des envois ne peut pas attendre le parcours natif qui en dépend. F03a démarre indépendamment ; F03b qualifie ensuite la composition complète.
- **Mesures à raccorder** : le calculateur hors ligne ne prouve pas que toutes les tentatives réelles ont été capturées. F04 à F07 raccordent et testent les sources avant les essais.
- **Fin du projet à définir** : le premier patch, le bénéfice mesuré et la publication ont des acceptations différentes. Les exigences historiques P1-P6 et U11-U18 restent suivies.
- **Documentation à réconcilier** : certaines pages et issues présentent encore des états historiques. F00 doit montrer l'état courant en tête, préserver les preuves anciennes et éviter un deuxième backlog contradictoire.

Les lectures GitHub du 7 octobre recensent 17 issues ouvertes, dont les deux parents #11 et #115. Les six corps #5, #11, #131, #132, #133 et #136 sont désormais réconciliés et relus après écriture, avec leur historique conservé et sans clôture. #131 porte `status:needs-evidence` ; #133 porte `phase:now` pour F03a. Les six précédents échecs HTTP 500 restent des observations historiques, pas un blocage actuel. Un label de priorité ne signifie pas qu’un agent travaille déjà sur le ticket.

## Unités de livraison et dépendances

Les identifiants F reprennent la roadmap. F01r nomme le réexamen positif après le NO-GO livré. Chaque ligne est un lot séparé ; une grande ligne se découpe en sous-lots vérifiables avant attribution de fichiers.

| Lot | Livre | Bloqué par | Validation de sortie |
| --- | --- | --- | --- |
| F00 | README, index, plan, registre et issues cohérents | Aucun | État présent distinct de l'historique ; issues ouvertes rattachées à un lot ; lecture après écriture des issues |
| F03a, [#133](https://github.com/FlorianBruniaux/agent-router/issues/133) | Décision sur le contrôle local de chaque envoi et reprise | Aucun | Fournisseur simulé avec compteur indépendant ; refus = zéro envoi ; toutes les reprises passent par le contrôle ; verdict GO/NO-GO |
| F01r, [#131](https://github.com/FlorianBruniaux/agent-router/issues/131) | Entrée dédiée retenue et frontière native précise | Point de contrôle F03a et annulation locale documentés, disponibles pour commencer ; sans attente de #132/F03b | Client app-server dédié épinglé + broker HTTP ; liaison tâche/thread/tour/modèle/effort et choix non admis refusé avant provider ; permissions, annulation et limites qualifiées pour le choix borné F02a ; neuf scénarios équivalents complets avant clôture de #131 |
| F04a puis F04b, [#116](https://github.com/FlorianBruniaux/agent-router/issues/116) | Une vue cohérente, bornée et expurgée d'une exécution | Aucun pour F04a ; F04a pour F04b | Lecture d'une transaction cohérente ; corruption, concurrence, données privées et limites testées ; magasin absent non créé |
| F05a, [#118](https://github.com/FlorianBruniaux/agent-router/issues/118) | Bilan chronologique lisible et JSON équivalent | F04a | Même identité de snapshot et même couverture ; aucune réparation ou relance à la lecture ; durées non prouvées UNKNOWN |
| F05b, [#117](https://github.com/FlorianBruniaux/agent-router/issues/117) | Durées par étape et tentative | F04a | Départ/fin sourcés ; horloge modifiée, redémarrage, interruption et borne manquante testés ; pas de durée inventée |
| F06a, [#120](https://github.com/FlorianBruniaux/agent-router/issues/120) | Corrélation minimale des tâches, tentatives, sessions et observations externes | F04a | Identités substituées, partagées, absentes et ambiguës refusées ou explicitement inconnues ; aucune conversion en faux reçu AR |
| F07b, [#5](https://github.com/FlorianBruniaux/agent-router/issues/5) | Alimenter le calcul livré F07a avec les observations qualifiées | F04a, F05a, F06a ; F05b pour les durées | Tentative sans feedback, échec, reprise, CI partagée et révision révoquée conservés ; coûts incomplets visibles ; anciennes sorties compatibles |
| F12a, [#11](https://github.com/FlorianBruniaux/agent-router/issues/11) | Composition réelle de l'hôte choisi avec la supervision et la sandbox | F01r positif | Identité native, accès nécessaires, refus, arrêt et limites OS observés ; un acquittement d'annulation ne vaut pas cleanup |
| F02a, [#132](https://github.com/FlorianBruniaux/agent-router/issues/132) | Une correction utile produite avec fournisseur simulé et vérifiée | F01r positif | Préimage, contenu, modes et périmètre liés ; doublon, concurrence, lien symbolique et interruption entre deux écritures testés |
| F02b, #132 | Le même parcours via l'entrée native dédiée | F02a, F12a, F04a, F06a | Session neuve, route appliquée, un résultat utile, refus conservés, retrait vérifié ; fournisseur encore simulé |
| F03b, #133 | Contrôle des envois intégré au parcours retenu | F03a, F02b | Concurrence au plafond, retries, perte de réponse, usage tardif et redémarrage ; réservation conservée quand l'effet est ambigu |
| F08p, #5 | Manifeste du premier essai concret | Préparation sans bloqueur ; finalisation après les preuves requises | Tâche, fichiers, tests, modèles/efforts, facturation/unité/plafond, dépense antérieure, collecte et arrêts liés au digest |
| F08, #5 | Premier essai réel puis calibration personnelle | F02b, F03b, F07b, F05b, F08p final et accord exact | Patch accepté par tests et revue humaine ; bilan complet ou incomplet explicite ; retrait réussi ; aucun doublon |
| F09, #5 | Comparaison sur tâches tenues à l'écart | F08 et protocole de confirmation approuvé avant résultats | Même départ par tâche, comparateurs conservés, ordre/répétitions scellés ; conclusion qualité/coût/temps avec limites |
| F12b, #11 | Matrice complète des hôtes/OS annoncés | F12a, puis admission propre à chaque cellule | Claude et Codex, macOS/Linux : preuves distinctes, refus/cleanup et tâche utile ; aucune équivalence CLI/App présumée |
| F13a, [#8](https://github.com/FlorianBruniaux/agent-router/issues/8), [#136](https://github.com/FlorianBruniaux/agent-router/issues/136) | Dossier de release et disposition sur la reproductibilité | Préparation sans bloqueur | Artefacts exacts, licences/notices, identité de signature ; égalité bloquante ; correction démontrée ou limite et décision explicites |
| F13b, #8 | Publication du périmètre choisi | F13a, validations du candidat et décision de publication ; F08 recommandé pour la première version utilisable | Acquisition HTTPS réelle, installation hors checkout, smoke, mise à niveau entre versions et retrait ; SHA et signatures vérifiés |
| Clôture complète, #11 | Fin du périmètre du projet | Critères MVP, P1-P6, F09, F12b, F13b et disposition explicite des exigences restantes | Matrice exigence → preuve → SHA sans case masquée ; aucune clôture obtenue en renommant un manque en option |

## Ordre d’exécution à partir du 8 octobre

1. **Qualifier la composition native F12a / #132.** La PR #144 a livré un résultat Git isolé ; ses protections Git, son contrôle d’autorité et le diagnostic d’arrêt ont reçu un correctif complémentaire, avec [24 contrôles locaux et revue indépendante](verification-publication-hardening-2026-10-08.json). La PR #145 est fermée comme doublon. Raccorder maintenant le client dédié à la supervision OS avant F02b. La publication préserve les fichiers utilisateur ; la sûreté complète de leur réécriture reste une exigence distincte de F02a. La comparaison ACP complète de #131 reste ouverte.
2. **En parallèle, raccorder les mesures.** Compléter F04b, F05a/F05b et F06a, puis alimenter F07b avec chaque tentative, erreur, durée et consommation disponible. F04a et F07a sont livrés et ne doivent pas être réimplémentés. Les données absentes restent UNKNOWN ; des événements de test ne prouvent pas une consommation réelle.
3. **Obtenir une correction utile, #132, puis préparer l’essai réel #5.** Le [candidat personnel](calibration-v2/personal-trial.md#candidat-concret-du-7-octobre-2026) vise un bilan humain `eval accepted-changes` de 20 lignes au plus, sans changer le JSON. Qualifier d’abord le parcours avec fournisseur simulé et supervision OS ; compléter F03b et la collecte avant l’inférence réelle. Le premier appel réel exige le manifeste final, son plafond et l’accord exact. La réussite demande un patch vérifié, les tests, l’acceptation du rendu et le bilan de toutes les tentatives.
4. **Comparer après l’essai d’utilisabilité.** Plusieurs tâches comparées au workflow fixe, avec affectation et critères scellés avant les résultats, peuvent établir qualité, coût et temps. Une première tâche réussie prouve seulement son périmètre d’usage ; elle ne démontre pas des économies. La préparation de la distribution F13a peut avancer séparément ; les exigences de publication restent celles de F13b.

La documentation et le changelog précèdent le prochain changement de code. Chaque livraison y ajoute ses preuves et ses limites, puis réconcilie les issues concernées sans clôture anticipée. Les 21 contrôles locaux consignés concernent la source de la PR #139 ; la CI distante n’a pas validé cette source, ses jobs étant empêchés par le budget/paiement ou annulés en conséquence.

Les preuves F01r permettent désormais le choix borné pour F02a simulé ; la comparaison complète de #131 reste ouverte. La chaîne de l’essai réel reste : F01r positif → F12a/F02a → F02b → F03b → F08. La chaîne de mesure F04a → F04b/F05a/F05b/F06a → F07b rejoint F08. Aucune flèche ne revient de F03a vers F02b.

Si F03a échoue, conserver l'exécution réelle désactivée pour le protocole actuel et publier le diagnostic borné. Étudier une interface native différente, un correctif de transport ou un protocole distinct à budget seulement suivi demande une décision explicite. Ne pas marquer le contrôle des envois vérifié pour débloquer l'essai.

## Essai concret et journal quotidien

Le [protocole personnel](calibration-v2/personal-trial.md) contient désormais un candidat concret pour une correction utile. Cette passe le référence seulement : aucun patch de ce candidat ni essai réel n’est annoncé. Son admission, son plafond et son accord final restent liés aux preuves exigées ci-dessous.

Réutiliser le [protocole personnel](calibration-v2/personal-trial.md) et son manifeste. Préparer un candidat sur une petite correction ou amélioration de documentation réellement nécessaire, avec fichiers autorisés et test d'acceptation indépendant. Ne pas inventer de bug pour fabriquer une réussite. Florian approuve le candidat concret et son plafond une fois les entrées techniques et les preuves prêtes.

Pour un début utilisable, une tâche active suffit. L'entrée dédiée doit router les tâches sans commande répétée ; l'approbation du travail et l'acceptation du patch restent visibles. Un hook de conseil éventuel ne sera pas présenté comme l'application automatique du choix. Claude passe ses propres contrôles : une réussite Codex n'autorise pas l'autre hôte.

Le premier essai peut être un test d'utilisabilité. Il n'entre dans une comparaison que si son affectation et son ordre ont été scellés avant l'appel. La calibration compare A-v2 au modèle fixe E ; le workflow habituel D reste présent s'il diffère. Aucun modèle ou effort précis n'est fixé avant la découverte des capacités sur la version retenue.

Réutiliser `docs/bench/runs/<date>-<campagne>/` pour les exports nettoyés et [playbook.md](playbook.md) pour le journal indexé. Les traces privées restent dans le stockage privé existant ; ne créer ni second collecteur ni base supplémentaire. Ne pas versionner prompts privés, réponses brutes, secrets ou chemins personnels.

| Donnée par tâche | Règle de mesure |
| --- | --- |
| Qualité | Test indépendant et verdict humain de la révision exacte ; rejet, abandon et UNKNOWN conservés |
| Modèle/effort | Conseil, demande et valeur observée séparés ; ne pas déduire l'identité fournisseur d'un nom demandé |
| Tentatives | Affectation enregistrée avant envoi ; tous les échecs, retries et escalades ; déduplication des observations seulement |
| Temps | Machine, attente, tests, revue et correction humaine séparés ; somme ou durée murale selon la définition scellée ; bornes absentes = UNKNOWN |
| Coût | Usage observé, catégorie de cache, prix daté et couverture ; estimation API, quota d'abonnement et facture séparés |
| Coût par accepté | Tout le travail du groupe, y compris échecs, divisé par les révisions sélectionnées acceptées ; zéro accepté = ratio indéfini |
| Automatisation | Déclenchements attendus/observés, doublons, refus, interventions manuelles et retrait |

Après calibration, fixer les marges de qualité/coût/temps et la règle d'analyse avant la confirmation. La taille et les répétitions dérivent de la calibration ; ce plan n'invente pas un nombre de tâches qui garantirait un résultat. Corriger la contradiction déjà recensée entre tests statistiques et intervalles du [protocole historique](router-value-protocol.md) avant une conclusion inférentielle. Sans méthode validée, rapport descriptif et adoption inconclusive.

Arrêter les nouveaux appels sur version/hash inattendu, modèle non admis, identité perdue, résultat ambigu, compteur discordant ou consommation inconnue empêchant le plafond. La perte de télémétrie ne justifie aucune répétition du travail. Une baisse des tokens ne prouve pas une baisse de facture d'abonnement.

## Distribution, performances et exigences complètes

Homebrew local et Cargo depuis Git local sont déjà qualifiés sur les artefacts consignés dans la [preuve de distribution](distribution-qualification-2026-10-07.md). Restent l'acquisition HTTPS, le préfixe Homebrew destiné aux utilisateurs, une mise à niveau entre deux versions et la qualification du candidat final. Cargo depuis Git épinglé demeure le premier canal ; crates.io n'est pas une condition ajoutée à cette livraison.

Les [quatre reconstructions](reproducibility-investigation-2026-10-07.md) correspondent à une source historique. Elles ne qualifient pas un futur binaire et n'expliquent pas la première divergence dont les fichiers ont été perdus. Ne pas relancer une campagne identique sans nouvelle condition à tester. Sur une divergence future, conserver les binaires et l'invocation réelle de l'éditeur de liens avant toute correction ; ne pas normaliser les différences pour faire passer la gate.

Le candidat final doit conserver les contrôles de licences/notices/SBOM, secrets et permissions, la signature de publication avec une identité décidée, et les preuves du support annoncé. La licence actuelle reste FSL-1.1-ALv2. La décision sur la licence de diffusion, la visibilité du dépôt et l'identité de signature précède la publication ; ce plan ne les modifie pas.

Le seuil historique de démarrage à froid de 150 ms reste non satisfait dans le registre. Mesurer le candidat dans des conditions consignées, corriger une cause identifiée ou faire accepter explicitement une révision du critère. Ne pas remplacer une mesure froide par une mesure chaude ni désactiver une protection OS.

Pour la fin complète, réconcilier le registre P1-P6 et les exigences R/O/U restantes avec les sources actuelles. Le corpus de pertinence agents/skills nécessite de vrais labels humains et des cas tenus à l'écart ; les fixtures générées ne les remplacent pas. Les garanties de mémoire et de cleanup, le support des deux hôtes et les preuves Linux ne se déduisent pas du nombre de tests macOS. Les anciennes exigences déjà satisfaites doivent être rattachées à leurs preuves, pas réimplémentées.

## Toutes les autres issues ont une suite explicite

| Issue | Déclencheur et livraison attendue |
| --- | --- |
| [#115](https://github.com/FlorianBruniaux/agent-router/issues/115) | Parent : fermer quand #116 à #120 satisfont chacun leur acceptation ; ne bloque pas intégralement le premier essai |
| [#119](https://github.com/FlorianBruniaux/agent-router/issues/119), F10 | Après F04b/F05a, règles de diagnostic avec preuves requises, suppressions et identités stables ; F05b avant les règles temporelles ; aucune action automatique |
| [#120](https://github.com/FlorianBruniaux/agent-router/issues/120), F06b | Après F04b et un consommateur identifié, exporter la projection sous consentement ; canaris privés absents de la sortie, autorité inchangée |
| [#130](https://github.com/FlorianBruniaux/agent-router/issues/130), F11 | Après le parcours de base, spécifier puis tester les permis locaux de tests/CI/revue ; dette conservée au redémarrage jusqu'au règlement prouvé ; intégration distante qualifiée séparément |
| [#124](https://github.com/FlorianBruniaux/agent-router/issues/124) | Décider la surface Claude et la pertinence d'un mod parent ; tests propres de rendu, modèle/effort, permissions et retrait. Cette extension ne remplace pas la qualification de l'adaptateur Claude |
| [#122](https://github.com/FlorianBruniaux/agent-router/issues/122) puis [#123](https://github.com/FlorianBruniaux/agent-router/issues/123) | Après la référence mesurée F09, prototype Pixel lié au snapshot, puis comparaison même tâche/modèle ; inclure indexation, reprises et cas manquants ; adopter, réviser ou rejeter selon les résultats |
| [#11](https://github.com/FlorianBruniaux/agent-router/issues/11) | Parent : suivre les unités, les décisions de périmètre et les exigences historiques ; ne fermer ni à la première publication ni au premier benchmark |

Ces lots restent dans le backlog et ne bloquent pas tous l'essai initial. Leur report n'est pas une clôture. Les fonctionnalités hors de la version choisie doivent être identifiées comme telles dans le support public.

Après stabilisation du contrat de transport et du bilan, reprendre l'échange déjà demandé avec la session cc-bridge. Comparer intégration et fusion avant toute modification des deux projets. L'export et le transport restent des responsabilités distinctes ; aucun regroupement de dépôts n'est décidé par ce plan. Windows, interfaces graphiques et apprentissage actif conservent leurs critères et décisions propres dans le registre complet.

## Répartition prévue des agents

Au maximum trois sous-agents actifs avec le coordinateur. Les rôles tournent selon les dépendances ; ne pas lancer des implémenteurs concurrents sur les mêmes fichiers.

| Responsabilité | Agent et réglage proposé | Livrable de revue |
| --- | --- | --- |
| Frontière native, envois, choix de surface | `system-architect`, `gpt-6-astra`, high | Propriétaire de chaque effet, graphe sans cycle, GO/NO-GO et contrats |
| Projection et raccord des mesures | `backend-architect`, `gpt-6.1-sol`, high | Contrats versionnés et tranches compatibles avec les stores existants |
| Exploration puis implémentation Rust | `rust-explorer` medium, puis `rust-implementer` high, `gpt-6-astra` | Chemins attribués, cas RED puis correctif minimal et GREEN |
| Contrats et validation adverse | `rust-test-engineer`, `gpt-6-astra`, high | Échecs reproduits, assertions sur effets observés, preuves séparées du code |
| Revue indépendante | `rust-code-reviewer` ou `rust-security-reviewer`, `gpt-6-astra`, high | Défauts confirmés, réfutés et non vérifiés ; limites d'autorité et verdict |
| Docs, issues et qualification documentaire | `technical-writer`, `gpt-6.1-sol`, medium | État courant, commandes essayées, liens et limites de support |

Séquence restante : durcissement du résultat Git simulé, compléments d’observation F04b et documentation F00 ; F12a qualifie en parallèle la supervision OS avant F02b. Les résultats locaux F03a et F04a sont conservés, sans transformer leur portée en admission globale. Avant toute modification, assigner les chemins possédés et prévenir que d'autres agents travaillent dans le dépôt. Le reviewer d'un lot n'en est pas l'auteur. Utiliser Agent Router comme conseil de sélection seulement si son catalogue et la requête représentent ces tâches ; enregistrer sa décision, sans lui attribuer la création des sous-agents ou une autorité qu'il n'a pas.

## Tests, livraison et retour arrière

Chaque changement de comportement commence par un scénario qui échoue sur l'ancien comportement. La validation suit le risque : fixtures ciblées, tests d'intégration, revue indépendante, puis gates du projet sur la révision figée. Les tests à sockets et les contrôles OS s'exécutent dans l'environnement requis, sans confondre refus du sandbox de développement et bug produit. Les tests purement documentaires vérifient références et commandes ; ils ne justifient pas une nouvelle campagne fournisseur.

Conserver les commandes, SHA, hashes, sorties et échecs initiaux dans les rapports locaux. Utiliser `scripts/check-local.py` pour les groupes applicables après lecture de son aide ; formatage, Clippy avec warnings refusés, tests workspace, harness et secrets restent les gates Rust. Vérifier la disponibilité d'un audit de dépendances avant de le lancer ; ne pas installer de nouvel outil par principe. Les résultats CI distants restent une preuve séparée, même quand les gates locales passent.

Pour chaque lot terminé : mettre à jour les docs et l'issue, faire un commit conventionnel signé, pousser la branche dédiée, ouvrir une PR liée aux preuves puis fusionner dans le cadre de l'autorisation existante. Vérifier le SHA et l'arbre de `main` après fusion. Une preuve n'est pas réutilisée après une modification pertinente de ses sources. Le checkout principal reste préservé.

Le retrait suit l'objet modifié : désactiver l'intégration possédée et vérifier une session neuve ; arrêter un producteur de format défectueux sans rétrograder la base privée ; retirer un paquet selon son inventaire sans supprimer les fichiers de l'utilisateur. Avant restauration, exclure les écritures concurrentes, comparer chaque fichier à la dernière version écrite, restaurer ses préimages et retirer uniquement les nouveaux fichiers inchangés. En cas de dérive, préserver l'état et réconcilier manuellement. Les preuves demeurent hors des fichiers retirés. Une exécution ambiguë conserve sa réservation et n'est pas rejouée automatiquement.

Les changements globaux suivent le protocole existant : inventaire et hashes, candidat inerte testé, diff expurgé et digest, accord exact, nouvelle comparaison des préimages puis écriture et vérification. Préparer tous les éléments avant de demander cet accord. Aucune activation globale, campagne payante, modification de licence ou publication n'est accomplie par ce plan.

## Critères de fin observables

- **Essai personnel terminé** : une tâche réelle acceptée, entrée automatique dédiée et modèle appliqué prouvés, absence de doublon, bilan relié à la révision et retrait réussi.
- **Évaluation terminée** : toutes les affectations conservées, calibration séparée, confirmation scellée et conclusion explicite, y compris absence de gain ou résultat inconclusif.
- **Version expérimentale livrée** : artefacts exacts et périmètre approuvé, acquisition/installation/retrait vérifiés, documentation fidèle, publication vérifiée après l'action.
- **Projet complet terminé** : les dix critères MVP et P1-P6 ont leurs preuves ; exigences restantes implémentées ou changements de périmètre explicitement acceptés ; aucun UNKNOWN obligatoire présenté comme réussi.
