# Routage consultatif d'une tâche réelle

[English](advisory-route.md) | Français

`advise` prend une tâche complète qui vous appartient et renvoie soit une
recommandation de modèle, d'effort et de skills par phase, soit un refus
expliqué. Le résultat se termine par la prochaine action manuelle à faire dans
votre host. C'est un conseil seulement : la commande ne lance aucun host ni
modèle, ne contacte aucun fournisseur, n'accorde aucune permission et
n'applique aucun budget. Cette première livraison est orientée Codex : le
catalogue fourni couvre les identifiants de modèles Codex. Tout catalogue et
toute description de host qui respectent les mêmes contrats sont lus de la même
façon ; ce parcours n'a pas encore été exécuté avec le catalogue Claude Code (#218).

La décision elle-même vient de la politique `route` existante. `advise` se
contente de convertir la tâche en requête de routage, d'étiqueter l'origine de
chaque fait et d'écrire la prochaine action manuelle.
L'[ADR 0051](adr/0051-advisory-task-route.md) (EN) consigne la conception.

## 1. Décrire la tâche

Créez un répertoire parent privé, puis laissez [`task-guide`](task-guide.md)
(EN) recueillir les faits. Le parent ne doit être modifiable ni par le groupe
ni par les autres utilisateurs.

```sh
mkdir -m 700 -p ~/agent-router-advice
agent-router task-guide --text 'Ajouter une option --dry-run à scripts/backup.sh' \
  --interactive --output-directory ~/agent-router-advice/backup-task --json
```

Exemple de réponses pour cette tâche : `implement`, `medium`, `internal`, un
objet budget, `["workspace_write"]`, une liste de critères d'acceptation,
`["scripts"]`, puis l'environnement cible `local`, les opérations
`["workspace_change"]` et `reversible` quand le normaliseur de risque les
demande. La commande exporte `input.json`, `risk-context.json` et
`normalization.json` dans le nouveau répertoire.

`task-guide` ne demande jamais les capacités de host requises. Si la tâche a
besoin d'outils, ajoutez `"required_capabilities": ["tools"]` à une copie de
`input.json` et relancez `task-guide --task <copie>`. Sans capacité requise,
`advise` ne vérifie aucune capacité du host et le dit dans son résumé.

## 2. Décrire le host

Choisissez une seule source de faits sur le host.

**Découvert** : lire le cache de modèles de Codex. La commande lit un fichier
local et ne lance rien. Fournissez la version complète de votre exécutable
Codex, observée indépendamment.

```sh
agent-router inspect --host codex --model-cache ~/.codex/models_cache.json \
  --observed-version <version de codex> --json \
  > ~/agent-router-advice/observation.json
```

Les modèles et efforts lus dans le cache sont `discovered`. Le cache ne dit rien
du support des outils : `tools` reste `unknown`.

