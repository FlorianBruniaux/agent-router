# Acceptation restante du projet complet

Audit de sources au 2026-09-27T12:20:48Z, depuis `7587302c59debc53523114b1684d906eb2e45dc5` avec modifications locales. Le dépôt évolue en parallèle : ce document fixe les exigences et les écarts observés, sans annoncer leur livraison distante. Aucun canary déjà vert n'a été relancé pour cet audit.

## Dénominateur et limites

Sources intégralement relues :

- [Spécification fonctionnelle](specs/functional-specification.md), SHA-256 `76302bdb52f89d9cc503eb158d645f69bb9002da28956739197142d03dbb7062` : **20 FR, 12 NFR et 10 critères de sortie MVP**, plus surfaces, presets et mesures de succès.
- [Spécification technique](specs/technical-specification.md), SHA-256 `9b56b73d751fb7781ae63636ffc825edc1ec21b0fedd2ad53745e3dcd6be1f98` : **21 sections et les 8 phases 0 à 7**. Les listes de crates/dépendances proposées ne sont pas des obligations de noms ; les comportements promis le sont.
- [Registre de livraison](completion.md), SHA-256 lors de la lecture `acc50170157a854c8593b1f56364e2f18f3906cddeae2f98a70d7ec27675abc7`. Une issue close ne vaut pas acceptation de toute une FR ou du projet complet.

Les lots Copilot/OpenCode déjà attribués ne sont pas réaudités ici et restent soumis à leur propre livraison et preuve native. La bibliothèque de promotion backend a reçu deux corrections indépendamment revues : politique de repli épinglée et absence de régression par famille. Son raccord à l'exécution est en cours séparément. Les entrées ci-dessous décrivent l'état avant ces raccords, sans les compter comme achevés.

`M` désigne une exigence fonctionnelle ou de sortie MVP encore partielle. `T` désigne un complément technique explicite ; son absence n'annule pas une fonctionnalité MVP équivalente. `E` désigne une extension des phases ultérieures, toujours comprise dans « tout le projet ». Les preuves externes sont regroupées en P1–P6 et ne sont pas transformées en travail logiciel fictif.

## Lots encore implémentables

### Avancement après l'audit initial

La table ci-dessous conserve le constat initial pour ne pas perdre le périmètre.
Les évolutions suivantes le complètent :

- **R14 livré** par la PR #19, merge `2261511e57870fb69dff67a8b8d28df854155294` :
  décision signée consommée par préparation, runtime, coordinator et feedback,
  avec revalidation au dispatch. L'activation réelle reste soumise à P1/P2.
- **R01 et R03 livrés par la PR #20** (`5525b1205d2db2a36d5b8fa6312f66a6756dbc9d`) : recherche sur cinq familles avec
  signaux de contexte et plancher de risque déterministe relié à la CLI.
- **R02 partiel** : mise à jour incrémentale et fusion RRF implémentées ; une
  source sémantique peut être fournie derrière une feature désactivée par défaut.
  Aucun fournisseur d'embeddings n'est encore exécuté, et P1 reste ouvert.
- **R13 partiel** : classification native des pannes implémentée et soumise à
  revue ; l'exécution de validations structurées reste à faire.
- **R12 partiel** : le code livré sur `5525b12` regroupe déjà le quota local
  par provider issu des exigences d'assets validées ; les identités inconnues
  partagent un groupe conservateur. L'équité entre workflows et les quotas
  partagés entre instances restent à faire. Le constat de table ci-dessous
  sur une clé par host est historique.
- **R16 partiel** : rendu humain des commandes produit, avec conservation des
  résultats JSON et digests d'approbation. La reprise du 28 septembre ajoute
  JSON/TOML/YAML stricts pour `config --layers` avec 16 tests dans chaque mode
  de compilation CLI. L'entrée guidée et les autres manifests restent séparés.
