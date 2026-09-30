# Installation

[English](install.md) | Français

Agent Router se compile depuis les sources. Il n'existe ni paquet publié dans un
gestionnaire de paquets, ni étape automatique de configuration globale. L'installation
n'active jamais d'intégration de host, de hook ni de permission (voir [Ce que l'installation ne
fait pas](#ce-que-linstallation-ne-fait-pas)).

Le dépôt est privé : pour le cloner, il faut un accès à
`FlorianBruniaux/agent-router`.

## Prérequis

| Prérequis | Détail |
| --- | --- |
| Système d'exploitation | macOS ou Linux pour l'exécution supervisée. L'exécution native sous Windows n'est pas implémentée ([hosts supportés](../README.fr.md#hosts-supportés)). Le routage, `explain` et les autres commandes en lecture seule ne lancent rien. |
| Rust | 1.85 ou plus récent. `rust-toolchain.toml` épingle le canal `1.85` avec `rustfmt` et `clippy`, donc `rustup` le sélectionne dans le clone. |
| Git | Pour cloner le dépôt. |
| Sandbox Linux | L'exécution supervisée a besoin d'un binaire bubblewrap et de namespaces non privilégiés fonctionnels ([runtime](runtime.md#native-sandbox-evidence)). |
| Sandbox macOS | L'exécution supervisée utilise `/usr/bin/sandbox-exec` (Seatbelt). |
| Python 3.11 ou plus récent | Uniquement pour les scripts de release de `scripts/`. La CLI n'appelle jamais Python. |
| ffmpeg | Uniquement pour régénérer les médias de démo du README ([guide des médias](media/README.md)). Inutile pour compiler ou utiliser la CLI. |

Une fois les dépendances Cargo disponibles, le routage fonctionne hors ligne.
Aucun service sémantique ni téléchargement de modèle n'est nécessaire à
l'exécution.

## Compiler depuis les sources

```sh
git clone git@github.com:FlorianBruniaux/agent-router.git
cd agent-router
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper
```

Vous obtenez deux binaires dans `target/release/` :

- `agent-router` : la CLI.
- `router-sandbox-helper` : le helper de limites de ressources dont a besoin
  l'exécution supervisée. C'est un binaire interne qui ne prend aucun argument
  utilisateur : ne le lancez pas directement. Vous indiquez son chemin absolu dans
  le fichier de sandbox (champ `helper`, voir le
  [guide d'exécution](cli-execution.md#sandbox-and-privacy-configuration)). Son
  SHA-256 fait partie de l'approbation d'exécution et il est revérifié avant le
  lancement ([runtime](runtime.md#native-sandbox-evidence)) ; un helper
  absent ou modifié échoue avant d'être exécuté.

Si vous vous contentez de router, planifier et expliquer, `agent-router` suffit.

### Sans adaptateurs de host

Le build par défaut inclut les adaptateurs Codex et Claude. Pour compiler
la CLI de routage sans eux :

```sh
cargo build --release --locked --no-default-features -p agent-router-cli
```

### Avec des features optionnelles

Les features s'activent explicitement à la compilation. Voir [Features
optionnelles](#features-optionnelles).

```sh
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper \
  --features agent-router-cli/local-api
```

## Vérifier l'installation

À lancer depuis la racine du dépôt. Ces commandes utilisent des fixtures
synthétiques fournies avec le projet, n'appellent aucun modèle et ne lancent aucun host.

```sh
./target/release/agent-router --version
./target/release/agent-router --help
```

Puis routez une fixture et lisez l'explication :

```sh
./target/release/agent-router route \
  --request tests/fixtures/requests/codex-implementation.json \
  --catalog assets/catalogs/codex-core-v1.json \
  --json > "${TMPDIR:-/tmp}/agent-router-decision.json"

./target/release/agent-router explain \
  --decision "${TMPDIR:-/tmp}/agent-router-decision.json"
```

Sortie attendue, à la ligne du hash de décision près :

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

`route` sort avec `0` pour une sélection, `3` pour un résultat valide sans route,
`2` pour une entrée invalide, `4` pour une erreur interne et `7` pour une erreur
d'intégrité ; voir les [codes de sortie](cli-output.md#exit-codes-and-error-codes).

### Diagnostic

`doctor` est la commande de diagnostic en lecture seule
([diagnostic](doctor.md)). Elle ne découvre pas les répertoires personnels, ne
lance aucun host, ne répare aucun fichier et ne lance aucune migration. Elle
existe sous deux formes :

```sh
agent-router doctor --request manifest.json --json
agent-router doctor --runtime-root /absolute/private/runtime --json
```

La seconde forme inspecte un répertoire de runtime existant et refuse un
répertoire absent ; elle ne crée rien et ne lance aucune reprise. Ce n'est pas un
test d'installation neuve, puisqu'une installation neuve n'a pas de
répertoire de runtime. SQLite peut quand même mettre à jour son index WAL
pendant la lecture : ce n'est donc pas une garantie zéro écriture
([doctor](doctor.md#passive-runtime-storage-diagnostics)).

Un `doctor` qui passe n'établit ni qu'un host fonctionne, ni qu'un sandbox
applique réellement quoi que ce soit, ni qu'un identifiant est valide.

## Build de release locale

Pour une archive binaire locale reproductible avec un manifeste signé, suivez
[release-local.md](release-local.md). Elle s'appuie sur
[binary-distribution.md](binary-distribution.md) (l'archive binaire) et
[distribution.md](distribution.md) (les archives source séparées). Rien, dans ces
étapes, ne signe, ne publie ni n'installe à votre place.

L'archive binaire est compilée avec `--no-default-features` et ne contient que
`agent-router`. Elle ne contient pas `router-sandbox-helper` : pour une exécution
supervisée depuis cette archive, il faut compiler le helper à part avec la
commande de build ci-dessus.

### Homebrew, préfixe privé

`scripts/release-local.py verify` peut générer une formule Homebrew à partir de
`distribution/homebrew/agent-router.rb.in`, qui pointe vers l'archive locale via
une URL `file://`. Son installation est UNKNOWN : elle n'a pas été exercée. Seule
la syntaxe a été vérifiée, avec `ruby -c`, sur une formule générée. Le bloc
`install`, la gestion de `file://` et le bloc `test` de la formule ne sont pas
testés ([limites](release-local.md#limits)).

Si vous l'essayez, utilisez un préfixe Homebrew privé. Installer avec le `brew` du
système a des effets globaux. La formule installe uniquement `bin/agent-router` et
les documents de licence.

## Features optionnelles

Relevées dans `crates/agent-router-cli/Cargo.toml` et les guides liés. Passez-les
sous la forme `--features agent-router-cli/<name>` quand vous compilez plusieurs
paquets avec `-p`.

| Feature | Par défaut | Ce qu'elle active |
| --- | --- | --- |
| `codex` | oui | Adaptateur de host Codex (`adapter-codex`). |
| `claude` | oui | Adaptateur de host Claude Code (`adapter-claude`). |
| `opencode` | non | Adaptateur OpenCode et champ optionnel `opencode` du manifeste de sandbox ([autres hosts](other-hosts.md)). |
| `copilot` | non | Adaptateur Copilot et champ optionnel `copilot` du manifeste de sandbox ([autres hosts](other-hosts.md)). |
| `local-api` | non | La sous-commande `serve` : une API de routage en lecture seule sur un socket Unix, avec un bearer token ([API locale](local-api.md)). Sans cette feature, pas de `serve`. |
| `local-sidecar` | non | Le transport client sortant du sidecar standard. Le build par défaut n'en contient aucun ([sidecar standard](standard-sidecar.md)). |
| `semantic-retrieval` | non | Validation d'un fichier `--semantic-ranking` fourni de l'extérieur, avec `--context-query` (active la feature `semantic-ranking` de `router-retrieval`). N'embarque aucun modèle d'embeddings, aucun provider ni aucune requête réseau ([recherche](retrieval.md#optional-externally-supplied-semantic-ranking)). |

`semantic-ranking` est une feature de la bibliothèque `router-retrieval`, pas de
la CLI. La CLI l'expose sous le nom `semantic-retrieval`.

Confirmé sur un build de ce clone : `agent-router serve --help` existe
avec `--features local-api` (options `--state-dir`, `--max-connections`,
`--request-deadline-ms`). Les autres builds avec features n'ont pas été
lancés pour cette page ; leurs effets sont tirés du manifeste Cargo et des guides
ci-dessus.

## Désinstaller

Le build écrit dans le clone (`target/`) et dans le cache de Cargo
(`~/.cargo`, partagé avec vos autres projets Rust ; laissez-le, sauf si vous voulez
récupérer de la place). Désinstaller revient à supprimer ce que vous avez créé.

1. Supprimez le clone, qui contient `target/` et les deux binaires. Si vous avez
   copié `agent-router` ou `router-sandbox-helper` ailleurs, supprimez aussi ces
   copies.
2. Supprimez les répertoires d'état privés que vous avez choisis. Agent Router
   n'utilise aucun emplacement par défaut : chacun est un chemin que vous avez
   passé explicitement.
   - Stores de runtime et de workflow : les chemins `--store` et
     `--runtime-root`. Ils contiennent la base SQLite (fichiers WAL compris), le
     fichier de verrou et les artefacts conservés
     ([runtime](runtime.md#durable-state-and-control)).
   - État de l'API locale : le chemin `--state-dir` passé à `serve`
     (`router.sock`, `token`, `lock`).
   - Stores de feedback et de catalogue, ainsi que les fichiers de sandbox ou
     d'approbation que vous avez écrits, aux chemins choisis.
3. Si vous avez généré puis installé une formule Homebrew, désinstallez-la avec
   Homebrew, dans le préfixe utilisé. Ce chemin est UNKNOWN, comme expliqué plus haut.

Avant de supprimer un store de runtime, vérifiez s'il reste des exécutions
interrompues ou des sorties conservées dont vous avez encore besoin. La suppression
retire les entrées de répertoire et ne prétend pas effacer le support de façon sécurisée
([runtime](runtime.md#durable-state-and-control)).

## Ce que l'installation ne fait pas

- Elle n'installe ni n'active aucun hook, plugin, skill ou intégration de host.
- Elle n'écrit ni configuration globale, ni fichier d'environnement, ni
  identifiant.
- Elle n'accorde aucune permission, aucune approbation ni aucun accès réseau. L'exécution
  exige un digest approuvé explicitement, des preuves de capacités récentes et
  un sandbox natif fonctionnel.
- Elle ne lance aucun host et n'appelle aucun modèle.
- Elle ne crée ni répertoire de runtime, ni base de données, ni token. Ils n'apparaissent
  que lorsque vous lancez une commande qui leur désigne un chemin.
- Elle ne télécharge ni embeddings, ni backend sémantique, ni dépendances à
  l'exécution.
- Elle ne publie, ne signe ni n'installe aucune archive de release.

Ensuite : [préparation](preparation.md), puis le
[guide d'une exécution unique](cli-execution.md) ou le [guide des workflows](coordinator.md).
