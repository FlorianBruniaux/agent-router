# Essai quotidien : `aradv`, un wrapper autour de votre propre host

[English](daily-trial.md) | Français

`integrations/daily/ar.py` permet d'utiliser `agent-router advise` sur chaque tâche réelle sans y penser, puis de regarder au bout de quelques semaines si `stable-v2` coûte moins cher que `stable-v1` en gardant des résultats acceptables. Vous lancez `aradv` à la place de `claude` ou `codex`. Le produit ne lance toujours rien et reste consultatif : `aradv` est vous qui exécutez votre propre host, avec les réglages de permission de ce host. La commande s'appelle `aradv` et non `ar`, parce que `ar` est l'archiveur système de macOS et de Linux.

Une exécution fait ceci :

1. Demande `advise --compare-policies` pour la même tâche sous `stable-v1` et `stable-v2`, avec `--declare-capability tools` (déclaré, jamais vérifié).
2. Choisit le bras : `--policy` s'il est donné, sinon un tirage 50/50 déterministe sur `request_hash` (dernier chiffre hexadécimal pair : `stable-v1`, impair : `stable-v2`). La même requête reçoit toujours le même bras. Le bras choisi et les deux plans sont affichés.
3. Lance une seule session du host sur le modèle d'exécution du bras, avec un effort plafonné (`--effort-cap`, défaut `high`). La session planifie et exécute elle-même. Pour un risque `high` ou `critical`, ou avec `--verify`, elle définit aussi un sous-agent `ar-verifier` sur le modèle de vérification. Avec `--dry-run`, elle affiche seulement l'argv en JSON.
4. Après la sortie du host, lit les transcripts Claude, estime le coût et contrôle les modèles qui ont vraiment tourné : la session principale ne doit avoir utilisé que le modèle d'exécution, et `ar-verifier`, s'il est demandé, que le modèle de vérification. Toute différence classe l'exécution en `overridden`, avec la raison affichée. Les autres sous-agents lancés d'eux-mêmes par la session sont listés.
5. Pose une question à une touche, enregistre le résultat avec `feedback advisory-record --followed` ou `--overridden` (avec `--used-model`, le premier modèle qui a tourné à la place : le magasin en exige un), ajoute une ligne à `runs.jsonl` et affiche les décisions enregistrées par politique.

## Pourquoi une seule session et pas un sous-agent de planification

La première version définissait un sous-agent `ar-planner` sur Opus et exécutait sur un modèle moins cher. Les mesures publiées depuis vont dans l'autre sens :