- **R05, CLI et coordinateur livrés** : journal, bornes approuvées,
  proposition et préparation/exécution CLI livrés par PR #22 (`cc8b3e4`).
  La PR #25 (`009b701`) livre le raccord coordinateur avec prompt final,
  grants, génération et contrôle du budget cumulé. Ses huit checks de PR
  et quatre checks post-fusion passent ; les cinq canaris CLI macOS séparés
  incluent deux scénarios coordinateur. Les compteurs demeurent attestés
  par le parent, pas observés.
- **R04, reprise du 28 septembre** : génération bornée, liaison CLI à la
  préparation et aux décisions effectives, migration v6→v7, ancre du plan dans
  le journal et refus de substitution revus. La preuve native et la livraison
  distante doivent accompagner la PR ; ce constat local ne clôt pas le lot.
- **R06, stockage, routage et préparation de bibliothèque livrés** : manifestes communs signés, hosts/presets,
  références de prompts et projections vérifiés par 42 tests catalogue.
  Le stockage durable du catalogue actif ajoute 15 tests : activation/révocation
  approuvées, révisions monotones, verrou interprocessus, expiration et refus
  après remplacement du répertoire ou du verrou. Les deux dernières régressions
  ont été reproduites puis revues indépendamment. La PR #23 (`4804b37`)
  livre le stockage ; la PR #27 (`9c47a2b`) applique les contraintes signées
  avant sélection et lie les receipts v4 au catalogue actif et à ses grants.
  Ses huit checks de PR et quatre checks post-fusion passent. La PR #31
  (`d4eafa1`) livre la préparation opaque, avec revalidation du catalogue,
  des grants et de la fraîcheur avant et après assemblage des seuls assets
  sélectionnés. Revue indépendante et huit checks de PR réussis ; 723 tests
  locaux passent, 27 ignorés. Le cycle CLI signé et le dispatch restent à
  raccorder. Voir [le stockage](catalog-store.md), [le routage signé](signed-routing.md)
  et [la préparation signée](signed-preparation.md).
- **R17, snapshot SQLite livré** : PR #26 (`4116045`), avec huit checks
  de PR et quatre checks post-fusion réussis. Une connexion déjà détenue
  permet une copie cohérente de WAL, bornée, privée et en mémoire, sans
  exporter le contenu ni changer la transaction par défaut de l'appelant.
  Les diagnostics CLI et les observations actives de hosts restent ouverts.
- **R11, client standard livré** : PR #28 (`2caf048`), avec huit checks de PR
  et quatre checks post-fusion réussis ; 731 tests locaux passent, 27 ignorés.
  Capacités, classement borné et exports ponctuels consentis sont vérifiés,
  ainsi que le délai commun à la négociation et au classement. Ce client
  ne livre pas le serveur R19. Voir [le protocole](standard-sidecar.md).
- **R20, preuves de licences et audit RustSec livrés** : PR #29 (`0fa3adf`)
  vérifie l'export réel strict des sources et ses notices ; PR #32 (`e1e8fe0`)
  audite le lock Cargo contre une base fraîche, sans exception d'advisory.
  Les huit et dix checks respectifs de PR passent. Le fuzz des parsers,
  le SBOM des binaires, la release signée et les gates P3/P6 restent ouverts.

### Candidat combiné du 28 septembre

Les PR #34 à #42 sont réunies localement sur `codex/project-integration`.
Le code à `1acec1d` passe 905 tests, sans échec, et six canaris macOS séparés.
Les 34 tests ignorés de la suite générale ne sont pas comptés comme réussis.
Les contrôles GitHub restent bloqués par la facturation, donc aucune livraison
fusionnée de ce candidat n'est revendiquée.

- **R12 / O3** : admission partagée, équité et allocations cumulées conservatrices
  implémentées ; la reprise conserve les engagements et les plafonds approuvés.
  Les coûts observés restent UNKNOWN. Voir [les budgets](session-budgets.md).
