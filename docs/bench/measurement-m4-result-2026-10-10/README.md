# M4 : arrêt avant les trois configurations

Le 10 octobre 2026, le diagnostic M4 approuvé s'est arrêté après le coordinateur : **279 563 tokens annoncés par Codex, 0 configuration exécutée et 0 patch accepté**. [#159](https://github.com/FlorianBruniaux/agent-router/pull/159) a été fusionnée au commit `36af4bf0d84640b8e992905a8af46095f9d9cfa6`. Le [protocole v1](../measurement-m4/README.md) reste inchangé comme pièce historique ; ses mentions d'accord en attente décrivent sa préparation, avant l'accord explicite reçu dans la conversation.

## Résultat conservé

| Poste | Constat |
| --- | ---: |
| Allocation du coordinateur | 60 000 tokens |
| Enveloppe v1 des participants | 600 000 tokens |
| Premier dépassement observé | 107 633 tokens |
| Total terminal du coordinateur | 279 563 tokens |
| Tokens observés après le premier signal d'arrêt | 171 930 |
| Entrée, cache compris | 279 062 |
| Dont entrée en cache | 220 416 |
| Sortie, raisonnement compris | 501 |
| Dont raisonnement | 11 |
| Réponses comptées | 5 |
| Configurations exécutées | 0 sur 3 |

Le cache est inclus dans l'entrée et le raisonnement dans la sortie. Les 279 563 tokens ne sont ni un prix, ni une mesure du quota d'abonnement. Les montants fournisseur, les identifiants d'appels physiques et le total du projet restent inconnus. Les frais de préparation, le suivi dans la conversation principale et le temps humain sont des postes séparés incomplets, jamais des zéros.

Les trois affectations Router, Sol fixe et Luna fixe restent `not_started`, avec usage, patch et acceptation à `null`. Les trois worktrees sont propres sur `e65a22ecb0a294b67a18e7e7ba867b506aba427f`. La relation parent/enfant locale ne contient aucun enfant du coordinateur. Le coût par patch accepté est indéfini puisque son dénominateur est nul.

## Ce qui a fonctionné, ce qui a échoué

La collecte a relié le participant à sa trace et à son identité déclarée `gpt-6.1-sol/medium`. La somme des cinq réponses correspond exactement aux six compteurs cumulés, et le dernier événement de consommation donne le même total. Les deux traces historiques utilisées avant lancement satisfaisaient aussi ces contrôles ; les 10 tests des helpers privés ont été rejoués avec succès. Il s'agit de cohérence des compteurs de l'hôte, pas d'un rapprochement avec une facture fournisseur indépendante.

Le budget du coordinateur était mal dimensionné : sa première réponse représentait déjà 50 825 tokens, dont 50 613 d'entrée. Les 44 249 et 45 850 tokens observés sur les premières réponses des deux traces historiques signalaient un contexte initial coûteux. La répartition v1 prévoyait pourtant des phases à 20 000 ou 30 000 tokens. Ces observations ne prédisent pas exactement un autre modèle, mais auraient dû empêcher de présenter cette répartition comme praticable sans réserve.

L'observateur écrivait `STOP.json`, puis s'arrêtait lui-même. Il n'avait aucune liaison automatique vers l'interruption du modèle. Le coordinateur a encore produit trois réponses après le premier signal ; quatre appels d'outils apparaissent au total dans sa trace. L'interruption demandée par le parent est arrivée après sa fin. Le contrôle a empêché l'admission des configurations, mais n'a pas coupé immédiatement la consommation du coordinateur. Ce défaut de supervision appartient au dispositif d'essai, sans prouver un défaut du routeur de production.

La v1 interdisait le transfert des allocations inutilisées. Les 600 000 tokens globaux n'ayant pas été atteints, cela n'autorisait pas à ignorer le dépassement des 60 000 réservés au coordinateur. Aucun second coordinateur ni essai de remplacement n'a été lancé.

## Preuves et limites

- [Résultat structuré](result.json) : accord, commits, dénominateur complet, arrêt et valeurs inconnues.
- [Compteurs expurgés du coordinateur](coordinator-usage.json) : les cinq réponses, leurs cumuls, empreintes de trace et d'identifiants. Aucun prompt, chemin privé, secret ou quota de compte n'est publié.
- [Collecteur portable](../measurement-m4-v2/capture.py) : relecture exacte et bornée ; la version v2 ajoute le nombre d'appels d'outils et explicite l'origine d'historique non déclarée. Les empreintes des helpers originaux restent dans le résultat.

Le fichier `STOP.json` a été remplacé lors du rafraîchissement terminal. Le premier signal est conservé à partir de la sortie observée du moniteur, identifiée dans `result.json`, et n'est pas présenté comme un journal d'arrêt immuable. Le préfixe exact de 256 406 octets de la trace conservée reproduit toutefois son empreinte, ses deux réponses et son cumul de 107 633 tokens ; cette relecture est enregistrée dans `first_stop_trace_snapshot`. La trace ne déclare pas `history_base` ; cette absence ne prouve pas un contexte vierge. La demande de délégation utilisait `fork_turns=none`, et la comptabilité est rapprochée depuis le premier enregistrement disponible.

**Aucun gain, correctif utile ou parcours natif n'a été démontré.** La [proposition M4 v2](../measurement-m4-v2/README.md) retire le coordinateur modèle et réduit l'essai à trois réponses de patch, puis à une vérification locale. #5 et #120 restent ouvertes.
