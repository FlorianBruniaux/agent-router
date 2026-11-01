# Routage consultatif d'une tâche réelle

[English](advisory-route.md) | Français

`advise` prend une tâche complète qui vous appartient et renvoie soit une
recommandation de modèle, d'effort et de skills par phase, soit un refus
expliqué. Le résultat se termine par la prochaine action manuelle à faire dans
votre host. C'est un conseil seulement : la commande ne lance aucun host ni
modèle, ne contacte aucun fournisseur, n'accorde aucune permission et
n'applique aucun budget. La commande fonctionne avec les
catalogues Codex fournis et avec le catalogue Claude Code `claude-core-v1`,
déclaré par l'opérateur ; tout catalogue et toute description de host qui
respectent les mêmes contrats sont lus de la même façon.

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
  Nothing was executed. In your codex host, select per phase plan: model gpt-5.6-sol at effort high with skills tdd; execute: model gpt-5.6-terra at effort medium with skills tdd; verify: model gpt-5.6-terra at effort high with skills tdd. Catalog codex-core-v2: ratings come from the catalog, a declared policy prior, not a measurement. Support for tools is declared by you, not verified.
```

Pour chaque phase, sélectionnez ce modèle et cet effort dans votre host,
chargez seulement les skills listées et gardez les réglages de permission de
votre host. Quand une phase porte `fresh_instance`, lancez-la dans une nouvelle
session. Une déclaration retire le hash de l'observation de la décision, car la
décision ne dépend plus de l'observation seule. `network` ne peut être déclaré
ni par l'option ni dans un fichier de host (`advisory.declaration.network`). Une
déclaration qui contredit une preuve de `unsupported` est refusée
(`advisory.declaration.contradicts_evidence`), de même que toute déclaration avec
un catalogue généré qui embarque sa propre observation
(`advisory.declaration.embedded_observation`) : `route` relie toujours cette
observation, la requête enregistrée ne pourrait donc pas être rejouée.

### Exemple Claude Code

[`claude-core-v1`](../assets/catalogs/README.md#claude-code-policy-catalog) (EN)
est déclaré par l'opérateur : chaque entrée est non notée (qualité 0) et ne
déclare que `tools`, un prior côté modèle qui ne remplace pas un fait du host.
Aucune découverte passive des modèles ou efforts
Claude n'existe : décrivez le host dans un fichier déclaré, avec les
identifiants et efforts que votre compte propose réellement :

```json
{
  "id": "my-claude-code",
  "harness": "claude",
  "supported_models": ["claude-haiku-4-5-20251001", "claude-opus-5-5", "claude-sonnet-5-5"],
  "supported_efforts": {
    "claude-haiku-4-5-20251001": ["medium", "high"],
    "claude-opus-5-5": ["medium", "high", "xhigh"],
    "claude-sonnet-5-5": ["medium", "high"]
  },
  "capabilities": []
}
```

```sh
agent-router advise --task ~/agent-router-advice/backup-task/input.json \
  --risk-context ~/agent-router-advice/backup-task/risk-context.json \
  --catalog assets/catalogs/claude-core-v1.json --host ~/agent-router-advice/claude-host.json
```

Pour une tâche sans capacité requise, la sortie se termine par :

```text
Plan: claude-haiku-4-5-20251001/high -> claude-haiku-4-5-20251001/medium -> claude-haiku-4-5-20251001/high
Next manual action:
  Nothing was executed. In your claude host, select per phase plan: model claude-haiku-4-5-20251001 at effort high with no skill; execute: model claude-haiku-4-5-20251001 at effort medium with no skill; verify: model claude-haiku-4-5-20251001 at effort high with no skill. Catalog claude-core-v1: every selected model is unrated in the catalog, so this choice is a deterministic tie-break, not a quality recommendation. The task requires no capability, so no host capability was checked.
```

`selection_basis` vaut `unrated_tie_break` : à scores égaux, le routeur choisit
selon l'ordre des identifiants. Le résultat nomme un modèle, pas un meilleur
modèle. Une tâche qui exige `tools` n'obtient une route que si le snapshot du host
liste cette capacité ou si vous passez `--declare-capability tools`
(étiquetée `declared`). La déclaration `tools` du catalogue ne remplace pas ce
fait du host : sans lui, aucune recommandation.

## 5. Enregistrer le résultat

Une fois la tâche faite à la main, vous pouvez enregistrer si vous avez suivi la
recommandation et comment elle s'est terminée, puis lire un rapport local. Il
faut le `--output-directory` de l'étape 3, car l'enregistrement se lie au
`decision_id` sauvegardé dans `advice.json`. Rien n'est exécuté et rien ne sort
de votre machine.

```sh
echo '{"schema_version":1,"collection_enabled":true,"learning_enabled":false,"retention_seconds":2592000}' \
  > ~/agent-router-advice/prefs.json
agent-router feedback --store ~/agent-router-advice/store --json \
  configure --preferences ~/agent-router-advice/prefs.json

# Recommandation suivie, acceptée telle quelle, aucun coût rapporté par le host.
agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/backup-advice/advice.json \
  --adherence followed --result accepted-as-is