- **R16** : [la saisie guidée](task-guide.md) complète les faits explicites et
  exporte une tâche complète ; elle ne déduit aucune permission.
- **R17** : [doctor runtime](doctor.md) observe le schéma 9 sans reprise ni
  libération de réservation. Les diagnostics Database par chemin demandent
  désormais un snapshot détenu ; les régressions de verrous sont corrigées.
- **R18** : [les métriques de cohorte](evaluation.md) couvrent succès sous budget,
  coût/temps par succès et calibration. Regret/frontière et cache restent ouverts.
- **R06** : gardes et reconstruction des revues livrées en bibliothèque dans le
  candidat. La CLI de catalogue actif et le dispatch signé restent ouverts.
- **R20** : cibles fuzz, SQLite corrigé et [recette binaire locale](binary-distribution.md)
  présents. Deux builds de la source `1cf033e` sont identiques et leurs 276 textes
  de licence vérifiés ; cette preuve précède les correctifs de verrous de
  l'intégration. Les trois essais fuzz instrumentés sur l'archive exacte
  `b38b860` passent avec corpus et lockfiles inchangés ; les workspaces extraits
  passent aussi leurs suites hors ligne. Ces preuves sont locales à macOS.
  La reproduction finale par plateforme, la release signée et P3/P6 restent
  ouverts.