**Déclaré** : écrivez vous-même un fichier `HostSnapshot` (voir le
[contrat de requête de routage](reference/json-contracts.md#route-request), EN).
Chaque valeur y est `declared`. Une capacité que vous ne listez pas reste
`unknown` ; elle n'est pas considérée comme absente.

```json
{
  "id": "my-codex",
  "harness": "codex",
  "supported_models": ["gpt-5.6-sol", "gpt-5.6-terra"],
  "supported_efforts": {
    "gpt-5.6-sol": ["high", "xhigh"],
    "gpt-5.6-terra": ["medium", "high"]
  },
  "capabilities": ["tools"]
}
```

## 3. Demander le conseil

```sh
agent-router advise \
  --task ~/agent-router-advice/backup-task/input.json \
  --risk-context ~/agent-router-advice/backup-task/risk-context.json \
  --catalog assets/catalogs/codex-core-v2.json \
  --host-snapshot ~/agent-router-advice/observation.json \
  --skill tdd \
  --output-directory ~/agent-router-advice/backup-advice
```

Utilisez `--host <fichier>` à la place de `--host-snapshot` pour un host
déclaré. `--skill` désigne une skill candidate et peut être répété. Ajoutez
`--json` pour le rapport versionné complet. Le code de sortie `0` signifie une
recommandation, `3` aucune recommandation ou des questions ouvertes, `2` une
entrée invalide.

## 4. Lire le résultat

Avec le cache de fixture assaini utilisé par les tests et une tâche qui exige
`tools`, rien n'est recommandé :

```text
Status: no_recommendation
  host.models [discovered]: [...]
  host.capability.tools [unknown]: unknown
Next manual action:
  Nothing was executed and no model is recommended for your codex host. Keep your current host choice, or supply one of the facts listed in would_change and run advise again.
  - host.capability.unknown tools: Supply authenticated host evidence for 'tools', or declare it with --declare-capability tools; a declaration is reported as declared, not verified.
```

Si vous savez que votre session host peut utiliser des outils, déclarez-le
explicitement :

```sh
agent-router advise ... --declare-capability tools
```

La même tâche obtient alors une recommandation :

```text
Status: recommended
  host.capability.tools [declared]: supported
Skill tdd: suggested
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Next manual action:
  Nothing was executed. In your codex host, select per phase plan: model gpt-5.6-sol at effort high with skills tdd; execute: model gpt-5.6-terra at effort medium with skills tdd; verify: model gpt-5.6-terra at effort high with skills tdd. Ratings come from catalog codex-core-v2, a declared policy prior, not a measurement. Support for tools is declared by you, not verified.
```

Pour chaque phase, sélectionnez ce modèle et cet effort dans votre host,
chargez seulement les skills listées et gardez les réglages de permission de
votre host. Quand une phase porte `fresh_instance`, lancez-la dans une nouvelle
session. Une déclaration retire le hash de l'observation de la décision, car la
décision ne dépend plus de l'observation seule. `network` ne peut pas être
déclaré (`advisory.declaration.network`).

## Provenance

| Étiquette | Sens |
| --- | --- |
| `declared` | Affirmé par vous, votre fichier de host, `--declare-capability` ou le prior du catalogue. Non vérifié. |
| `discovered` | Lu dans une observation de host dont l'empreinte et l'âge ont été contrôlés. Non authentifié. |
| `verified` | Contrôlé par cette commande, comme l'empreinte de l'observation, ou appuyé sur une preuve authentifiée. |
| `unknown` | Aucune preuve fournie. |

Les notes du catalogue sont toujours `declared` : `builtin-policy-prior` et
`builtin-unrated-policy-prior` sont des bornes de politique, pas des mesures de
qualité, de prix ou de latence.

## Correspondance des champs de la tâche

| Champ de la tâche | Routage consultatif |
| --- | --- |
| `objective`, `kind`, `acceptance`, `required_capabilities` | Routés sans changement |
| `risk` | Le risque effectif après le plancher du normaliseur est routé ; la valeur déclarée est rapportée à côté |
| `scope`, `budget`, `allowed_mutations`, `sensitivity` | Rapportés comme `not_enforced_by_advisory_routing`, avec leur valeur |
| `deliverables`, `constraints`, `dependencies` | Rapportés comme `not_enforced_by_advisory_routing` |

Rien n'est perdu en silence : chaque champ apparaît dans `mapping` avec son
traitement.

## Refus et limites

- Un modèle du host absent du catalogue est refusé avec `advisory.model.unknown` ;
  la commande ne devine jamais une note ni un alias.
- Une skill absente du catalogue est rapportée comme `absent_from_catalog` et
  n'est jamais suggérée. Une skill du catalogue n'est suggérée que si sa
  confiance, ses types de tâche et ses capacités requises conviennent et que le
  routeur l'attache à une phase retenue.
- Une tâche incomplète renvoie `needs_clarification` avec les questions
  ouvertes et n'écrit rien. Répondez-y avec `task-guide --task <fichier> --interactive`.
- Les enregistrements de `capability attest` n'alimentent pas `advise` : une
  attestation laisse le support de la capacité à `unknown` dans l'observation
  et n'est consommée que par le routage par phase avec une liste d'acceptation
  explicite.
- Il n'existe pas encore d'option d'index de recherche ; les skills viennent
  uniquement de `--skill`.

## Fichiers enregistrés

Sans `--output-directory`, rien n'est écrit. Avec cette option, la commande crée
un nouveau répertoire en mode 0700 et des fichiers en mode 0600, avec la même
publication sans écrasement que `task-guide`. Elle ne réutilise jamais un
chemin existant.

| Fichier | Contenu |
| --- | --- |
| `task.json` | L'entrée de la tâche telle que lue |
| `risk-context.json` | Le contexte de risque explicite, s'il est fourni |
| `route-request.json` | La requête exacte transmise au routeur |
| `decision.json` | La décision ; `explain --decision` la lit |
| `advice.json` | Le rapport complet, publié en dernier comme marqueur de fin |

`route --request route-request.json --catalog <catalogue> --json` rejoue la
même décision. Pour une observation non modifiée, passez aussi
`--host-snapshot`. Ces fichiers contiennent le texte de votre tâche : gardez-les
hors des dépôts publics.
