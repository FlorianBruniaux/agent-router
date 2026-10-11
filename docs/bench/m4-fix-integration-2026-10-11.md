# M4 : intégration du correctif accepté

Le 11 octobre 2026, Florian a demandé explicitement : « Vas-y, intègre le correctif M4 ». Le candidat Router est retenu pour l’intégration ; Luna fixe avait produit exactement le même fichier. Il s’agit d’un seul changement accepté, pas de deux succès indépendants.

Le [reçu d’acceptation](measurement-m4-v2-result-2026-10-11/integration-acceptance.json) lie cette décision aux empreintes du patch et du fichier obtenu. La PR d’intégration et son commit enregistrent la révision livrée. Le [bilan de l’essai](measurement-m4-v2-result-2026-10-11/README.md) conserve les résultats observés avant cette décision.

## Comportement corrigé

Le rapport du banc conserve désormais les coûts catalogue et produit des tentatives invalides dans le total et le coût par correctif accepté. Exemple synthétique : une tentative acceptée à 7 USD et une tentative invalide à 11 USD donnent 18 USD, au lieu de 7 USD. Un coût inconnu rend le total concerné inconnu ; sans correctif accepté, le ratio reste indéfini. Le décompte des résultats valides et acceptés ne change pas.

Les quatre nouvelles régressions échouent sur la version précédente et passent avec les deux lignes du patch exact. Les 17 tests existants du banc passent aussi. Les quatre cas du protocole initial passent sur le correctif intégré. Le vérificateur historique confirme toujours les empreintes, les trois captures et les 156 434 tokens de l’essai.

## Limites de mesure

L’acceptation intervient après l’essai, à la demande de l’utilisateur. Elle ne transforme pas les résultats en comparaison aveugle et ne répare pas le patch Sol refusé. Les trois tentatives restent dans le bilan, avec leurs consommations.

Le correctif porte seulement sur les champs de coûts catalogue/produit du rapport. Les autres agrégats existants de tokens et de coût déclaré par l’hôte gardent leur périmètre ; ce changement ne certifie pas une comptabilité complète des reprises ou des répertoires historiques exclus du rapport.

La facture fournisseur, la coordination, l’intégration et le temps humain restent incomplets ou inconnus. Aucun coût monétaire de bout en bout ni pourcentage d’économie ne peut être déduit. L’acceptation ne qualifie pas l’exécution native. #5 et #120 restent ouverts.

## Vérification locale

```sh
python3 -B -m unittest discover -s docs/bench/campaign-next/tests -v
python3 -B docs/bench/measurement-m4-v2/acceptance.py --repo . --task t1
python3 -B docs/bench/measurement-m4-v2/acceptance.py --repo . --task t1 --hidden docs/bench/measurement-m4-v2/public-cases.json
python3 -B docs/bench/measurement-m4-v2-result-2026-10-11/verify.py
```

Ces commandes ne lancent aucun modèle.