- L'équipe Applied AI d'Anthropic a constaté qu'un agent unique avec des skills bat souvent un orchestrateur avec sous-agents en qualité, en coût et en latence ; un sous-agent se justifie pour un contexte saturé, des tâches parallèles jetables ou des instructions contradictoires (https://claude.com/resources/articles/building-agents-when-to-fold-subagents-into-your-main-agent-loop, 2026-10-22).
- Le benchmark de `jev-router` (https://github.com/dominicrico/jev-router/blob/main/benchmarks/README.md, lu le 2026-11-14) trouve Sonnet seul à égalité avec Opus seul sur quatre tâches notées par tests cachés (38/40 chacun) pour 62 % de moins, un changement de modèle dans une longue session 42 % plus cher que de rester sur Opus, et un effort non plafonné jusqu'à 123 % plus cher.
- Les evals de `model-routing` (https://github.com/AqueGen/model-routing/blob/main/evals/README.md, lu le 2026-11-14) expliquent le coût d'une délégation : un sous-agent neuf paie l'écriture de cache pour tout ce qu'il lit, alors que la session principale paie la relecture, 12,5 fois moins chère.
- Le kit de migration d'Anthropic garde un sous-agent pour la « vérification indépendante » parmi ses bonnes raisons (https://github.com/anthropics/single-agent-loop-kit, lu le 2026-11-06).

Le bras décide donc du modèle et de l'effort de la session principale, choisis une fois au lancement, et un vérificateur indépendant n'est ajouté que là où le risque justifie son prix.

## Installation, une seule fois

Installez le binaire depuis la branche qui contient les catalogues voulus :

```sh
cargo install --git https://github.com/FlorianBruniaux/agent-router --branch <branche> --locked agent-router-cli
```

Créez le dossier privé et copiez les fichiers depuis une copie du dépôt (le dossier, `runs` et `store` ne doivent pas être modifiables par le groupe ni les autres ; `aradv` les crée en mode 0700) :

```sh
mkdir -m 700 -p ~/agent-router-advice
cp assets/catalogs/claude-core-v2.json assets/catalogs/codex-core-v3.json ~/agent-router-advice/
```

Écrivez `~/agent-router-advice/claude-host.json` avec les modèles et efforts que votre compte offre vraiment (Claude n'a pas de découverte passive ; les valeurs sont `declared`) :

```json
{
  "id": "my-claude-code",
  "harness": "claude",
  "supported_models": ["claude-fable-5-1", "claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"],
  "supported_efforts": {
    "claude-fable-5-1": ["low", "medium", "high", "xhigh", "max"],
    "claude-haiku-5-5": ["low", "medium", "high", "xhigh", "max"],
    "claude-opus-5-5": ["low", "medium", "high", "xhigh", "max"],
    "claude-sonnet-5-5": ["low", "medium", "high", "xhigh", "max"]
  },
  "capabilities": []
}
```

| Fichier sous `$AR_HOME` | Rôle |
| --- | --- |
| `claude-core-v2.json`, `codex-core-v3.json` | Catalogues (vous les copiez) |
| `claude-host.json` | Host Claude déclaré (vous l'écrivez) |
| `observation.json` | Instantané Codex, régénéré par `aradv` s'il manque ou date de plus de 20 heures |
| `store/` | Magasin de retours, créé à la première exécution enregistrée |
| `consent-preferences.json` | Écrit à la première exécution enregistrée si le magasin n'a pas de consentement |
| `runs/<horodatage>/` | `compare.json` et un dossier par politique pour chaque exécution |
| `runs.jsonl` | Une ligne par exécution enregistrée |

`$AR_HOME` est la variable d'environnement `AR_HOME`, sinon `~/agent-router-advice`. `AR_BIN` désigne le binaire `agent-router` s'il n'est pas dans `PATH`. L'instantané Codex lit `~/.codex/models_cache.json` ; `aradv` en passe le `client_version` comme `--observed-version`, écrit le nouvel instantané dans un fichier temporaire et ne le déplace que si `inspect` a réussi. L'horodatage de l'instantané est le `fetched_at` du cache : un cache que Codex n'a pas rafraîchi depuis 24 heures fait refuser `advise` avec `snapshot.stale`. Ouvrez Codex une fois, puis relancez.

Une fonction shell dans `~/.zshrc`, avec le script et `claude-pricing.json` gardés ensemble :

```sh
aradv() { python3 ~/agent-router/integrations/daily/ar.py "$@"; }
```

La première exécution enregistrée ne trouve aucun consentement de collecte et l'active (`collection_enabled` à true, `learning_enabled` à false, 90 jours), avec un message. Vous pouvez le retirer ensuite avec `feedback purge`.

## Usage quotidien

```sh
aradv claude --text 'Ajouter un drapeau --dry-run à scripts/backup.sh' --kind implement --risk medium --scope scripts
aradv codex  --text 'Relire le plan de migration' --kind review --risk high --scope docs --policy stable-v1
aradv claude --text 'Corriger le test de retry instable' --kind implement --risk low --scope src --dry-run -- --permission-mode plan
```

`--text`, `--kind`, `--risk` et `--scope` sont obligatoires. `--scope` n'a pas de valeur par défaut : `agent-router` refuse `.` et `./` avec `workflow.invalid` (vérifié le 2026-11-03 avec `--scope .`, `./`, `*` et `**`), et un périmètre est un fait que le produit ne définit jamais par défaut. Autres options : `--budget MICROUSD:MS:TOKENS` (défaut `0:1800000:400000`), `--skill ID` (répétable), `--policy stable-v1|stable-v2`, `--effort-cap NIVEAU` (défaut `high`), `--verify` ou `--no-verify` (défaut : vérificateur seulement pour `high` et `critical`), `--dry-run`. Les arguments après `--` vont au host, avant le texte de la tâche.

Ce que reçoit le host :

- Claude : `claude --model <modèle d'exécution> --effort <effort d'exécution plafonné> --session-id <uuid4> [--agents <json>] --append-system-prompt <texte> <tâche>`. Le prompt ajouté demande à la session de planifier et de faire le travail elle-même, et liste les skills recommandés. Avec un vérificateur, `--agents` définit `ar-verifier` (modèle de vérification, effort de vérification plafonné) et le prompt demande de l'appeler une fois avant de finir, sans lui passer de modèle ni d'effort.
- Codex : `codex -m <modèle d'exécution> -c model_reasoning_effort=<effort plafonné> <tâche>`. Pas de vérificateur.

À la sortie du host, `aradv` affiche `Advice: followed`, `Advice: overridden (<raison>)` ou `Advice: UNKNOWN (<raison>), recorded as followed`, puis demande `[a]ccepted as is / [r]ework needed / [x] abandoned / [s]kip`. Répondez sur le travail lui-même : le suivi du conseil est établi à partir des transcripts, pas par vous. Ignorer (`s`) n'enregistre rien, ni dans le magasin ni dans `runs.jsonl`. `aradv` rend le code de sortie du host.

## Lire le résultat après quelques semaines

```sh
agent-router feedback --store ~/agent-router-advice/store --json advisory-report
```

Lisez `groups` (une entrée par `policy_version` et `risk`, chacune séparée en `followed` et `overridden`), `paired_trials` et `interpretation_notes`. Les taux restent nuls sous 5 décisions par groupe (`minimum_sample`). `runs.jsonl` garde ce que le magasin ne compare pas : les tokens par modèle et le coût estimé. Coût estimé total par bras :

```sh
jq -s 'map(select(.cost_microusd != null)) | group_by(.arm) | map({arm: .[0].arm, runs: length, microusd: (map(.cost_microusd) | add)})' ~/agent-router-advice/runs.jsonl
```

Ne comparez le coût qu'entre bras de même mélange de types et de risques, et laissez hors de la somme les exécutions au coût `null` : elles sont UNKNOWN, pas gratuites.

## Comment le coût est estimé

Pour Claude, `aradv` lit `~/.claude/projects/*/<session-id>.jsonl` et `~/.claude/projects/*/<session-id>/subagents/*.jsonl`, garde les lignes `assistant`, déduplique par `message.id` (la dernière ligne l'emporte) et additionne `input_tokens`, `output_tokens`, `cache_creation_input_tokens` et `cache_read_input_tokens` par `message.model`. Les tokens enregistrés sont ces quatre valeurs additionnées. Le coût utilise `integrations/daily/claude-pricing.json`, des prix catalogue en microusd par million de tokens copiés de `docs/bench/campaign-next/pilot/pricing.json` et `pricing-cache-writes.json` (consultés le 2026-09-30), arrondis au microusd entier par modèle.

Les écritures de cache sont facturées au tarif 5 minutes. Si `cache_creation` porte `ephemeral_5m_input_tokens` et `ephemeral_1h_input_tokens` dont la somme égale `cache_creation_input_tokens`, la part 1 heure utilise le tarif 1 heure. Ces deux noms de sous-clés ont été lus sur un vrai transcript Claude Code par le mainteneur le 2026-11-03. Un modèle sans prix dans la table (aujourd'hui `claude-haiku-5-5`, dont les prix de cache ne figurent pas dans le dépôt) rend le coût de l'exécution `null`, tandis que ses tokens restent comptés.

## Limites

- L'échantillon est auto-sélectionné : vous choisissez les tâches, le bras est déterministe par requête, et vous pouvez abandonner ou ignorer. C'est un essai personnel, pas une expérience contrôlée.
- Le coût est une estimation à partir de prix catalogue et de transcripts, pas une facture. L'abonnement, les remises batch et les surcoûts de contexte long ne sont pas modélisés.
- Un `model` passé par la session à l'appel d'un sous-agent l'emporte sur le `model` de sa définition. Au premier vrai run (2026-11-04), une instruction globale qui impose de toujours fixer le modèle d'un sous-agent a fait lancer `ar-planner` sur Sonnet et `ar-verifier` sur Haiku au lieu d'Opus. Le prompt demande maintenant de ne pas en passer, et le contrôle des transcripts enregistre l'exécution en `overridden` si la session le fait quand même. Les efforts ne sont pas visibles dans les transcripts : un effort remplacé passe inaperçu. `CLAUDE_CODE_EFFORT_LEVEL`, s'il est défini, remplace tous les efforts passés par `aradv`.
- Sans transcript trouvé, et pour toute exécution Codex, le suivi est UNKNOWN et l'exécution est enregistrée comme suivie ; `runs.jsonl` garde `adherence: "UNKNOWN"` pour pouvoir l'exclure.
- Le coût et les tokens Codex restent UNKNOWN : ses fichiers ne sont pas lus. `advisory-record` est appelé sans eux.
- L'étape de plan du conseil n'est pas utilisée : la session planifie sur le modèle d'exécution. Pour une tâche critique dont l'étape de plan recommande un modèle plus fort, passez `--policy` ou lancez le host vous-même.
- Les qualités qui guident le choix sont des déclarations proposées ([ADR 0057](adr/0057-claude-core-v2-declared-ratings.md)), pas des mesures. Un meilleur coût sous `stable-v2` montre que les notes déclarées mènent à des modèles moins chers, pas que les résultats sont aussi bons ; le taux d'acceptation est votre propre jugement.
- Un `--policy` forcé est enregistré comme `forced` dans `runs.jsonl` ; excluez ces exécutions d'une comparaison non biaisée.