# Recommandation écartée : indiquez ce que vous avez utilisé, et le coût rapporté s'il existe.
agent-router feedback --store ~/agent-router-advice/store --json advisory-record \
  --advice ~/agent-router-advice/other-advice/advice.json \
  --adherence overridden --result accepted-after-rework \
  --used-model gpt-5.6-terra --used-effort medium --used-no-skill \
  --cost-microusd 4200

agent-router feedback --store ~/agent-router-advice/store --json advisory-report
```

La première commande est le consentement explicite : rien n'est enregistré tant
que `collection_enabled` n'est pas vrai, et `retention_seconds` borne la durée de
conservation. Ensuite :

- `--adherence` et `--result` sont obligatoires. `--result` vaut `accepted-as-is`,
  `accepted-after-rework` ou `abandoned`.
- Une recommandation écartée doit nommer au moins l'un de `--used-model`,
  `--used-effort`, `--used-skill` (répétable) et `--used-no-skill`. Ce que vous
  n'indiquez pas reste UNKNOWN. Une recommandation suivie n'en accepte aucun.
- `--cost-microusd` et `--tokens` sont ce que votre host a rapporté. Omettez-les
  s'il n'a rien rapporté : ils restent UNKNOWN et ne sont jamais stockés à zéro.
- Seuls des énumérations, des identifiants, des nombres et le `decision_id` sont
  stockés. Un identifiant est vérifié sur sa syntaxe, pas par rapport à un
  catalogue : lettres, chiffres et `._:-`, sans `/`, sans `\` et sans `.` ou `~`
  en tête. Le texte de la tâche, l'objectif, les chemins et tout commentaire ne
  sont pas stockés, et un texte libre ou un chemin dans une option est refusé
  avec `feedback.invalid`.
- Le type de tâche, le risque et les phases recommandées sont copiés depuis
  l'`advice.json` que vous passez, et vous pouvez le modifier. Seul le
  `decision_id` est un condensat de la requête : le regroupement par type de
  tâche repose donc sur votre propre fichier. Un écart qui indique un modèle, un
  effort et des skills tous égaux à la recommandation est refusé.
- Conseiller la même tâche une seconde fois donne le même `decision_id`. Répéter
  le dernier enregistrement est sans effet ; un résultat différent (reprise ou
  correction) est stocké comme tentative suivante, et le rapport compte une fois
  la dernière tentative.
- Un rapport `status: no_recommendation` n'a rien à suivre
  (`advisory.outcome.not_recommended`), et un fichier qui n'est pas un rapport de
  conseil de version 1 est refusé (`advisory.outcome.advice_invalid`).

`advisory-report` compare suivi et écarté par type de tâche. Chaque taux affiche
son numérateur et son dénominateur, la taille d'échantillon et les compteurs
UNKNOWN (coût, tokens et, pour les écarts, le modèle, l'effort et les skills
utilisés), ainsi que le nombre de recommandations issues d'un départage entre
modèles non notés. Un groupe vide n'a pas de taux, et non un taux nul. `decisions` et
`sample_size` comptent chaque décision une fois ; `records` indique le nombre de
tentatives enregistrées. Les coûts
sont comptés comme connus ou inconnus, jamais additionnés.

Pour partager une copie expurgée ou tout supprimer :

```sh
agent-router feedback --store ~/agent-router-advice/store --json export > outcomes.json
agent-router feedback --store ~/agent-router-advice/store --json purge
```

`export` liste les résultats sous `advisory_outcomes` ; ils ne contiennent aucun
texte libre. `purge` les supprime avec le reste du store.

**Un rapport personnel n'est pas une preuve de gain général.** Les
enregistrements sont déclaratifs et non vérifiés, l'échantillon est le vôtre et
auto-sélectionné, et les tâches diffèrent. L'évaluation comparative est suivie
dans l'[issue #5](https://github.com/FlorianBruniaux/agent-router/issues/5).
Voir [feedback](feedback.md#advisory-outcomes) et
[ADR 0052](adr/0052-advisory-outcome-record.md).

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
  ouvertes et n'écrit rien ; si `--output-directory` est fourni, le résumé
  indique que le répertoire n'a pas été créé. Répondez-y avec
  `task-guide --task <fichier> --interactive`.
- Un fichier de tâche de plus de 1 Mio est refusé avec `advisory.task.too_large`.
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
| `advice.json` | Le rapport complet, publié en dernier comme marqueur de fin. Il est identique au rapport `--json` imprimé, `output_directory` compris |

`route --request route-request.json --catalog <catalogue> --json` imprime les
mêmes octets que `decision.json` (testé) :

| Source du host | Rejeu |
| --- | --- |
| fichier `--host` | tel quel |
| `--host-snapshot`, sans déclaration | ajoutez le même `--host-snapshot` |
| `--host-snapshot` avec `--declare-capability` | tel quel, sans l'observation : la requête porte la surcouche et la décision n'est pas liée |
| catalogue généré avec observation embarquée | tel quel ; `route` relie l'observation embarquée, c'est pourquoi les déclarations y sont refusées |

Ces fichiers contiennent le texte de votre tâche : gardez-les hors des dépôts
publics.
