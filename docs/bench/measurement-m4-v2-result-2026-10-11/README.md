# M4 v2 : bilan des trois configurations, 11 octobre 2026

**156 434 tokens hôte consommés sur les 300 000 autorisés. Les trois configurations ont terminé avec des compteurs cohérents. Deux patchs passent les contrôles locaux ; le troisième est refusé avant application. Aucune économie générale n’est démontrée.**

La tâche réelle consistait à corriger le rapport du banc : le coût des tentatives invalides doit rester dans les statistiques de coût. Le [protocole scellé](../measurement-m4-v2/README.md), fusionné dans [#160](https://github.com/FlorianBruniaux/agent-router/pull/160), a été approuvé avant les appels. Son texte conserve l’état historique « à approuver » ; le [reçu d’accord](approval.json) et ce bilan enregistrent la suite sans modifier le scellement.

## Résultats

| Configuration | Modèle / effort | Tokens hôte | Patch exact | Contrôles locaux | Acceptation humaine |
| --- | --- | ---: | --- | --- | --- |
| Router, phase execute | `gpt-6-luna / medium` | 51 923 | [744 octets](patches/router.patch), applicable | 4 cas publics + 17 tests réussis | En attente |
| Sol fixe | `gpt-6.1-sol / medium` | 52 570 | [713 octets](patches/strongfixed.patch), refusé | Non lancés | Non accepté |
| Luna fixe | `gpt-6-luna / medium` | 51 941 | [744 octets](patches/cheapfixed.patch), applicable | 4 cas publics + 17 tests réussis | En attente |
| **Total** | **3 participants, 3 réponses** | **156 434** | **2 applicables, 1 refusé** | **21 contrôles par candidat applicable** | **0 accepté à ce stade** |

Les trois participants ont produit une seule réponse, sans outil, relance ni revue par un autre modèle. Aucun seuil individuel de 100 000 tokens n’a été dépassé. Le registre termine à `complete` : cela décrit la collecte des trois affectations, pas leur acceptation fonctionnelle ou humaine.

Les deux patchs applicables produisent exactement le même fichier, SHA-256 `129ae3e8932d7307151ca03692fb9301564e17a95ae20b5dccd5261a534d01b7`. Ils calculent les deux listes de coûts à partir de toutes les tentatives (`rows`), au lieu des seules tentatives valides (`valid`). Les octets hors de `Pilot.report()` sont inchangés. Les modifications restent dans les worktrees d’essai ; ce dossier ne livre pas le correctif dans le code du banc.

Sol propose la même correction logique à la lecture, mais son artefact exact ne se termine pas par un saut de ligne. `git apply --check` le refuse avec `error: corrupt patch at line 13` ([sortie](logs/strongfixed-apply-check.log)). Le protocole interdit de réparer le diff : il n’a été ni normalisé ni appliqué. Ce rejet mesure la livraison du patch sous ce contrat, pas une infériorité de raisonnement du modèle. Aucun résultat fonctionnel n’est attribué à Sol.

## Ce que les compteurs prouvent

Le précontrôle a passé les 18 tests du collecteur et du registre, puis retrouvé les 279 563 tokens de la trace v1 conservée. Pour chaque participant réel, la liaison session/parent/affectation, le modèle, l’effort, la fin de session et les compteurs par réponse ont été rapprochés du cumul final. Les trois sessions rapportent la version hôte `0.162.0-alpha.17.2`. Les [preuves projetées](result.json), [vérifications](verification.json), journaux de tests et empreintes sont conservés ici. Les traces brutes restent privées.

| Configuration | Entrée, cache inclus | Dont cache | Sortie, raisonnement inclus | Dont raisonnement |
| --- | ---: | ---: | ---: | ---: |
| Router | 51 675 | 17 152 | 248 | 54 |
| Sol fixe | 52 287 | 0 | 283 | 95 |
| Luna fixe | 51 610 | 45 824 | 331 | 137 |
| **Total** | **155 572** | **62 976** | **862** | **286** |

Le total additionne uniquement entrée et sortie. Cache et raisonnement sont déjà inclus ; ils ne sont pas ajoutés une seconde fois. L’hôte rapporte explicitement zéro pour `cache_write_input_tokens`. Ces compteurs ne vérifient ni l’identité des appels physiques fournisseur ni une facture.

Le prompt de tâche demandé mesure 7 648 octets et son empreinte est conservée. La plateforme injecte aussi ses instructions et schémas d’outils : le contexte total et le cache ne sont pas identiques entre participants. Le texte de tâche n’apparaît pas dans la trace enfant et l’argument correspondant est opaque dans le journal parent. L’égalité du contenu ne peut donc pas être revérifiée indépendamment depuis ces journaux. Les appels visibles documentent la demande ; cette limite ne constitue pas une dérive observée et ne rend pas les compteurs incohérents.

## Limites et suite

- Le dispositif simplifié a atteint les trois configurations, après l’arrêt de v1 avant tout participant. Les **279 563 tokens v1 restent une dépense antérieure distincte**. Préparation, coordination de cette conversation, publication et temps humain restent des coûts séparés, incomplets ou `UNKNOWN`.
- Router et Luna fixe utilisent le même modèle et le même effort. Une tâche publique et une réponse par configuration ne mesurent pas un avantage de sélection. Aucun prix ni pourcentage d’économie n’est publié.
- Les tests locaux prouvent cette correction bornée. Le plan complet du routeur, le runtime natif, l’isolation réseau/OS et un usage quotidien autonome restent hors de cet essai.
- Florian doit relire les [patchs Router](patches/router.patch) et [Luna fixe](patches/cheapfixed.patch), puis enregistrer son verdict. Jusqu’à cet accord, zéro patch est humainement accepté et le ratio de coût par patch accepté reste indéfini.
- Avant une campagne suivante, figer une règle de sérialisation du patch, décider du traitement du contexte/cache et retenir plusieurs tâches avec des configurations réellement distinctes. Le budget restant de v2 n’autorise aucune relance ni réparation de ces trois essais.

Le suivi reste ouvert dans [#5](https://github.com/FlorianBruniaux/agent-router/issues/5) et [#120](https://github.com/FlorianBruniaux/agent-router/issues/120). Aucun verdict natif ou fournisseur n’est déduit de cette projection de traces Codex.

## Relecture sans appel modèle

Depuis la racine du dépôt, avec Python 3.10 ou ultérieur et le commit de base disponible localement :

```sh
python3 -B docs/bench/measurement-m4-v2-result-2026-10-11/verify.py
```

Ce contrôle recalcule les empreintes, les sommes, le scellement et l’applicabilité des patchs exacts dans des dossiers temporaires. Il ne lance aucun modèle et ne remplace ni les journaux privés de collecte ni la revue humaine.
