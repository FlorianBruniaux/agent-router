# Essai quotidien : `ar`, un wrapper autour de votre propre host

[English](daily-trial.md) | Français

`integrations/daily/ar.py` permet d'utiliser `agent-router advise` sur chaque tâche réelle sans y penser, puis de regarder au bout de quelques semaines si `stable-v2` coûte moins cher que `stable-v1` en gardant des résultats acceptables. Vous lancez `ar` à la place de `claude` ou `codex`. Le produit ne lance toujours rien et reste consultatif : `ar` est vous qui exécutez votre propre host, avec les réglages de permission de ce host.

Une exécution fait ceci :

1. Demande `advise --compare-policies` pour la même tâche sous `stable-v1` et `stable-v2`, avec `--declare-capability tools` (déclaré, jamais vérifié).
2. Choisit le bras : `--policy` s'il est donné, sinon un tirage 50/50 déterministe sur `request_hash` (dernier chiffre hexadécimal pair : `stable-v1`, impair : `stable-v2`). La même requête reçoit toujours le même bras. Le bras choisi et les deux plans sont affichés.
3. Lance le host avec ce bras, ou affiche seulement l'argv en JSON avec `--dry-run`.
4. Après la sortie du host, lit les transcripts Claude et estime le coût (Claude uniquement).
5. Pose une question à une touche, enregistre le résultat avec `feedback advisory-record --followed`, ajoute une ligne à `runs.jsonl` et affiche les décisions enregistrées par politique.

## Installation, une seule fois

Installez le binaire depuis la branche qui contient les catalogues voulus :

```sh
cargo install --git https://github.com/FlorianBruniaux/agent-router --branch <branche> --locked agent-router-cli
```

Créez le dossier privé et copiez les fichiers depuis une copie du dépôt (le dossier, `runs` et `store` ne doivent pas être modifiables par le groupe ni les autres ; `ar` les crée en mode 0700) :

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
| `observation.json` | Instantané Codex, régénéré par `ar` s'il manque ou date de plus de 20 heures |
| `store/` | Magasin de retours, créé à la première exécution enregistrée |
| `consent-preferences.json` | Écrit à la première exécution enregistrée si le magasin n'a pas de consentement |
| `runs/<horodatage>/` | `compare.json` et un dossier par politique pour chaque exécution |
| `runs.jsonl` | Une ligne par exécution enregistrée |

`$AR_HOME` est la variable d'environnement `AR_HOME`, sinon `~/agent-router-advice`. `AR_BIN` désigne le binaire `agent-router` s'il n'est pas dans `PATH`. L'instantané Codex lit `~/.codex/models_cache.json` ; `ar` en passe le `client_version` comme `--observed-version`, écrit le nouvel instantané dans un fichier temporaire et ne le déplace que si `inspect` a réussi. L'horodatage de l'instantané est le `fetched_at` du cache : un cache que Codex n'a pas rafraîchi depuis 24 heures fait refuser `advise` avec `snapshot.stale`. Ouvrez Codex une fois, puis relancez.

Un alias zsh, avec le script et `claude-pricing.json` gardés ensemble :

```sh
alias ar='python3 ~/agent-router/integrations/daily/ar.py'
```

La première exécution enregistrée ne trouve aucun consentement de collecte et l'active (`collection_enabled` à true, `learning_enabled` à false, 90 jours), avec un message. Vous pouvez le retirer ensuite avec `feedback purge`.

## Usage quotidien

```sh
ar claude --text 'Ajouter un drapeau --dry-run à scripts/backup.sh' --kind implement --risk medium --scope scripts
ar codex  --text 'Relire le plan de migration' --kind review --risk high --scope docs --policy stable-v1
ar claude --text '...' --kind implement --risk low --scope src --dry-run -- --permission-mode plan
```

`--text`, `--kind`, `--risk` et `--scope` sont obligatoires. `--scope` n'a pas de valeur par défaut : `agent-router` refuse `.` et `./` avec `workflow.invalid` (vérifié le 2026-11-03 avec `--scope .`, `./`, `*` et `**`), et un périmètre est un fait que le produit ne définit jamais par défaut. Autres options : `--budget MICROUSD:MS:TOKENS` (défaut `0:1800000:400000`), `--skill ID` (répétable), `--policy stable-v1|stable-v2`, `--dry-run`. Les arguments après `--` vont au host, avant le texte de la tâche.

Ce que reçoit le host :