Le [registre de complétion](completion.md#combined-local-candidate-september-28)
détaille les résultats et les limites. La table suivante reste le constat
historique de départ ; ces compléments ne ferment pas les preuves externes.

Chaque ligne donne la source de l'exigence, le constat dans le code et une condition de clôture observable. Les références de code sont relatives à la racine du dépôt.

| Lot | Périmètre et exigence | Écart concret et preuve de code | Acceptation observable |
| --- | --- | --- | --- |
| R01 | **M**, FR-004 ; technique §6 récupération | `crates/router-retrieval/src/types.rs:6,29,80` ne couvre que agent/skill/prompt/tool. Aucun modèle, champ langage, outil requis ou historique de réussite dans la recherche. | Recherche séparée sur les cinq familles, champs manquants typés, exclusions stables, scores par source et tests où ces signaux changent réellement le classement. Aucun corps chargé avant sélection. |
| R02 | **T/E**, technique §6 récupération et phase 2 | `crates/router-retrieval/src/index.rs:40-58` reconstruit un index BM25 en mémoire ; pas d'API de mise à jour incrémentale, seconde source, RRF ou feature embeddings. Le moteur BM25 existant n'est pas un défaut par son seul choix de bibliothèque. | Ajout/suppression/modification incrémentaux équivalents à un rebuild ; fusion RRF déterministe et déduplication canonique ; embeddings désactivés par défaut, erreur/fallback explicites, aucune installation ou requête réseau implicite. Rappel validé séparément en P1. |
| R03 | **T**, technique §6 normalisation | `crates/router-workflow/src/normalize.rs:6-8,103` prend le risque explicitement fourni et pose les questions manquantes ; pas de classification déterministe imposant un plancher de risque à partir du contexte. | Règles versionnées et explicables, cas sensibles impossibles à abaisser par texte LLM ou override moins restrictif ; inconnus déclenchent clarification. Conserver l'autorité explicite sur permissions et budget. |
| R04 | **T**, technique §6 génération/score ; FR-006/007/010/013 | `crates/router-policy/src/phases.rs:719` score qualité/relevance/coût/latence d'un pool déjà fourni. `crates/router-workflow/src/plan.rs:31,75` compose des presets fixes. Pas de jointure top-k des dimensions, beam search borné, dominance/diversité ni coût de coordination/changement de modèle/cache/historique. | Générateur borné de plans éligibles avant scoring ; budgets cumulés ; dominance et diversité testées ; score et alternatives expliqués avec poids par classe. Un plan complexe proposé par modèle passe schéma, portée et approbation avant toute exécution. Aucun candidat ou droit ajouté par un backend. |
| R05 | **T, CLI et coordinateur livrés**, FR-006 et technique §9 runtime | `router-runtime/src/escalation.rs` reconstruit les tentatives et réserve une décision distincte ; `agent-router-cli/src/escalation.rs` expose bounds/proposal ; `router-coordinator` et la CLI reconstruisent le bundle enregistré et les grants exacts. | PR #22/#25 et leurs CI livrent les chemins testés : prédécesseur non vide vérifié, deux échecs distincts, budget restant, sélection secondaire, nouvelle approbation, reprise historique et refus de substitution/cumul. Baseline et fallback manuel préservés ; aucune indisponibilité ni mesure de coût inventée. |
| R06 | **M/T**, FR-005/008/017/018 ; technique §5 assets, §13 | `crates/router-catalog/src/generated.rs:18,39` expose modèle/agent/skill/prompt/tool, sans catalogue de hosts/presets. `crates/router-types/src/asset_contracts.rs:54,75` lie provenance/licence/hash des skills mais pas signature d'éditeur, auteurs et validité commune de tous les assets. | Catalogue versionné couvrant hosts/presets ; manifeste commun avec provenance/licence/auteurs/période de validité ; validation de signature avec clé approuvée extérieurement ; suppression/modification invalide index/cache. Un hash réémis ne crée jamais un éditeur fiable. |
| R07 | **E**, FR-005 ; technique §5 ModelSpec, §11 API | Les contrats déclarent audio/images/fichiers/citations/JSON structuré, mais `crates/router-policy/src/asset_phases.rs` refuse les exigences que `ExecutionRequest` texte seul ne représente pas. Le refus est correct ; le transport multimodal n'existe pas. | Entrées et sorties typées bornées pour chaque modalité prise en charge ; schéma transmis au vrai adaptateur ; tests de contraintes et flux tronqués ; preuve native par modèle/host/version avant `Supported`. Aucun élargissement à partir d'une déclaration enfant. |
| R08 | **E**, FR-008/018 ; technique §5/13 | `crates/router-asset-contracts/src/gate.rs:289` renvoie `ExecutableUnsupported` pour une skill exécutable ; seule l'instruction textuelle approuvée peut être préparée. | Runner distinct à argv typés, artefact et dépendances épinglés, revue/scanner/approbation des bytes, permissions réduites et sandbox CPU/mémoire/PID/réseau ; fixtures de code tiers refusé et canary local confiné. Ne pas assimiler instruction et programme. |
| R09 | **M/T**, surfaces fonctionnelles §7 ; technique §11/12 et phase 4 | `crates/router-adapter-api/src/execution.rs:299,357` dégrade `NativeSubagent` en `ExternalWorker` ou refuse. `integrations/codex/adapter.json` et `integrations/claude-code/adapter.json` sont des descripteurs ; les skills/hooks sous le harness servent au développement du dépôt. | Packages produit Codex/Claude avec skills courtes, agents et hooks propres au contrat natif vérifié ; enveloppe/scope/approbation contrôlés ; collecte d'outcome sans auto-validation ; installation projet réversible et inactive avant approbation. CLI et App testées séparément ; handoff Tasks uniquement si API prouvée et demande utilisateur distincte. |
| R10 | **E**, surfaces API ; technique §11 API proxy, phase 5 | `integrations/bridge-contract/src/provider.rs:26,195` prépare et vérifie une enveloppe Ed25519 et des requêtes provider, avec decode de streams. Aucun transport HTTP exécutant `PreparedProviderRequest` n'est raccordé au runtime. | Transport HTTP supervisé vers endpoint approuvé, secrets hors logs/argv, enveloppe revalidée avant envoi, délais/cancellation/bornes/redaction et erreurs normalisées ; serveur factice pour streams tronqués/forgés/429/401. Aucun droit workspace accordé par une API de génération. Canary réel seulement avec credentials et coût autorisés. |
| R11 | **T/E**, technique §7 contrat sidecar | `crates/router-decision/src/sidecar.rs:80` utilise `/v1/systemone`. Les endpoints standard `/v1/capabilities`, `/v1/rank`, `/v1/outcome`, `/v1/feedback` promis n'existent pas dans ce contrat. | Version/capacités négociées, rank borné et soumis au pool ; outcome/feedback respectent le consentement ; tests de protocole incompatible, timeout lent, mauvais schéma et fallback. L'adaptateur legacy reste explicite. |
| R12 | **M/T**, FR-011 ; technique §9 concurrence | `crates/router-runtime/src/lib.rs:967` indexe le quota dit provider par `request.host`. Deux harnesses vers le même provider possèdent donc deux quotas. Les sémaphores sont locaux à une instance `Runtime` ; pas de politique équitable entre petits et grands workflows. | Identité provider validée issue du contexte, quota commun entre hosts et instances supervisées ; isolation des providers distincts ; petit workflow progresse sous charge ; raisons de queue exactes, cancellation sans fuite de permis. Ne pas réétiqueter un quota host en quota provider. |
| R13 | **M/T**, FR-011/012 ; technique §9 processus | `crates/router-types/src/runtime.rs:57` définit Authentication/RateLimit/Network, mais `crates/router-runtime/src/lib.rs:693` transforme toute autre sortie non nulle en Crash. `record_verification:1015` accepte une attestation externe ; `WorkflowNode.verification` reste `Vec<String>`, sans exécuteur de validations argv ou assertions structurées. | Classes de panne issues uniquement de métadonnées natives fiables, inconnu/crash sinon ; retry selon classes autorisées. Contrats de validation typés, commandes argv confinées et résultats liés à l'artefact réel. Séparer command validation, avis indépendant et objectif ; conserver l'attestation humaine comme telle. |
| R14 | **M/E, raccord en cours**, FR-014/016 ; technique §7/8 et phase 6 | `router-backend-policy` produit une preuve opaque signée ; `router-runtime::PendingRun` conserve encore un receipt déterministe seul à l'instant de l'audit. `router-decision` reste shadow. Une promotion valide de bibliothèque ne change donc pas encore la sélection exécutée. | Preuve de décision active consommée par préparation/runtime/coordinator, baseline intacte ; signature, politique épinglée, racines externes, état courant et TTL exact vérifiés au dispatch ; rollback entre review et lancement bloque. Toutes les gates v3 restent obligatoires. P1 conserve l'activation réelle fermée. |
| R15 | **E**, FR-016 ; technique §8 et phase 6 | `crates/router-feedback/src/learning.rs:110` calcule Beta/statistiques/préférences/drift. `crates/router-feedback/src/policy.rs:177` reçoit une politique proposée par l'appelant ; pas de bandit contextualisé, estimation de choix ou décroissance temporelle. | Propositions apprises bornées sur données consenties, exploration seulement shadow/canary à faible risque, seed/replay reproductible et propension exacte journalisée ; décroissance testée ; UNKNOWN exclu des labels positifs ; gains et non-régressions par sous-groupes avant activation signée. |
| R16 | **M/T**, FR-013, modes §4 ; technique §14/15 | `crates/agent-router-cli/src/product.rs:42,59,126,140`, `phases.rs` et `coordinator.rs` imposent `--json`. Les commandes produit se pilotent par fichiers/manifests ; seule la CLI initiale dispose d'un rendu humain. Les nouvelles politiques sont JSON, sans chargement TOML/YAML du contrat proposé. | Sorties humaines compactes pour route/plan/explain/run/status/doctor/feedback, dérivées du même résultat versionné ; chaîne explicative relie assumptions, retrieval, exclusions, choix et preuves manquantes. Entrée texte/fichier guidée conservant les clarifications d'autorité ; format de configuration documenté et validation stricte. Les différences de nom de commande seules ne sont pas un défaut. |
| R17 | **M/T**, FR-019 ; technique §15 doctor | `router-doctor` inspecte une liste fournie. `deep.rs` ajoute vrais validateurs, corps et SQLite, mais l'auth/chargement des hooks/liveness restent inconnus ; `database.rs` ne certifie pas les layouts Linux `/dev/fd` et exige un export cohérent pour WAL. | Diagnostics actifs séparément autorisés, bornés et sans appel modèle payant, sur primitives natives prouvées ; provenance/fraîcheur explicites. Voie sûre de snapshot SQLite cohérent vérifiable sous Linux ; migration attendue/état de schéma, locks et orphelins distingués. Aucun succès runtime déduit du fichier, PID ou hash. |
| R18 | **T/E**, technique §10/13 ; mesures fonctionnelles §10 | Pas de cache de décision/retrieval durable avec clé complète host/version/policy/catalog/risque/tâche redacted. `crates/router-eval/src/promotion.rs:58` expose accuracy/coût/latence/abstention/outcomes, sans tout le tableau produit : succès@budget, regret/frontière, Brier/ECE, acceptation du premier plan, faux positifs scanner et couverture des preuves. | Cache invalidé sur chaque dimension, isolation des secrets, reprise cohérente. Métriques calculées sur dénominateurs figés avec UNKNOWN conservé ; golden tests de calibration/regret/coût par succès et observabilité des décisions. Aucun gain produit affirmé sans données P1. |
| R19 | **E**, technique §1/16 | Aucun serveur produit ni commande daemon dans `crates/agent-router-cli/src/command.rs`. Le client loopback d'un sidecar est distinct du serveur local promis. | API versionnée capabilities/route/plan/execute/events/outcome/feedback/artifacts/health, Unix socket par défaut, TCP loopback opt-in et token 0600 ; SSE borné et reprise ; tests d'accès et de cancellation. Routing CLI autonome sans daemon. |
| R20 | **T**, technique §17/18 | `.github/workflows/ci.yml` couvre fmt/clippy/tests/harness/secrets sur macOS/Linux. Pas de job de fuzz des parsers ni audit explicite dépendances/licences ; pas de construction de binaries de release avec SBOM lié à target/features. MSRV 1.85 est fixé dans `rust-toolchain.toml`, ce point n'est pas absent. | Corpus fuzz pour contrats/streams/paths et budget CI borné, audit dépendances/licences avec exclusions motivées, artefacts reproductibles par target/features et SBOM/checksums du binaire effectivement distribué. Les optimisations LTO/strip sont retenues après mesures. |
| R21 | **T/E**, technique §18 release | `scripts/package-core.py` distribue et vérifie les **sources** core/intégrations. `docs/distribution.md` distingue intégrité et signature ; aucun workflow Homebrew ou signature d'archives/binaires. | Release candidate reproductible, signature vérifiable avec clé externe protégée, archives/checksums/notices par plateforme et formule Homebrew testée sur archive locale. Publication séparée après P3 ; aucune activation de host à l'installation. |
| R22 | **E**, NFR-007 et technique §18 | Runtime/doctor/sandbox utilisent les primitives Unix ; `router-doctor/src/files.rs` retourne Unsupported hors Unix. Pas de cible Windows dans CI. | Contrats Windows de permissions privées, process tree/job, confinement et paths ; mêmes suites host/runtime et reprise sur runner Windows, refus explicites des capacités non garanties. |
| R23 | **E**, technique phase 7 | Pas de TUI/web locale, catalogue de packs signés, marketplace ou importers dans le workspace et la CLI. | Une surface locale TUI **ou** web sur contrats stabilisés, revue/approbation visible ; packs/importers versionnés et signés, provenance/licence/quarantaine, import idempotent et rollback. Aucun téléchargement/activation silencieux ; UI web applique les protections de §16. |

## Compléments Omnigent, 2026-09-28

La [comparaison et ses sources](omnigent-comparison.md) ajoutent six acceptations
aux lots existants, sans remplacer le périmètre R01–R23 ni les gates P1–P6 :

- **O1, R11/R19** : traduction stricte de l'API de routage externe Omnigent,
  puis endpoint local opt-in. Un refus ne vaut pas blocage natif tant que le
  repli Omnigent vers le harness par défaut n'est pas maîtrisé.
- **O2, R04/R09/R13** : revue par fournisseur distinct avec identité prouvée et
  comportement explicite si aucun reviewer indépendant n'est disponible.
- **O3, R10/R12** : compteurs de budget et seuils cumulatifs par session,
  persistants et partagés entre instances ; restrictions monotones conservées.
- **O4, R08/R10** : broker de credentials sans secret dans le processus enfant,
  testé sur serveur local et destination bornée avant toute activation.
- **O5, R19/R23** : reprise/fork locaux et contrôle humain visible ; nouveau
  contexte d'autorité, sans copie d'approbation exécutable depuis le parent.
- **O6, R06/R09/R23** : registre d'adaptateurs versionnés et signés, découverte
  inerte, noms réservés, installation réversible et preuve native séparée.

O1 possède une traduction hors ligne revue, avec dix tests de contrat ;
l'endpoint et la preuve côté Omnigent restent à livrer. Les autres points sont
planifiés. La documentation d'un autre produit n'est pas une preuve de livraison
dans ce dépôt. Les critères de test sont définis dans la comparaison ; aucune
installation Omnigent ni autorisation globale n'est requise pour leur première
validation locale.

## Ordre et dépendances

1. R12/R13 corrigent les contrats de runtime ; R14 termine le raccord déjà attribué. Ils se valident sans modèle réel.
2. R01/R02 et R03/R04 constituent deux chaînes indépendantes, puis se rejoignent dans préparation et explication. R05 dépend des tentatives classées de R13.
3. R06 précède packs et skills exécutables. R07/R08 sont des chemins d'exécution nouveaux avec approbations propres, pas des relaxations des refus actuels.
4. R09 et les lots Copilot/OpenCode réservés avancent séparément de R10/R11. Leurs matrices natives sont conservées distinctes.
5. R15/R18 consomment les traces consenties ; ils n'ouvrent pas le gate humain. R16/R17 peuvent avancer avec des fixtures sans credentials.
6. R20/R21 préparent une release révisable ; R19/R22/R23 restent nécessaires au projet complet après stabilisation des contrats.

## Gates de preuve qui ne sont pas « implémentés »

| Gate | Exigence | Preuve restante et limite |
| --- | --- | --- |
| P1 | Sorties MVP 2/3 ; FR-016 ; technique phases 0/2/6 | Les 100 cas et replays synthétiques ne sont pas 100 labels humains indépendants. Figer reviewers, familles tenues à l'écart, seuil rappel@5 et critères avant mesure ; obtenir de vraies observations. Aucune activation apprise/semantic en production à partir des tests positifs. |
| P2 | Sorties MVP 1/9 ; FR-001 et surfaces | Matrice versionnée Codex CLI/Claude réels, puis App et autres hosts distincts ; effet natif, modèle et effort observés ou UNKNOWN. Un fake host vérifie le superviseur, pas le fournisseur. Les lots Copilot/OpenCode en cours doivent déposer leurs propres preuves ; aucun canary déjà vert n'est demandé une seconde fois ici. |
| P3 | Sortie MVP 10 ; technique §18 | L'inventaire et les textes de licences existent ; `licenseConcluded: NOASSERTION` reste une absence de conclusion de compatibilité. Valider licences/notices des assets exacts et de la release finale ; les clés de signature et autorisations de publication restent des entrées externes. |
| P4 | NFR-001/002/003 | Les mesures de décision/index local sont documentées ; la première ouverture CLI mesurée dans `docs/startup-investigation.md` dépasse 150 ms. Ne pas remplacer ce constat par le temps chaud ni annoncer le seuil atteint. Établir les causes restantes et répéter seulement sur une nouvelle version ou condition de mesure pertinente, sans désactiver la sécurité OS. |
| P5 | NFR-004/005/006/007 ; sorties MVP 5/6 | Préserver les preuves natives macOS déjà vertes et obtenir le verdict de la matrice Linux finale avec le même code livré. Les garanties cleanup groupe/namespace et mémoire par OS restent explicites ; `GroupTerminated` ne prouve pas tous les descendants détachés ni une limite mémoire non appliquée. |
| P6 | Toutes sorties MVP et release | Vérification sur SHA livré : CI distante, archives finales, scans et workspaces extraits, états GitHub des issues/PR. Une réussite locale ou un document ne remplace pas cette preuve. |

## Couverture du dénominateur

Cette table évite de réduire le périmètre aux seuls lots faciles à implémenter. « Aucun ajout isolé » signifie qu'aucun nouveau trou de code n'a été identifié ici ; les gates de preuve et la validation de livraison restent applicables.

| Exigence fonctionnelle | Reste identifié |
| --- | --- |
| FR-001 découverte | R09, R17, P2 |
| FR-002 normalisation | R03 (complément technique), R16 |
| FR-003 DAG | R04 (génération technique avancée), P5 ; DAG presets et suggestion de preset simplifié déjà présents |
| FR-004 retrieval | R01, R02, P1 |
| FR-005 contraintes | R06/R07 ; les modalités non représentables et le reasoning reuse sont actuellement refusés, sans contournement identifié |
| FR-006 modèle/effort | R04/R05, P2 |
| FR-007 agents | R04/R09, P2 |
| FR-008 skills | R06/R08 ; instruction v3 et review liée aux bytes présentes |
| FR-009 prompts | Aucun ajout isolé : templates typés/versionnés et préparation liés aux hashes présents |
| FR-010 plan-execute-verify | R04/R09/R13 ; coordinator, handoff d'artefacts et vue de vérification déjà présents |
| FR-011 runtime | R05/R12/R13, P5 |
| FR-012 vérification | R13 ; transport, validation et objectif restent distincts |
| FR-013 explication | R04/R16/R18 |
| FR-014 couches | R14 pour consommation de décision active ; restrictions monotones déjà présentes |
| FR-015 feedback | Aucun ajout isolé : consentement, séparation auto/humain, rétention et purge présents |
| FR-016 apprentissage | R14/R15/R18, P1 |
| FR-017 catalogues | R02/R06 |
| FR-018 confiance | R06/R08 ; scanner/revue/approbation d'instructions présents, P3 |
| FR-019 doctor | R17, P2/P5 |
| FR-020 audit | Aucun ajout isolé : projection typée allowlist et chaîne vérifiable présentes, P6 |

NFR-001/002/003 restent suivies en P4 ; NFR-004/005/006 en P5 ; NFR-007 en P5/R22 ; NFR-008 dans R16/R18 et les receipts existants ; NFR-009/010/011 dans les tests de déterminisme, refus UNKNOWN et absence de réseau par défaut, à préserver dans chaque lot ; NFR-012 dans les gates CI existantes et R20. Les dix sorties MVP sont conservées : 1→P2, 2→P1/P6, 3→R01/P1, 4→P6, 5→P5, 6→P5, 7→P6, 8→P6, 9→P2, 10→P3.

Les sections techniques d'architecture/domaine/configuration/CLI renvoient à R03/R04/R06/R16 ; retrieval à R01/R02 ; policy/learning à R11/R14/R15/R18 ; runtime/store à R05/R12/R13/R17/P5 ; adapters/plugins à R07/R09/R10 et aux lots hosts réservés ; sécurité à R06/R08/R17/R20 ; API à R19 ; tests/release à R20/R21/R22/P1–P6 ; phase 7 à R23. Les exemples de seed, choix de noms de crates et durées indicatives de roadmap ne sont pas transformés en critères de succès.
