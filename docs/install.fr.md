# Installation

[English](install.md) | Français

Agent Router se compile depuis les sources. Il n'existe ni paquet publié, ni
étape de configuration globale automatique. L'installation n'active jamais une
intégration d'hôte, un hook ou une permission (voir [Ce que l'installation ne
fait pas](#ce-que-linstallation-ne-fait-pas)).

Le dépôt est privé : le cloner demande un accès à
`FlorianBruniaux/agent-router`.

## Prérequis

| Prérequis | Détail |
| --- | --- |
| Système | macOS ou Linux pour l'exécution supervisée. L'exécution native sous Windows n'est pas implémentée ([hôtes pris en charge](../README.fr.md#prise-en-charge-des-hôtes)). Le routage, `explain` et les autres commandes en lecture seule ne lancent rien. |
| Rust | 1.85 ou plus récent. `rust-toolchain.toml` épingle le canal `1.85` avec `rustfmt` et `clippy`, donc `rustup` le choisit dans le dépôt. |
| Git | Pour cloner le dépôt. |
| Bac à sable Linux | L'exécution supervisée demande un binaire bubblewrap et des namespaces non privilégiés fonctionnels ([runtime](runtime.md#native-sandbox-evidence)). |
| Bac à sable macOS | L'exécution supervisée utilise `/usr/bin/sandbox-exec` (Seatbelt). |
| Python 3.11 ou plus récent | Seulement pour les scripts de release de `scripts/`. La CLI n'appelle jamais Python. |
| ffmpeg | Seulement pour régénérer les médias de démo du README ([guide des médias](media/README.md)). Inutile pour compiler ou utiliser la CLI. |

Une fois les dépendances Cargo disponibles, le routage fonctionne hors ligne.
Aucun service sémantique ni téléchargement de modèle n'est nécessaire à
l'exécution.

## Compiler depuis les sources

```sh
git clone git@github.com:FlorianBruniaux/agent-router.git
cd agent-router
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper
```

Cela produit deux binaires dans `target/release/` :

- `agent-router` : la CLI.
- `router-sandbox-helper` : l'assistant de limites de ressources dont
  l'exécution supervisée a besoin. C'est un binaire interne sans argument
  utilisateur, ne le lancez pas directement. Son chemin absolu se renseigne dans
  le fichier de bac à sable (champ `helper`, voir le
  [guide d'exécution](cli-execution.md#sandbox-and-privacy-configuration)). Son
  SHA-256 fait partie de l'approbation d'exécution et il est revérifié avant le
  lancement ([runtime](runtime.md#native-sandbox-evidence)) ; un assistant
  absent ou modifié échoue avant d'être exécuté.

Pour seulement router, planifier et expliquer, `agent-router` suffit.

### Sans adaptateurs d'hôte

La compilation par défaut inclut les adaptateurs Codex et Claude. Pour compiler
la CLI de routage sans eux :

```sh
cargo build --release --locked --no-default-features -p agent-router-cli
```

### Avec des options

Les options se compilent explicitement. Voir [Options de
compilation](#options-de-compilation).

```sh
cargo build --release --locked -p agent-router-cli -p router-sandbox-helper \
  --features agent-router-cli/local-api
```

## Vérifier l'installation

À lancer depuis la racine du dépôt. Ces commandes utilisent des fixtures
synthétiques fournies, n'appellent aucun modèle et ne lancent aucun hôte.

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

Sortie attendue, hors ligne du hash de décision :

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

`route` sort en `0` pour une sélection, `3` pour un résultat valide sans route,
`2` pour une entrée invalide et `4` pour une erreur interne. La table complète
des codes de sortie est dans [la sortie de la CLI](cli-output.md).

### Diagnostic

`doctor` est la commande de diagnostic en lecture seule
([diagnostic](doctor.md)). Il ne découvre pas les répertoires personnels, ne
lance pas d'hôte, ne répare pas de fichier et ne lance pas de migration. Deux
formes existent :

```sh
agent-router doctor --request manifest.json --json
agent-router doctor --runtime-root /absolute/private/runtime --json
```

La seconde forme inspecte un répertoire de runtime existant et refuse un
répertoire absent ; elle ne crée rien et ne récupère rien. Ce n'est pas un
contrôle d'installation neuve, puisqu'une installation neuve n'a pas de
répertoire de runtime. SQLite peut tout de même mettre à jour son index WAL
pendant la lecture : ce n'est donc pas une garantie d'absence d'écriture
([doctor](doctor.md#passive-runtime-storage-diagnostics)).

Un `doctor` réussi n'établit pas qu'un hôte fonctionne, qu'un bac à sable
applique quoi que ce soit ni qu'un identifiant est valide.

## Compilation de release locale

Pour une archive binaire locale reproductible avec un manifeste signé, suivez
[release-local.md](release-local.md). Elle s'appuie sur
[binary-distribution.md](binary-distribution.md) (l'archive binaire) et
[distribution.md](distribution.md) (les archives de sources séparées). Rien n'y
signe, ne publie ni n'installe à votre place.

L'archive binaire est compilée avec `--no-default-features` et ne contient que
`agent-router`. Elle ne contient pas `router-sandbox-helper` : l'exécution
supervisée depuis cette archive demande l'assistant compilé à part avec la
commande ci-dessus.

### Homebrew, préfixe privé

`scripts/release-local.py verify` peut produire une formule Homebrew à partir de
`distribution/homebrew/agent-router.rb.in`, qui pointe vers l'archive locale par
une URL `file://`. Son installation est UNKNOWN : elle n'a pas été exercée. Seule
la syntaxe a été vérifiée avec `ruby -c` sur une formule produite. Le bloc
`install`, la gestion de `file://` et le bloc `test` de la formule ne sont pas
testés ([limites](release-local.md#limits)).

Si vous essayez, utilisez un préfixe Homebrew privé. Installer avec le `brew` du
système a des effets globaux. La formule installe `bin/agent-router` et les
documents de licence uniquement.

## Options de compilation

Lues dans `crates/agent-router-cli/Cargo.toml` et les guides liés. Passez-les
sous la forme `--features agent-router-cli/<nom>` quand vous compilez plusieurs
paquets avec `-p`.

| Option | Par défaut | Ce qu'elle active |
| --- | --- | --- |
| `codex` | oui | Adaptateur d'hôte Codex (`adapter-codex`). |
| `claude` | oui | Adaptateur d'hôte Claude Code (`adapter-claude`). |
| `opencode` | non | Adaptateur OpenCode et champ optionnel `opencode` du manifeste de bac à sable ([autres hôtes](other-hosts.md)). |
| `copilot` | non | Adaptateur Copilot et champ optionnel `copilot` du manifeste de bac à sable ([autres hôtes](other-hosts.md)). |
| `local-api` | non | La sous-commande `serve` : une API de routage en lecture seule sur un socket Unix avec un jeton bearer ([API locale](local-api.md)). Sans l'option, pas de `serve`. |
| `local-sidecar` | non | Le transport client sortant du sidecar standard. La compilation par défaut n'en a pas ([sidecar standard](standard-sidecar.md)). |
| `semantic-retrieval` | non | Validation d'un fichier `--semantic-ranking` fourni de l'extérieur avec `--context-query` (active l'option `semantic-ranking` de `router-retrieval`). N'embarque aucun modèle d'embeddings, fournisseur ni requête réseau ([recherche](retrieval.md#optional-externally-supplied-semantic-ranking)). |

`semantic-ranking` est une option de la bibliothèque `router-retrieval`, pas de
la CLI. La CLI l'expose sous le nom `semantic-retrieval`.

## Désinstaller

La compilation écrit dans le dépôt (`target/`) et dans le cache de Cargo
(`~/.cargo`, partagé avec vos autres projets Rust ; laissez-le sauf pour
récupérer de la place). Désinstaller consiste à supprimer ce que vous avez créé.

1. Supprimez le dépôt, qui contient `target/` et les deux binaires. Si vous avez
   copié `agent-router` ou `router-sandbox-helper` ailleurs, supprimez ces
   copies.
2. Supprimez les répertoires d'état privés que vous avez choisis. Agent Router
   n'utilise aucun emplacement par défaut : chacun est un chemin passé
   explicitement.
   - Stores de runtime et de workflow : les chemins `--store` et
     `--runtime-root`. Ils contiennent la base SQLite (fichiers WAL compris), le
     fichier de verrou et les artefacts conservés
     ([runtime](runtime.md#durable-state-and-control)).
   - État de l'API locale : le chemin `--state-dir` passé à `serve`
     (`router.sock`, `token`, `lock`).
   - Stores de feedback et de catalogue, fichiers de bac à sable ou
     d'approbation que vous avez écrits, aux chemins choisis.
3. Si vous avez produit et installé une formule Homebrew, désinstallez-la avec
   Homebrew dans le même préfixe. Ce chemin est UNKNOWN, comme indiqué plus haut.

Avant de supprimer un store de runtime, vérifiez qu'aucun run interrompu ni
aucune sortie conservée ne vous est encore utile. La suppression retire les
entrées de répertoire et ne prétend pas effacer le support de façon sécurisée
([runtime](runtime.md#durable-state-and-control)).

## Ce que l'installation ne fait pas

- Elle n'installe ni n'active de hook, plugin, skill ou intégration d'hôte.
- Elle n'écrit ni configuration globale, ni fichier d'environnement, ni
  identifiant.
- Elle n'accorde aucune permission, approbation ni accès réseau. L'exécution
  demande un digest d'approbation explicite, des preuves de capacité fraîches et
  un bac à sable natif fonctionnel.
- Elle ne lance pas d'hôte et n'appelle pas de modèle.
- Elle ne crée ni répertoire de runtime, ni base, ni jeton. Ils n'apparaissent
  que lorsque vous lancez une commande qui leur donne un chemin.
- Elle ne télécharge ni embeddings, ni backend sémantique, ni dépendance à
  l'exécution.
- Elle ne publie, ne signe ni n'installe d'archive de release.

Ensuite : [préparation](preparation.md), puis le
[guide d'une exécution](cli-execution.md) ou le [guide des workflows](coordinator.md).