- Claude : `claude --model <modèle d'exécution> --effort <effort d'exécution> --session-id <uuid4> --agents <json> --append-system-prompt <texte> <tâche>`. Les deux sous-agents `ar-planner` (modèle du plan) et `ar-verifier` (modèle de vérification) sont définis par `--agents` ; le prompt ajouté demande à la session de planifier avec `ar-planner`, d'exécuter elle-même, de vérifier avec `ar-verifier` et liste les skills recommandés par étape.
- Codex : `codex -m <modèle d'exécution> -c model_reasoning_effort=<effort> <tâche>`. Les étapes de plan et de vérification sont seulement affichées comme instructions.

À la sortie du host, `ar` demande `[a]ccepted as is / [r]ework needed / [x] abandoned / [s]kip`. Ignorer (`s`) n'enregistre rien, ni dans le magasin ni dans `runs.jsonl`. `ar` rend le code de sortie du host.

## Lire le résultat après quelques semaines

```sh
agent-router feedback --store ~/agent-router-advice/store --json advisory-report
```

Lisez `groups` (une entrée par `policy_version` et `risk`), `paired_trials` et `interpretation_notes`. Les taux restent nuls sous 5 décisions par groupe (`minimum_sample`). `runs.jsonl` garde ce que le magasin ne compare pas : les tokens par modèle et le coût estimé. Coût estimé total par bras :

```sh
jq -s 'map(select(.cost_microusd != null)) | group_by(.arm) | map({arm: .[0].arm, runs: length, microusd: (map(.cost_microusd) | add)})' ~/agent-router-advice/runs.jsonl
```

Ne comparez le coût qu'entre bras de même mélange de types et de risques, et laissez hors de la somme les exécutions au coût `null` : elles sont UNKNOWN, pas gratuites.

## Comment le coût est estimé

Pour Claude, `ar` lit `~/.claude/projects/*/<session-id>.jsonl` et `~/.claude/projects/*/<session-id>/subagents/*.jsonl`, garde les lignes `assistant`, déduplique par `message.id` (la dernière ligne l'emporte) et additionne `input_tokens`, `output_tokens`, `cache_creation_input_tokens` et `cache_read_input_tokens` par `message.model`. Les tokens enregistrés sont ces quatre valeurs additionnées. Le coût utilise `integrations/daily/claude-pricing.json`, des prix catalogue en microusd par million de tokens copiés de `docs/bench/campaign-next/pilot/pricing.json` et `pricing-cache-writes.json` (consultés le 2026-09-30), arrondis au microusd entier par modèle.

Les écritures de cache sont facturées au tarif 5 minutes. Si `cache_creation` porte `ephemeral_5m_input_tokens` et `ephemeral_1h_input_tokens` dont la somme égale `cache_creation_input_tokens`, la part 1 heure utilise le tarif 1 heure. Ces deux noms de sous-clés ont été lus sur un vrai transcript Claude Code par le mainteneur le 2026-11-03. Un modèle sans prix dans la table (aujourd'hui `claude-haiku-5-5`, dont les prix de cache ne figurent pas dans le dépôt) rend le coût de l'exécution `null`, tandis que ses tokens restent comptés.

## Limites

- L'échantillon est auto-sélectionné : vous choisissez les tâches, le bras est déterministe par requête, et vous pouvez abandonner ou ignorer. C'est un essai personnel, pas une expérience contrôlée.
- Le coût est une estimation à partir de prix catalogue et de transcripts, pas une facture. L'abonnement, les remises batch et les surcoûts de contexte long ne sont pas modélisés.
- Le modèle et l'effort des sous-agents suivent le plan. Dans le JSON de `--agents`, chaque agent accepte `model` (un alias ou un identifiant complet comme `claude-opus-5-5`, mêmes valeurs que `--model`) et `effort` (de `low` à `max`, qui remplace l'effort de la session pour ce sous-agent) ; `ar` renseigne les deux sur `ar-planner` et `ar-verifier`. Source : la documentation de Claude Code, https://code.claude.com/docs/en/sub-agents.md and https://code.claude.com/docs/en/cli-reference.md, lue le 2026-11-03. Les sous-agents héritent de la configuration de réflexion de la session principale ; il n'existe pas de réglage de réflexion par sous-agent. Non vérifié avec un vrai run de `claude` : contrôlez les entrées `per_model` de `runs.jsonl` pour repérer les modèles inattendus.
- Le coût et les tokens Codex restent UNKNOWN : ses fichiers ne sont pas lus. `advisory-record` est appelé sans eux.
- Les qualités qui guident le choix sont des déclarations proposées ([ADR 0057](adr/0057-claude-core-v2-declared-ratings.md)), pas des mesures. Un meilleur coût sous `stable-v2` montre que les notes déclarées mènent à des modèles moins chers, pas que les résultats sont aussi bons ; le taux d'acceptation est votre propre jugement.
- Un `--policy` forcé est enregistré comme `forced` dans `runs.jsonl` ; excluez ces exécutions d'une comparaison non biaisée.
