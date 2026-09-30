# Pilote A/B du protocole de valeur du routeur, Claude Code et Codex

Rédigé le 2026-09-30 à partir de `origin/main` `c43d289` (lu dans `agent-router-wt/binary` et exporté par `git archive` dans `src/`, compilé en release dans ce dossier). Dossier du pilote : `$PILOT_HOME/Sites/perso/agent-router-pilot/`, hors de tout dépôt. Rien n'a été modifié dans `agent-router` ni dans le worktree `binary`.

## TL;DR

La chaîne produit `route -> prepare -> run --approve -> verify -> artifacts apply` ne peut pas exécuter une vraie tâche d'écriture Claude ou Codex aujourd'hui : les instantanés d'hôte que le produit sait produire (`inspect`) déclarent toutes les capacités `unknown`, et le routage rejette alors tout candidat qui écrit (`capability.unknown_or_unsupported`, sortie 3, mesuré sur les deux hôtes). Le pilote proposé est donc en mode host-direct : `agent-router route` calcule la décision du bras A (réelle, déterministe, scellée), puis un même exécuteur lance les CLI hôtes pour tous les bras. Les bras ne diffèrent que par la décision de routage. Le pilote ne mesure ni le runtime du routeur, ni son sandbox, ni l'assemblage de prompt.

Dix tâches donnent une direction et une estimation de variance (discordance, écart type du log-ratio de coût) pour fixer N. Elles ne donnent aucune conclusion publiable ni adoption de H1 ou H2.

## 1. Verdicts de faisabilité

| Étape | Verdict | Preuve |
| --- | --- | --- |
| (a) `claude -p "Reply with OK" --model claude-haiku-4-5` | Joignable et authentifié, depuis le bac à sable | Sortie 0, la sonde a répondu OK, `total_cost_usd` 0.096879, `cache_creation_input_tokens` 48222. `claude` n'est pas dans `excludedCommands`, donc la commande était sandboxée (TMPDIR de session `/tmp/claude-501`). Les refus réseau concernent `mcp-proxy.anthropic.com`, `datadoghq.com` et `posthog.com`, pas l'API |
| (a bis) écriture host-direct Claude, tâche t06, Haiku | Fonctionne | `probes/a3-stream.jsonl` : sortie 0, `pilot_lib/roman.py` modifié, test caché 2/2 OK, 0.0653304 USD déclarés, 11 tours, entrée 82 + écriture cache 11717 + lecture cache 232094, sortie 3721, un refus attendu (Bash hors liste blanche) |
| (b) `codex exec --model gpt-5.6-luna -c model_reasoning_effort=low --json --ephemeral --skip-git-repo-check "Reply with OK"` | Bloqué ici par le bac à sable, commande lancée seule sur sa ligne | Sortie 1, `Error: failed to initialize in-process app-server client: Operation not permitted (os error 1)`, plus `deny network-outbound ab.chatgpt.com:443`. `~/.codex` n'est pas dans la liste d'écriture du bac à sable. Relancée par Florian hors bac à sable le 2026-09-30 : OK. `agent_message` « OK », `turn.completed` usage `input_tokens` 30682, `cached_input_tokens` 11008, `cache_write_input_tokens` 0, `output_tokens` 5, `reasoning_output_tokens` 0. Bruit : avertissement `loading hooks from both ~/.codex/hooks.json and ~/.codex/config.toml`, deux échecs de rafraîchissement OAuth MCP (`sentry`, `vercel`, `invalid_grant`) |
| (c1) `route` sur fixture `codex-implementation.json` | OK | Sortie 0, `Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high` |
| (c2) `route-phase` sur les fixtures `fixtures/examples` avec `--now 100` | OK | Sortie 0, candidat `primary` sélectionné |
| (c3) `inspect --host claude` sur un vrai flux Claude 2.1.285 | Refusé | `{"error":"probe.cache.invalid"}`. Cause isolée : l'init annonce `claude-haiku-4-5`, le message assistant `claude-haiku-4-5-20251001`. Une copie de diagnostic avec l'init réécrit passe (sortie 0), ce n'est pas une preuve utilisable |
| (c4) `route-phase` d'une phase d'écriture sur l'instantané Claude dérivé | Refusé | Sortie 3, `capability.unknown_or_unsupported` (`WorkspaceWrite`) et `effort.host.unsupported` |
| (c5) `route-phase` d'une phase d'écriture sur l'instantané Codex réel (`inspect --host codex`) | Refusé | Sortie 3, `capability.unknown_or_unsupported` (`WorkspaceWrite`) |
| (c6) `route-phase` d'une phase `plan` en lecture seule sur l'instantané Codex réel | OK | Sortie 0, `gpt-5.6-luna` sélectionné |
| (c7) `prepare` à sec de cette phase `plan` (asset_authority null, prompt scellé en Python) | Refusé, cause non isolée | Sortie 2, `prepare.invalid_or_denied` |
| `route` v1 avec les catalogues du pilote (bras A) | OK, décision constante | Claude : `claude-opus-5-5/high -> claude-sonnet-5-5/medium -> claude-sonnet-5-5/medium` ; Codex : `gpt-6.1-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/medium` (catalogue `pilot-codex-pool-v2`). Une seule recette sur les 10 tâches pour chaque fournisseur, et le risque `low`, `medium` ou `high` ne la change pas (testé sur le pool Claude) |
| Pilote de bout en bout avec hôtes factices | OK | `./run-pilot.sh calibrate --fake` puis `run --fake` : 300/300 runs, relance sans travail, run interrompu déplacé et refait, refus sur dérive d'une entrée scellée, coût vérifié à la main (10208 et 14208 micro-USD) |
| Auto-contrôle du corpus | OK | Chaque test caché échoue sur la base et passe avec sa solution de référence : 20/20 |

