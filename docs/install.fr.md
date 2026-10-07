# Installation

[English](install.md) | Français

Agent Router propose deux canaux d'installation expérimentaux : Cargo depuis
une révision Git épinglée, et une formule Homebrew générée pour une archive
locale vérifiée. Tous deux installent `agent-router` avec ses adaptateurs
Claude/Codex par défaut et `router-sandbox-helper`. L'installation n'active
aucune intégration de host, aucun hook ni permission (voir [Ce que l'installation
ne fait pas](#ce-que-linstallation-ne-fait-pas)).

Le dépôt est privé : pour le cloner, il faut un accès à
`FlorianBruniaux/agent-router`. Aucun paquet crates.io, tap Homebrew public ni
asset de release public n'est annoncé ici. Les observations actuelles sont dans
la [qualification de distribution](bench/distribution-qualification-2026-10-07.md) ;
l'[ancien essai de la CLI seule](bench/release-qualification-2026-10-07.md) reste historique.

## Prérequis

| Prérequis | Détail |
| --- | --- |
| Système d'exploitation | macOS ou Linux pour l'exécution supervisée. L'exécution native sous Windows n'est pas implémentée ([hosts supportés](../README.fr.md#hosts-supportés)). Le routage, `explain` et les autres commandes en lecture seule ne lancent rien. |
| Rust | 1.85 ou plus récent. `rust-toolchain.toml` épingle le canal `1.85` avec `rustfmt` et `clippy`, donc `rustup` le sélectionne dans le clone. |
| Toolchain C | Compilateur C natif, linker et SDK de la plateforme pour les builds source, y compris Cargo, car SQLite est compilé depuis les sources embarquées. |
| Git | Pour cloner le dépôt. |
| Sandbox Linux | L'exécution supervisée a besoin d'un binaire bubblewrap et de namespaces non privilégiés fonctionnels ([runtime](runtime.md#native-sandbox-evidence)). |
| Sandbox macOS | L'exécution supervisée utilise `/usr/bin/sandbox-exec` (Seatbelt). |
| Python 3.11 ou plus récent | Uniquement pour les scripts de release de `scripts/`. La CLI n'appelle jamais Python. |
| ffmpeg | Uniquement pour régénérer les médias de démo du README ([guide des médias](media/README.md)). Inutile pour compiler ou utiliser la CLI. |

Une fois les dépendances Cargo disponibles, le routage fonctionne hors ligne.
Aucun service sémantique ni téléchargement de modèle n'est nécessaire à
l'exécution.

## Cargo depuis une révision Git épinglée

Choisissez un préfixe d'installation absolu appartenant à votre utilisateur.
Remplacez le chemin d'exemple avant de lancer la commande. Cargo y écrit les
binaires et ses métadonnées d'installation, et télécharge les sources et les
dépendances dans son cache. Tant que le dépôt est privé, utilisez votre accès
Git authentifié existant.

```sh
install_prefix="/absolute/user-prefix"
cargo install --git https://github.com/FlorianBruniaux/agent-router \
  --rev f378afad4f584bac0efabbbb9dab37c48db4298f --locked \
  --root "$install_prefix" agent-router-cli router-sandbox-helper

"$install_prefix/bin/agent-router" version --json
```

La révision épinglée identifie le code produit utilisé pour cette qualification,
pas les changements d'outillage de distribution non publiés. Les features CLI
attendues sont `claude` et `codex`. Le helper reste un programme interne : son
chemin absolu et son hash figurent explicitement dans la configuration et
l'approbation d'exécution décrites ci-dessous. Ajouter les binaires au `PATH`
reste un choix de l'utilisateur.

Les dépendances du workspace utilisent des chemins locaux ; installer
`agent-router-cli` par son nom depuis crates.io n'est pas un canal pris en
charge. La commande Git épinglée fournit le workspace complet. Sur macOS ARM64,
les deux paquets ont été installés séparément depuis un clone Git local
`file://` à cette révision, avec les dépendances vendoriées hors ligne.
Route/explain ont passé hors du clone, avec les sources et clones en cache
masqués ; la répétition, le remplacement avec `--force` à la même révision et
la désinstallation ont préservé le marqueur préexistant. L'acquisition Git en
HTTPS n'a pas été exercée. Le remplacement à une seule révision ne valide pas
une montée de version du produit, qui reste `UNKNOWN`. Voir la qualification liée.

## Compiler depuis un clone

```sh
git clone https://github.com/FlorianBruniaux/agent-router.git
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

Les fixtures sont synthétiques, n'appellent aucun modèle et ne lancent aucun
host. Pour un build source, lancez les contrôles de version suivants depuis la
racine du dépôt :

```sh
./target/release/agent-router --version
./target/release/agent-router --help
```

Pour Cargo ou Homebrew, utilisez le chemin absolu du binaire installé.
Obtenez un clone autorisé de la même révision pour les deux fixtures et
remplacez les deux chemins ci-dessous. Les commandes copient uniquement ces
entrées, puis routent depuis un répertoire temporaire extérieur au clone :

```sh
source_checkout="/absolute/agent-router-checkout"
agent_router_bin="/absolute/user-prefix/bin/agent-router"
example_dir=$(mktemp -d)
cp "$source_checkout/tests/fixtures/requests/codex-implementation.json" "$example_dir/request.json"
cp "$source_checkout/assets/catalogs/codex-core-v1.json" "$example_dir/catalog.json"
(
  cd "$example_dir" || exit
  "$agent_router_bin" route --request request.json --catalog catalog.json \
    --json > decision.json
  "$agent_router_bin" explain --decision decision.json
)
rm "$example_dir/request.json" "$example_dir/catalog.json" "$example_dir/decision.json"
rmdir "$example_dir"
```

Sortie attendue, à la ligne du hash de décision près :

```text
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: 13
Missing evidence: 0
```

`Missing evidence: 0` décrit la fixture fournie, pas votre host installé ni ses
identifiants. Pour un build source, `agent_router_bin` peut désigner le chemin
absolu de `target/release/agent-router` dans le clone.

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
la [procédure de release locale](release-local.md). Elle s'appuie sur la
[distribution binaire](binary-distribution.md) (l'archive binaire) et sur la
[distribution des sources](distribution.md) (les archives source séparées). Rien, dans ces
étapes, ne signe, ne publie ni n'installe à votre place.

L'archive binaire v2 contient `agent-router` (features exactes `claude` et
`codex`) et `router-sandbox-helper`. Ce sont les mêmes composants produit par
défaut que ceux de la commande Cargo. Les features optionnelles demandent
toujours un build source séparé. La présence du helper ne configure aucun
sandbox et n'autorise aucune exécution : son chemin absolu `helper` et son hash
restent des entrées explicites.

### Homebrew, formule locale

`scripts/release-local.py verify` peut générer une formule Homebrew à partir de
`distribution/homebrew/agent-router.rb.in`, qui pointe vers l'archive locale via
une URL `file://` par défaut. La formule installe les deux binaires et les
documents de licence, et refuse une plateforme ou un CPU incompatible. Son
test autonome exerce route/explain et le refus d'arguments du helper, sans
lancer de host ni prouver le confinement natif.

Sur macOS ARM64, l'archive v2 complète a passé l'installation locale, le test de
formule, la réinstallation à version identique, un second test et la
désinstallation. Les hashes des deux binaires correspondaient à l'archive ; le
helper a refusé les arguments absents avec le code `125`, et la suppression a
préservé le marqueur sans lien avec l'installation. Le sandbox Homebrew est
resté actif, sans sandbox extérieur ni contournement de ses contrôles. Homebrew
a classé le préfixe personnalisé hors de `/tmp` en Tier 3 : cela ne qualifie ni
préfixe par défaut, ni tap public, téléchargement HTTPS, montée de version ou
autre plateforme. Voir la [qualification actuelle](bench/distribution-qualification-2026-10-07.md).

Utilisez un préfixe isolé déjà préparé hors de `/tmp`, avec le catalogue core
habituel de Homebrew disponible en local et ses contrôles conservés. L'essai a
copié un catalogue core complet, shallow et épinglé, après deux tentatives
d'autoclonage profond interrompues. Les révisions et contrôles d'environnement
exacts sont dans la qualification. Ce guide ne prépare aucun préfixe et ne
modifie pas le Homebrew du système ; l'ancien échec de sandbox imbriqué reste historique.

Une formule locale exige un tap local. Placez la formule vérifiée sous
`Library/Taps/local/homebrew-agent-router/Formula/agent-router.rb` dans le
dépôt de ce Homebrew, puis choisissez le chemin absolu de son exécutable :

```sh
brew_bin="/absolute/isolated-homebrew/bin/brew"
"$brew_bin" install local/agent-router/agent-router
"$brew_bin" test local/agent-router/agent-router
```

Le tap et l'archive doivent déjà exister. Ces commandes désignent votre tap
local, pas un tap amont publié. Une URL HTTPS canonique peut être rendue pour
une publication future ; cela ne téléverse aucun asset et ne vérifie pas son
existence ([attestation de release](release-local.md#archive-url-and-publication)).

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

Retirez les paquets avec le canal qui les a installés. Préservez les paquets,
fichiers et caches Cargo sans lien avec cette installation. Pour le même
préfixe que ci-dessus :

```sh
cargo uninstall --root "$install_prefix" agent-router-cli router-sandbox-helper
```

Pour Homebrew, utilisez le même exécutable isolé :

```sh
"$brew_bin" uninstall local/agent-router/agent-router
```

La qualification actuelle indique quelles désinstallations ont été observées.
Ces commandes ne suppriment ni état du runtime ni clone source.

1. Pour un build depuis un clone, retirez uniquement les sorties de build ou
   copies de binaires créées par cette installation. Inspectez les changements
   sans lien avec elle avant de supprimer le clone. Le cache Cargo est partagé.
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
- Elle ne publie ni ne signe de release. Cargo compile les sources ; Homebrew
  installe l'archive explicitement choisie.

Ensuite : [préparation](preparation.md), puis le
[guide d'une exécution unique](cli-execution.md) ou le [guide des workflows](coordinator.md).
