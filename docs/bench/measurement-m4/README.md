# M4 : premier diagnostic réel sur une correction utile

Protocole proposé le 10 octobre 2026, après la fusion de [#158](https://github.com/FlorianBruniaux/agent-router/pull/158) au commit `e65a22ecb0a294b67a18e7e7ba867b506aba427f`. Aucun agent de l'essai n'a été lancé. Le [manifeste](manifest.json) fixe la tâche, les configurations et le budget ; [APPROVAL.md](APPROVAL.md) donne l'empreinte à approuver. Cette approbation sera demandée après présentation du protocole, conformément à la demande de Florian.

## Question et tâche

Sur une petite correction réelle d'Agent Router, le choix du modèle et du niveau de raisonnement change-t-il la réussite, la consommation observée et le travail de vérification ?

La [tâche T1](task.md) corrige `Pilot.report()` dans le banc historique. Une exécution acceptée à 1 USD et une exécution invalide à 2 USD donnent actuellement un total de 1 USD. Le total et le coût par tâche acceptée devraient être 3 USD. Une dépense absente doit rester inconnue. Ces montants sont des entrées de test synthétiques, pas le prix de M4.

Le défaut est reproduit sur la base fusionnée : le cas public et les 3 cas complémentaires échouent comme attendu. Le [relevé initial](baseline.json) conserve les sorties. La solution n'est pas écrite pendant la préparation.

Périmètre autorisé pour l'exécutant : `docs/bench/campaign-next/pilot/run_pilot.py` et un nouveau test `docs/bench/campaign-next/tests/test_report_costs.py`. Le schéma, les comptes de réussite/rejet et les autres comportements restent identiques. L'inclusion des anciens dossiers de reprise est hors de cette tâche.

## Trois configurations, même procédure

| Configuration | Plan | Exécution | Vérification interne |
| --- | --- | --- | --- |
| Modèle puissant fixe | `gpt-6.1-sol/high` | `gpt-6.1-sol/medium` | `gpt-6.1-sol/high` |
| Modèle léger fixe | `gpt-6-luna/high` | `gpt-6-luna/medium` | `gpt-6-luna/high` |
| Agent Router | `gpt-6-luna/high` | `gpt-6-luna/medium` | `gpt-6-luna/high` |

La dernière ligne vient de deux appels locaux à `route` donnant les mêmes octets pour la [requête T1](router-request.json). La [décision conservée](router-decision.json) n'est pas remplacée pour forcer une différence. « Léger » décrit ici le modèle de référence proposé ; aucun prix ni avantage de qualité n'a été mesuré pour ces alias.

Router et Luna fixe sont donc identiques dans ce bloc. Une différence de résultat entre eux peut venir de la variabilité d'exécution ou du cache ; elle ne démontre pas un avantage de sélection. Le bloc vérifie surtout que la sélection peut être appliquée, suivie et évaluée. Les poids du catalogue restent des hypothèses non calibrées.

Ordre fixé avant résultat, avec la graine `20261010` : **Router, modèle puissant fixe, modèle léger fixe**. Une tâche, une répétition, 3 affectations. Pas de remplacement d'un échec par un autre problème, ni de recherche après coup d'une tâche qui favorise le routeur.

Chaque configuration utilise 3 sessions neuves, une par étape, avec le rôle `default` et `fork_turns="none"`. Les [instructions communes](phase-prompts.md), outils et critères sont identiques. Seul le plan de la même affectation passe à son exécutant. Aucun échange entre configurations. Une revue finale distincte, avec `gpt-6.1-sol/high`, reçoit chaque patch anonymisé et les mêmes critères, sans le résultat de la vérification interne.

Le coordinateur de l'essai est lui aussi neuf : `gpt-6.1-sol/medium`, sans historique hérité. Cette session de préparation reste hors des exécutants. Les 3 configurations s'exécutent séquentiellement. La séparation de contexte et de répertoire ne vaut pas une isolation OS des autres fichiers de la machine.

## Budget précis et limite de contrôle

Unité : **tokens annoncés par l'hôte, entrée + sortie**. Le cache est déjà inclus dans l'entrée et le raisonnement dans la sortie. Les tokens ne sont ni des euros ni une unité garantie du quota d'abonnement.

| Poste | Allocation |
| --- | ---: |
| Plan de chaque configuration | 30 000 tokens |
| Exécution de chaque configuration | 100 000 tokens |
| Vérification interne de chaque configuration | 30 000 tokens |
| Total des 3 configurations | 480 000 tokens |
| 3 revues finales indépendantes, 20 000 chacune | 60 000 tokens |
| Coordinateur neuf | 60 000 tokens |
| **Enveloppe des participants à cet essai** | **600 000 tokens** |

Au plus 9 démarrages d'étape, 3 démarrages de revue et 1 coordinateur. **Aucune reprise, aucune escalade, aucune réaffectation d'un reliquat.** Les étapes suivantes d'une affectation arrêtée restent non exécutées dans le bilan.

Ce budget est une règle d'admission et un seuil d'arrêt surveillé. L'outil de délégation ne fournit pas de plafond de tokens garanti par appel. Un appel en cours peut dépasser son allocation avant que le compteur soit observable ; le dépassement maximal est inconnu. Le coordinateur interrompt dès qu'il observe un dépassement et ne lance rien ensuite. Le dépassement est conservé, jamais tronqué pour faire respecter le tableau.

Les compteurs doivent être accessibles avant de poursuivre après la première étape. Une consommation manquante, ambiguë ou une identité modèle/effort absente bloque la suite. Si les périmètres parent/enfant peuvent se chevaucher, leur somme sert uniquement de charge opérationnelle conservatrice pour l'arrêt ; elle n'est pas publiée comme consommation réelle additionnable. Sans compteurs, aucune nouvelle étape n'est admise.

La préparation déjà engagée, les essais antérieurs et le suivi administratif dans cette conversation restent des postes séparés, incomplets lorsqu'ils ne sont pas attribuables. Ils ne deviennent pas gratuits et doivent apparaître dans tout bilan de retour sur investissement. Les 600 000 tokens couvrent les participants nommés du futur essai, pas tout le projet depuis son démarrage. Aucun participant M4 n'a encore été lancé.

Aucun achat de crédits ni transport API payant supplémentaire n'est autorisé par ce protocole. Le coût monétaire et la facture restent `UNKNOWN` : aucun tarif vérifié ne permet ici de convertir les alias de la délégation Codex en euros. Si un plafond monétaire ou un plafond de tokens strict est exigé, ce transport n'est pas prêt et l'essai ne démarre pas.

## Mesures et verdict

Pour chaque étape, conserver l'identifiant de session réellement retourné par l'hôte, l'identité demandée, l'identité observée, les limites temporelles et les compteurs cumulatifs complets. Ne pas prendre la seule dernière réponse comme consommation de la tâche. Les tokens mis en cache, les champs absents et toute remise à zéro restent visibles.

La collecte lit uniquement les traces explicitement liées aux participants de ce bloc. Les traces brutes, prompts et chemins privés restent locaux. Le rapport public ne conserve que les champs admis, les empreintes et les verdicts. [M2](../measurement-m123/m2/README.md) peut traiter les fenêtres exportées ; il ne constitue pas à lui seul un collecteur en direct. Son total combiné reste inconnu si un champ ou la preuve de disjonction manque. Une session Codex n'est pas transformée en appel physique fournisseur pour alimenter le calculateur natif.

Le [gabarit de résultat](result.template.json) garde les valeurs absentes à `null`. Il relie l'affectation à la base, au patch exact, aux résultats des tests et à la revue. Les fenêtres parent/enfant, la consommation conservatrice pour l'admission et la consommation réellement additionnable restent distinctes.

Acceptation d'un patch : les 4 cas gelés passent, les tests existants et les tests ajoutés passent, la revue finale valide le périmètre et tous ces éléments désignent le même patch. Un PASS déclaré par l'exécutant ne suffit pas. Après le verdict final, pas de correction dans ce bloc. Une révocation ultérieure reste une nouvelle preuve.

Tous les cas de vérification sont **publics** pour ce diagnostic. Le fichier historique s'appelle `hidden_cases.json`, mais ce nom n'en fait pas un test secret. Le vérificateur de référence est copié par le coordinateur hors des chemins d'écriture attribués ; ses empreintes sont comparées avant et après. Cela contrôle une dérive, sans prétendre démontrer une isolation OS.

Le bilan affiche les 3 affectations, y compris rejetées, interrompues et non exécutées. Mesure principale : acceptation et consommation complète par patch accepté, lorsque la couverture le permet. Avec 0 patch accepté, le ratio est indéfini. Les tests locaux, le délai total et le temps humain actif sont consignés séparément ; aucun taux horaire n'est inventé.

Une différence de tokens n'est pas automatiquement une économie financière. Une tâche répétée une fois ne suffit pas à conclure pour l'ensemble du projet. Aucune extension automatique aux 18 affectations M3 n'est autorisée.

## Préparation vérifiée et conditions avant lancement

Déjà vérifié hors ligne : base et fichiers sources, reproduction du défaut, catalogue/efforts déclarés, choix reproductible de Router et somme des allocations. Les permissions et identités fournisseur ne sont pas déduites de ce résultat. La requête M1 ne teste que la sélection des modèles : ses capacités vides ne certifient pas les outils nécessaires à la tâche.

Avant toute délégation réelle, le coordinateur doit :

1. Vérifier l'accord lié à l'empreinte dans `APPROVAL.md`, puis chaque fichier du scellement. Un changement de tâche, modèle, budget, règle ou entrée scellée demande une nouvelle version et un nouvel accord.
2. Vérifier les modèles/efforts encore annoncés et la fraîcheur des métadonnées, les outils locaux autorisés et les permissions effectives. La version CLI observée `0.159.2` ne prouve pas la version interne de la délégation desktop.
3. Préparer 3 checkouts indépendants de la base exacte et un dossier de preuves détenu par le coordinateur. Vérifier l'état initial et l'absence de solution précédente ; ne pas toucher au checkout personnel.
4. Établir le moyen de relier chaque nouvel identifiant de participant à sa trace locale et aux compteurs, sans lancer de probe modèle distinct. Cette liaison et le suivi en direct ne sont pas encore qualifiés. Si le transport ne les expose pas, arrêter avant la première étape de travail.
5. Enregistrer la décision d'admission, puis seulement lancer l'étape suivante. Après sa fin, vérifier identité, patch, compteurs et allocation avant toute autre étape. Ne jamais relancer pour reconstruire une télémétrie perdue.

L'accord sur ce protocole accepte seulement le diagnostic assisté et sa limite de budget surveillé. Il ne transforme pas une condition manquante en réussite. L'essai natif F08, ses preuves OS/réseau et le plafond par envoi de #132/#133 restent séparés. #5 et #120 restent ouvertes.

## Commandes de reproduction sans modèle

Depuis la racine d'un checkout de la base scellée, en utilisant les fichiers de protocole conservés à leur chemin absolu :

```sh
python3 -B docs/bench/measurement-m123/m3/acceptance.py --repo "$AR_ASSIGNMENT_DIR" --task t1
python3 -B docs/bench/measurement-m123/m3/acceptance.py --repo "$AR_ASSIGNMENT_DIR" --task t1 --hidden docs/bench/measurement-m123/m3/verifier/hidden_cases.json
python3 -B -m unittest discover -s docs/bench/campaign-next/tests -p 'test_*.py' -v
```

`AR_ASSIGNMENT_DIR` désigne le checkout de l'affectation, pas le checkout personnel. Les 2 premières commandes doivent échouer sur la base initiale, puis réussir sur un patch correct. La dernière s'exécute dans le checkout à vérifier. Pour le verdict, utiliser la copie de confiance du vérificateur conservée par le coordinateur. Aucune de ces commandes ne lance le pilote fournisseur.

Le [relevé de préparation](verification.json) rapporte seulement les contrôles réellement exécutés. La fusion d'une PR de documentation n'autorise pas l'essai réel.