Coût total des sondes réelles, déclaré par Claude : 0.096879 + 0.047978 + 0.065330 = 0.210187 USD. Aucune dépense Codex.

## 2. Pourquoi host-direct

Faits lus dans le code (`c43d289`) :

- `ClaudeStreamProbe` met toutes les capacités à `Unknown` et `supported_efforts` à vide (`crates/adapter-claude/src/probe.rs:112-135`) ; `CodexProbe` produit aussi des capacités `unknown` (instantané `probes/c5-codex-snapshot.json`).
- Le routage rejette toute capacité requise qui n'est pas `Supported` (`crates/router-policy/src/phases.rs:638-641`).
- Le Seatbelt du runtime n'ouvre le réseau que si la requête exige `Network` (`crates/router-runtime/src/lib.rs:1521`, `crates/router-runtime/src/sandbox.rs:447-449`). Sans réseau, la CLI hôte ne joint pas l'API ; avec, le routage refuse (même règle).
- Aucune commande CLI ne produit les documents v3 d'autorité d'actifs ni ne scelle un prompt : `seal_contracts`, `seal_availability` et `seal_prompt` n'apparaissent pas dans `crates/agent-router-cli/src` ; les tests natifs les appellent en bibliothèque (`crates/agent-router-cli/tests/support/native_fixture.rs:82-173`).
- Un cache de modèles Codex ne peut pas fournir de preuve native d'authentification (`docs/asset-contracts.md:19-21`).
- Le ledger indique « actual model behavior remains UNKNOWN » pour les adaptateurs (`docs/completion.md`, ligne #7).

Hypothèses non testées, qui bloqueraient ensuite l'exécution réelle par le runtime :

- l'adaptateur Claude passe `--permission-mode manual --permission-prompts none` en écriture (`crates/adapter-claude/src/execution.rs:34-53`) ; selon `claude --help`, tout ce qui demanderait une permission est alors refusé, donc les éditions le seraient probablement ;
- le Seatbelt vide l'environnement et ne lit que les racines listées : accès de Claude au trousseau et écritures dans `~/.claude`, non vérifiés ;
- Codex applique son propre Seatbelt, imbriqué dans celui du routeur : non vérifié.

Fabriquer un instantané qui déclare `supported` contredirait la règle « explicit UNKNOWN evidence » du projet. Je ne l'ai pas fait. Débloquer la chaîne produit demande une décision de Florian : une sonde native de capacités (travail produit) ou des instantanés attestés par l'opérateur, marqués comme tels, pour le seul banc.

## 3. Bras

Par fournisseur p, chaque bras reçoit le même énoncé, le même dépôt et la même acceptation :

| Bras | Claude | Codex | Phases |
| --- | --- | --- | --- |
| A, routeur | décision `route` : opus-5-5/high, puis sonnet-5-5/medium, puis sonnet-5-5/medium | gpt-6.1-sol/high, puis gpt-5.6-terra/medium, puis gpt-5.6-terra/medium | 3 processus |
| B, frontière effort high | claude-opus-5-5/high | gpt-6.1-sol/high | 3 processus |
| G, frontière effort medium | claude-opus-5-5/medium | gpt-6.1-sol/medium | 3 processus |
| C, moins cher effort medium | claude-sonnet-5-5/medium | gpt-5.6-terra/medium | 3 processus |
| E, une seule session | claude-opus-5-5/high | gpt-6.1-sol/high | 1 processus |
| D, défaut hôte (optionnel, `--with-d`) | sans `--model` ni `--effort` | idem | 1 processus |

G et E tiennent dans le pilote : ce sont des configurations fixes, sans code produit. Le pool de A est limité à B, G et C, comme le demande le protocole. Les catalogues `catalogs/*-pool.json` reprennent la structure de `assets/catalogs/codex-core-v1.json` (le moins cher n'a pas de stade `plan`). Conséquence mesurée : A applique une recette par phase identique pour toutes les tâches. Le pilote mesure donc cette recette fixe contre B, G et C, pas un choix par tâche.

Arguments hôtes, identiques pour tous les bras d'un fournisseur, hors modèle et effort :

- Claude : `--print --output-format stream-json --verbose --input-format text --no-session-persistence --permission-prompts none --permission-mode acceptEdits|manual --allowedTools Read Glob Grep "Bash(python3 -m unittest:*)" "Bash(git diff:*)" "Bash(git status:*)"`.
- Codex : `exec --json --ephemeral --color never --cd <wt> --skip-git-repo-check --sandbox workspace-write|read-only -`.

Configuration hôte par défaut : celle de Florian, telle quelle (`host_config: user-config` dans le manifeste). Elle pèse sur chaque processus : 48222 tokens d'écriture cache Claude (sonde a) et 30682 tokens d'entrée Codex (sonde b) pour « Reply with OK ».

Variante `isolated`, désactivée par défaut, activée seulement par `init --isolated` et scellée dans le manifeste. Elle passe uniquement par des options de ligne de commande, sans toucher aux fichiers de configuration ni aux identifiants :

- Claude : `--safe-mode`, qui selon `claude --help` désactive CLAUDE.md, skills, plugins, hooks, serveurs MCP, commandes et agents personnalisés, et garde l'authentification. Mesuré sur la sonde a bis : 11717 tokens d'écriture cache au lieu de 23302 à 48222.
- Codex : `--ignore-user-config` (ne charge pas `config.toml`, donc ni ses serveurs MCP ni ses hooks ; l'authentification reste), `--ignore-rules`, `--disable hooks` (couvre aussi `~/.codex/hooks.json`), `--disable plugins`, `--disable apps` (noms lus dans `codex features list`). Effet non mesuré avant la calibration ; chargement de `~/.codex/AGENTS.md` : non vérifié.

Surcoût fixe mesuré à part : `calibrate` lance d'abord, pour chaque fournisseur, chaque modèle du pool (frontière et moins cher) et chaque configuration (`user-config` et `isolated`), un appel en lecture seule « Reply with OK » dans un clone du dépôt, soit 8 appels. Le rapport les publie dans `fixed_overhead.probes`, puis estime par bras le surcoût (coût de la sonde du modèle de chaque phase, sous la configuration active) et sa part du coût du bras. Estimation seulement : le cache fournisseur change le coût du préfixe d'un appel à l'autre.

## 4. Tâches, vérification, acceptation

Dépôt jetable `corpus/base` (paquet `pilot_lib`, bibliothèque standard seule), recréé par `init` avec un commit à date fixe. Chaque run clone ce dépôt, sans remote.

| Tâche | Module | Risque déclaré | Objet |
| --- | --- | --- | --- |
| t01 | pagination | low | pages 1-based, dernière page partielle |
| t02 | text | low | `slugify` avec accents |
| t03 | durations | medium | `parse_duration` h/m/s et erreurs |
| t04 | intervals | low | `merge_intervals` |
| t05 | cache | medium | LRU réel au lieu de FIFO |
| t06 | roman | low | notation soustractive et erreurs |
| t07 | tables | medium | CSV avec en-tête et cellules vides |
| t08 | retry | high | nombre d'appels, backoff exponentiel |
| t09 | semver | medium | précédence SemVer 2.0.0 avec pré-versions |
| t10 | text | low | `word_wrap` glouton |

Vérification par tâche, jamais par l'agent : copie de l'arbre final sans `.git`, ajout de `corpus/tasks/tNN/hidden_test.py` en `tests/test_hidden_acceptance.py`, puis `python3 -m unittest tests.test_hidden_acceptance`. Accepté si la sortie vaut 0 et qu'aucun fichier hors `pilot_lib/` n'a changé. L'équivalent de `integration.applied` est implicite : l'acceptation porte sur l'arbre où l'agent a écrit. Le verdict PASS/FAIL de la phase verify de l'agent est enregistré à part (`agent_pass_but_rejected`).

Run invalide, gardé et marqué : arbre sale au départ, modèle annoncé différent du modèle demandé (alias et identifiant daté acceptés), plus d'une session pour E ou D, échec d'infrastructure (timeout, sortie non nulle sans usage) avec une relance, version d'une CLI changée depuis `init` (refus de continuer).

## 5. Sorties

`results/report.json` et `results/report.md`, par fournisseur et par bras : runs valides et invalides, acceptés et rejetés, coût total au prix catalogue, coût par tâche acceptée (ratio de sommes, échecs inclus, nul dès qu'un coût est inconnu) en deux variantes, temps jusqu'à l'acceptation p50 et p90, tokens par champ avec le nombre de runs où le champ est inconnu, phases dont le modèle n'est pas annoncé. Section `sizing` : pour A-B, A-C, B-E, B-G et G-C, discordance de succès et écart type par tâche du log-ratio de coût, puis N avec les formules du protocole à alpha 5 % et 1,25 % (formules recontrôlées sur son tableau : 62, 124, 96 et 32 retrouvés).

Coût « product » : sémantique de `router_eval::price_usage` et de `feedback report --pricing` (écritures cache au prix d'entrée, arrondi au micro-dollar supérieur par phase). Coût « list » : ajoute la prime d'écriture cache Claude selon le partage 5 min / 1 h déclaré dans le flux. Le coût déclaré par Claude (`total_cost_usd`) sert de recoupement, jamais de source.

## 6. Tarifs

`pricing.json` au format `PricingTable` (`crates/router-eval/src/replay.rs:48-63`), accepté par `agent-router feedback --store <vide> --json report --pricing pricing.json` (sortie 0). En USD par million de tokens, entrée / entrée en cache / sortie :

| Modèle | Prix | Source, consultée le 2026-09-30 |
| --- | --- | --- |
| claude-opus-5-5 | 4 / 0.20 / 20 ; écriture cache 5 / 8 | https://platform.claude.com/docs/en/about-claude/pricing |
| claude-sonnet-5-5 | 2 / 0.20 / 10 ; écriture cache 2.50 / 4 | idem |
| claude-haiku-4-5 | 1 / 0.10 / 5 ; écriture cache 1.25 / 2 | idem |
| claude-fable-5-1 | 10 / 0.25 / 50 ; écriture cache 12.50 / 20 | idem |
| gpt-6.1-sol (frontière Codex) | 2 / 0.10 / 10 ; écriture cache 2.50, non appliquée | https://developers.openai.com/api/docs/pricing, table « Standard pricing data », contexte court (≤272K), ligne citée dans `pricing-sources.md` |
| gpt-5.6-sol | 4 / 0.40 / 20 | idem, lecture du premier passage |
| gpt-5.6-terra | 2 / 0.20 / 12 | idem |
| gpt-5.6-luna | 0.20 / 0.02 / 1.20 | idem |
| gpt-6-sol, gpt-6-luna, gpt-6-astra | 2/0.20/10, 0.10/0.01/0.50, 10/1/50 | idem |

Réserves : la page OpenAI a été lue à travers un outil de récupération qui résume (https://openai.com/api/pricing/ répond 403) ; un humain doit relire ces lignes avant le lancement. La grille de crédits Codex (https://help.openai.com/en/articles/20001106-codex-rate-card) répond 403 : non lue, aucune valeur en crédits dans le pilote, C3 reste UNKNOWN. Les coûts sont des prix catalogue API ; les runs consomment en réalité les abonnements de Florian (connexion Claude et ChatGPT), le coût calculé n'est donc pas une facture.

## 7. Volume et coût

Runs = 10 tâches x 5 bras x 2 fournisseurs x 3 répétitions = 300 runs tâche-bras (règle du protocole pour le pilote). Processus hôtes = par fournisseur 10 x 3 x (4 bras x 3 phases + 1) = 390, soit 780 au total. Avec D : 360 runs et 840 processus.

Tokens par run : UNKNOWN. Le dépôt ne contient aucune mesure de tokens par run (`docs/bench/` ne contient que le protocole ; `docs/performance.md` mesure la latence de routage et le démarrage). Le seul point mesuré est la sonde a bis (Haiku, une phase, 0.065 USD), qui ne représente ni Opus ni Codex. Le coût du pilote est donc UNKNOWN avant calibration.

Calibration intégrée : `./run-pilot.sh calibrate` exécute le premier bloc de l'ordre tiré (1 tâche, 1 répétition, 5 bras, 2 fournisseurs : 10 runs, 26 processus), puis projette le coût total par fournisseur et s'arrête. Le bloc de calibration n'a pas de plafond : sa taille le borne. `run` refuse de démarrer sans plafond explicite pour chaque fournisseur, passé par `--budget-cap-usd 'claude=USD,codex=USD'` ou `PILOT_BUDGET_CAP_USD` ; il n'existe aucune valeur par défaut. Le pilote cesse ensuite de lancer les runs d'un fournisseur dès que son coût catalogue cumulé atteint le plafond. Florian fixe les plafonds après la calibration.

## 8. Lancement

Codex ne tourne pas dans le bac à sable de Claude Code ; le lancement se fait donc hors bac à sable, par Florian avec `!` :

1. Sonde Codex : faite par Florian le 2026-09-30, OK (section 1).
2. Scellement, 8 sondes de surcoût, puis bloc de calibration (10 runs, 26 processus) : `! $PILOT_HOME/Sites/perso/agent-router-pilot/run-pilot.sh calibrate`. Pour la variante isolée à la place : `! $PILOT_HOME/Sites/perso/agent-router-pilot/run-pilot.sh calibrate --isolated`.
3. Après lecture de la projection, avec les plafonds choisis : `! $PILOT_HOME/Sites/perso/agent-router-pilot/run-pilot.sh run --budget-cap-usd 'claude=USD,codex=USD'`

La commande `run` reprend là où elle s'est arrêtée. `status` et `report` lisent sans rien lancer. Toute modification de corpus, tarifs, catalogues, configuration, pilote ou binaire `agent-router` après `init` fait refuser la reprise.

## 9. Écarts au protocole

- B, G et C ne passent pas par le pipeline produit (« Pipeline, single candidate » dans le protocole) ; A non plus. La comparaison porte sur la décision de routage seule.
- Faits `verify` et `artifacts apply` non enregistrés et `feedback report --pricing` non utilisé : aucun run ne passe par le runtime. Le pilote recalcule les mêmes métriques avec la même sémantique de prix.
- P2, P3 et P5 du protocole ne sont pas livrés sur `c43d289` (`scripts/bench/` absent, `TokenPrice` sans prix d'écriture cache). Le pilote tient lieu de P5 minimal hors produit.
- Tokens de raisonnement Codex : comptés dans la sortie, hypothèse reprise du décodeur produit (`crates/router-adapter-api/src/execution.rs:976-989`). Raisonnement Claude : UNKNOWN (`thinking_tokens` enregistré à part, non tarifé à part).
- L'énoncé de E concatène les consignes des trois phases, pas les prompts préparés de B (pas de `prepare`).
- Aucun délai imposé entre phases ni entre runs d'un même modèle ; les écarts sont enregistrés. Le cache fournisseur peut favoriser le bras qui suit un autre bras du même modèle : ordre tiré au sort avec graine fixe, rien de plus.
- Le risque déclaré des tâches ne change pas la décision de A ; les strates ne sont pas analysables avec 10 tâches.

## 10. Écart relevé pour le produit, sans modification du produit

Codex 0.159.2 déclare `cache_write_input_tokens` dans `turn.completed.usage` (valeur 0 sur la sonde b). Le décodeur d'usage du lot 2 ne lit pas ce champ pour Codex (`UsageCollector::codex_turn`, `crates/router-adapter-api/src/execution.rs:976-989`) : `cache_write_input_tokens` y reste UNKNOWN. Le pilote lit le champ quand il est présent, garde `None` quand il est absent, et rend le coût catalogue UNKNOWN si une valeur non nulle apparaît, faute de sémantique connue (le prix d'écriture cache OpenAI, 2.50 USD par MTok, est noté dans `pricing-sources.md` mais pas appliqué).

## 11. Décisions

Décisions de Florian transmises le 2026-09-30 : mode host-direct accepté ; Codex frontière `gpt-6.1-sol`, moins cher `gpt-5.6-terra` ; plafonds fixés après calibration.

Reste ouvert :

1. Relecture humaine des lignes de prix OpenAI (`pricing-sources.md`), lues à travers un outil qui résume.
2. Grille de crédits Codex non relue (403).
3. Choix de la configuration hôte : `user-config` (défaut) ou `isolated`, au vu des sondes de surcoût de la calibration. Changer d'avis après `init` impose un nouveau dossier `results/`.

## 12. Fichiers

- `run-pilot.sh`, `run_pilot.py` : lanceur et pilote (init, selfcheck, calibrate, run, report, status ; `--fake`, `--providers`, `--with-d`).
- `pilot-config.json`, `catalogs/claude-pool.json`, `catalogs/codex-pool.json`.
- `pricing.json`, `pricing-cache-writes.json`, `pricing-sources.md` (lignes de prix citées, source non relue).
- `corpus/base/`, `corpus/tasks/tNN/{statement.md,hidden_test.py,meta.json}`, `corpus/solutions/` (auto-contrôle seulement), `corpus/build_tasks.py`.
- `fake/fake_host.py` : hôte synthétique pour tester le pilote sans token.
- `probes/` : sorties brutes des sondes. `results-fake/` : run factice complet. `src/` : export de `c43d289` et binaires release.
