# Contrat de liaison runtime natif et mesures

Date : 2026-10-10. Base lue : `cb3be612bc25b76d9c8cd3ec3fe08bd29b05f2db` (worktree `claude-c1`).
Statut : proposition de contrat, étape 2 de la passation. Aucun format partagé n'est modifié par ce document.

## TL;DR

Le journal runtime capture aujourd'hui le run, la tentative logique (ordinal 1 à 5), l'admission locale de la tentative, le modèle et l'effort demandés, la terminaison du superviseur, une durée superviseur non qualifiée et la révision produite. Rien dans le runtime ne produit d'identité d'envoi physique, de thread, de tour, de modèle appliqué, d'usage durable ou de verdict d'acceptation : ces faits sont soit `typé UNKNOWN` dans l'observation v2, soit `non trouvé`. Le premier lot B implémentable sans les lots natifs est une jointure hors ligne, à preuves fournies, entre les clés de tentative v2 et les clés physiques F07a, qui refuse collision et substitution et laisse UNKNOWN tout ce qui n'a pas de producteur.

## Sources lues

- ADR : `docs/adr/0024` à `0028`.
- Contrats : `docs/reference/json-contracts.md` (tableau des versions, lignes 26 à 50 ; usage hôte, lignes 182 à 239).
- Producteurs : `crates/router-runtime/src/store.rs`, `store/escalation.rs`, `store/admission.rs`, `admission/mod.rs`, `admission/runtime.rs`, `lib.rs`, `process.rs`, `integration.rs`, `finite_client.rs` ; `crates/router-adapter-api/src/execution.rs` ; `crates/agent-router-cli/src/execution_feedback.rs`.
- Consommateurs : `crates/router-runtime/src/observation.rs`, `store/observation.rs`, `escalation.rs` ; `crates/agent-router-cli/src/run_observation.rs` ; `crates/router-feedback/src/accepted_changes.rs` ; `crates/agent-router-cli/src/schemas/run-observation.schema.json`.
- Issues lues le 2026-10-10 par `gh issue view` : #120, #117, #116, #118, #5, #133.

## Hypothèses de la passation, recoupées dans le code

| Hypothèse | Verdict | Source |
| --- | --- | --- |
| v1 est la sortie par défaut, v2 opt-in conserve les octets v1 | Confirmé par le code : la branche v2 n'est prise que si `schema_version == 2`, sinon `observe_run` v1 | `crates/agent-router-cli/src/run_observation.rs:14-31` |
| v2 expose identité de tentative, modèle et effort demandés, terminaison superviseur, références de preuve | Confirmé | `crates/router-runtime/src/store/observation.rs:292-382` |
| Modèle appliqué, usage, durée, objectif, notifications sont typés inconnus | Confirmé : type `ObservationUnknown` à variante unique, et `const: "unknown"` dans le schéma publié | `crates/router-runtime/src/observation.rs:51-55`, `146-159` ; `run-observation.schema.json` (`$defs.native`) |
| L'identité physique F07a combine espace d'autorité et identité physique | Confirmé | `crates/router-feedback/src/accepted_changes.rs:107-110` |
| Le calcul hors ligne annonce `supplied_unverified` | Confirmé | `crates/router-feedback/src/accepted_changes.rs:606` |
| Une identité de run peut dépendre du digest d'approbation | Confirmé, et le retry change ce digest | `store/observation.rs:163-167` ; `store.rs:1941-1948` |

## 1. Inventaire des faits candidats

Légende propriétaire : **A** = lot natif (producteur), **B** = lot mesures (jointure, consommation). Un fait déjà capturé reste produit par le code existant ; B le consomme sans le modifier.

| # | Fait | Producteur (fonction qui écrit) | Identité et domaine d'unicité | Complétude | Propriétaire | Consommateur | Statut |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Tâche | `task_execution_sha256` calculé par `escalation.rs:586-599`, écrit dans `attempt.evidence.v1` par `Store::observed_attempt` (`store/escalation.rs:44-49`) | Digest de contenu de la requête (workspace, source, scope, prompt, mode, cible). Deux affectations identiques produisent le même digest : ce n'est pas une identité d'affectation F07a | Absent si la tentative n'a pas d'`AttemptEvidence`. Non projeté par l'observation | A (existant), B (lien vers `assignment_id`) | Escalade (`escalation.rs:487`) | `capturé aujourd'hui` (journal, non projeté) |
| 2 | Run | `Store::register_workflow` : `INSERT INTO runs` (`store.rs:317-325`), événement `planned` (`store.rs:340`) | `run_id` clé primaire par store (`store.rs:69`). Projection : `run_key_sha256` = `(run_id, definition_hash, approval_digest)` | `run_key` change au retry (`store.rs:1944-1947`). Aucune identité de store exposée, donc pas d'unicité inter-stores | A (existant) | Observation v1/v2, feedback (`execution_feedback.rs:82`) | `capturé aujourd'hui` |
| 3 | Tentative logique | `Store::attempt` : `run.attempts += 1`, `INSERT INTO run_attempts` (`store.rs:823`), `attempt.started` (`store.rs:827`) | Ordinal 1 à 5 par `run_id` (clé `(run_id, attempt)`, `store.rs:70`). Clé v2 `attempt_key_sha256` = `(run_id, definition_hash, ordinal, started_event_hash)` (`store/observation.rs:294-302`), stable au retry, unique dans une chaîne de journal | Toujours présente pour un départ durable. `evidence: unknown` si l'`AttemptEvidence` manque | A (existant) | Observation v2, B1 | `capturé aujourd'hui` |
| 4 | Tentative physique (envoi) | Aucun producteur trouvé. Cherché : `physical`, `request_id`, `settlement`, `send` dans `crates/` et `integrations/` hors tests | Néant | Un processus lancé n'est pas un envoi physique (#133) | A | B1, F07a | `non trouvé` |
| 5 | Session / thread natif | Aucun producteur durable. `bind_failure_session` lit `session_id` pour vérifier la cohérence d'un flux d'échec puis l'abandonne (`router-adapter-api/src/execution.rs:442-454`). L'exemple de diagnostic imprime des ids RPC sur stdout (`examples/finite_client_supervision.rs:413-418`) | Néant dans le journal | `thread_id: unknown` imposé par le type | A | Observation v2, #120 | `typé UNKNOWN` |
| 6 | Tour (turn) | Aucun producteur durable trouvé. Cherché : `turn_id`, `turnId` | Néant | `turn_id: unknown` imposé par le type | A | Observation v2, #120 | `typé UNKNOWN` |
| 7 | Appel fournisseur | Aucun producteur runtime trouvé. F07a accepte une clé fournie `(authority_sha256, physical_sha256)` (`accepted_changes.rs:107-110`) ; `external_request_id` n'est jamais une clé (ADR 0024) | Côté fourni : unicité par paire autorité/identité physique | Toujours `supplied_unverified` | A (producteur), B (consommation fournie) | F07a | `non trouvé` (runtime) |
| 8 | Admission locale de tentative | `Store::start_admission` (`store/admission.rs:53-84`) et `persist` qui écrit `admission.intent.v1` (`store/admission.rs:230-234`) ; `admission.queued` (`admission/runtime.rs:164-166`) | Ticket scellé `runtime-admission-ticket-v1` (`admission/mod.rs:115-118`) ; `admission_sequence` 1 à 4096 par run (`store/admission.rs:68`) ; `ticket.attempt = run.attempts + 1` (`store/admission.rs:73`) | Admission du lancement, pas de chaque envoi. Non projetée par l'observation (préflight seulement, `store/observation.rs:94-101`) | A (existant) | Nettoyage de l'observation | `capturé aujourd'hui` |
| 9 | Admission par envoi physique | Aucun producteur runtime trouvé. Le broker qualifié par ADR 0026 vit dans l'outillage de diagnostic `docs/bench/`, non lu ici | Néant | #133 F03b ouvert | A | #133, B1 | `non trouvé` |
| 10 | Réception fournisseur simulé | Aucun producteur durable trouvé. Compteur en mémoire dans un exemple (`examples/finite_client_supervision.rs:429-430`) | Néant | Pas de persistance | A | #133, B1 | `non trouvé` |
| 11 | Modèle demandé | `Runtime::execute` remplit `AttemptEvidence.model` (`lib.rs:1412`), écrit par `store/escalation.rs:44-49` | Par tentative durable | `null` si l'`AttemptEvidence` manque. Identifiant restreint à `[A-Za-z0-9-_.:/]`, 256 octets (`store/observation.rs:334-341`) | A (existant) | Observation v2, escalade | `capturé aujourd'hui` |
| 12 | Modèle déclaré par l'hôte | `decode_stream` produit `RuntimeEvidence.executed_models` (`lib.rs:1521-1538`), émis dans la sortie JSON de `run` (`agent-router-cli/src/execution.rs:1272`). Non persisté : le journal ne garde que `protocol.observed` + `decoded_not_verified` (`lib.rs:1526-1533`) ; le feedback enregistre le modèle sélectionné (`execution_feedback.rs:341`) | Néant de durable | `observed_model: unknown` imposé par le type | A | Observation v2, #120 | `typé UNKNOWN` |
| 13 | Identité d'inférence | Aucun producteur trouvé. Cherché : `inference_id`, `served_model`, `response_model`, `provider_model`, `system_fingerprint`, `upstream_model` | Néant | Une requête qui nomme un modèle ne prouve pas l'inférence réelle (ADR 0026) | A | #120, F07b | `non trouvé` |
| 14 | Effort demandé | `AttemptEvidence.effort` (`lib.rs:1413`), même écrivain que le #11 | Par tentative durable | `null` si l'`AttemptEvidence` manque | A (existant) | Observation v2 | `capturé aujourd'hui` |
| 15 | Effort appliqué | `RuntimeEvidence.executed_effort` en mémoire seulement (`router-types/src/execution.rs:221`) | Néant de durable | `observed_effort: unknown` imposé par le type | A | Observation v2 | `typé UNKNOWN` |
| 16 | Usage tokens | Hors journal : `measurements()` copie `host_usage` dans le store feedback opt-in (`execution_feedback.rs:297-311`), provenance `host_reported`, clé `(run_key, ordinal)`. Dans le journal, `observed_tokens` et `observed_cost_microusd` sont toujours écrits `None` (`lib.rs:1419-1420`) | Feedback : `(run_key_sha256, ordinal)`, donc rompu par un retry qui change `run_key` | Usage déclaré par l'hôte, non mesuré ni facturé. `usage: unknown` imposé dans l'observation | A (producteur qualifié), B (consommation) | Rapport feedback, F07b | `typé UNKNOWN` (observation) ; capturé hors journal sous consentement |
| 17 | Durée superviseur de tentative | `capture_guarded` : `Instant::now()` avant la validation de dispatch (`process.rs:172`), `elapsed_ms` après le reaping et les deux collectes bornées stdout/stderr ; écrit dans `AttemptEvidence.transport` | Par tentative durable | Inclut validation, SIGTERM/SIGKILL, attente de reaping et les deux collectes bornées stdout/stderr ; exclut l'écriture journal. Non qualifiée par #117, non projetée (`duration: unknown`) | A (existant), #117 qualifie | Escalade (`escalation.rs:516-518`), feedback `latency_ms` (`execution_feedback.rs:305`) | `capturé aujourd'hui` (non qualifié) |
| 18 | Frontières temporelles de phase / tentative | Aucun producteur trouvé : `RuntimeEvent` n'a aucun champ temporel (`router-types/src/runtime.rs:103-111`). Seules horloges murales : `observed_at_unix_seconds` et `started_at_unix_seconds` du feedback (`execution_feedback.rs:36`, `57`) | Néant | Duration `unknown` dans v1 et v2 | A | #117, #118 | `non trouvé` |
| 19 | Terminaison | `AttemptEvidence.transport` (`termination`, `exit_code`), écrit par `store/escalation.rs:44-49` ; transition `transport_observed` (`store/escalation.rs:41`) ; nettoyage `process.cleanup_confirmed` / `cleanup_unknown` (`store.rs:903`, `909`) | Par tentative durable | Terminaison superviseur seulement. Une annulation acquittée ne prouve pas l'arrêt (ADR 0025) | A (existant) | Observation v2 | `capturé aujourd'hui` |
| 20 | Révision produite | `Store::artifact` : `artifact_sha256` et événement `artifact` (`store.rs:993-1005`) ; publication : `integration.intent` (`store.rs:763`) puis `integration.published` avec `PatchPublicationReceipt` (`integration.rs:353-357`) | Table `artifacts` : une ligne par `(run_id, kind)`, remplacée (`INSERT OR REPLACE`). Le lien à l'ordinal n'est que positionnel dans le journal. Reçu : `result_revision` OID Git | État `Unknown` si l'événement terminal manque (ADR 0027). Aucun lien défini entre OID Git et `revision_sha256` F07a | A (existant), B (mappage) | `observe_publication`, F07a (fourni) | `capturé aujourd'hui` |
| 21 | Révision acceptée | Aucun producteur runtime trouvé. `record_verification` (`store.rs:699-736`) enregistre un verdict de vérificateur, pas une acceptation ; la publication n'émet pas de fait `integration: applied` (ADR 0027). Verdicts fournis à F07a (`accepted_changes.rs:65-74`) | F07a : `RevisionBinding` exact, séquences fournies | `supplied_unverified` | B | F07a, F07b | `non trouvé` (runtime) |

Décompte : 9 `capturé aujourd'hui`, 5 `typé UNKNOWN`, 7 `non trouvé`, sur 21 lignes. Les lignes 8/9 et 14/15 séparent l'admission et l'effort en deux faits, parce que leurs producteurs diffèrent.

## 2. Règles de jointure minimales pour B1 (#120)

### Clés autorisées

| Lien | Clé | Raison |
| --- | --- | --- |
| Tentative runtime | `attempt_key_sha256` v2 | Stable au retry, contrairement à `run_key_sha256` (`store.rs:1944-1947`) |
| Run au moment d'une approbation | `run_key_sha256` | Lie la liaison d'approbation courante, rien de plus (ADR 0025) |
| Appel physique | `(authority_sha256, physical_sha256)` fourni | Règle F07a. `external_request_id` reste un affichage |
| Instantané runtime | `snapshot_sha256` v2 | Toute affirmation de jointure cite l'instantané qu'elle lie |

Clés interdites : `external_request_id`, `task_execution_sha256` comme identité d'affectation, un identifiant de modèle, un ordinal seul, un horodatage.

### Observation dupliquée et nouvel appel

- Même clé physique, contenu identique : une seule observation.
- Même clé physique, un champ différent (tentative revendiquée, modèle déclaré, séquence, preuve) : conflit, refus du rapport entier.
- Clé physique nouvelle : nouvel appel, même si `external_request_id` se répète.
- Une tentative peut porter plusieurs clés physiques : un client peut réessayer à l'intérieur d'une tentative. Leur nombre reste `unknown` tant qu'aucun inventaire complet par tentative n'est fourni ; aucun producteur runtime n'en écrit aujourd'hui.

### Refus de substitution et de collision

- Clé de tentative revendiquée absente de l'instantané fourni : refus (substitution).
- Deux tentatives fournies avec la même clé, ou ordinaux hors 1 à 5, dupliqués ou non contigus depuis 1 : refus.
- Clé physique dont l'autorité égale l'autorité déclarée pour le runtime : refus. Une capture externe ne devient jamais un reçu d'exécution AR (#120).
- Tentative `evidence: unknown` accompagnée d'une sélection demandée non nulle, ou `evidence: durable_attempt` sans sélection demandée : refus, la forme contredit v2 qui pose les deux ensemble (`crates/router-runtime/src/store/observation.rs:303-304` et `370-374`).
- Modèle déclaré côté physique différent du modèle demandé : la jointure reste, `selection_agreement: mismatch` est rapporté. Le modèle n'est jamais une clé et ne corrige jamais le modèle demandé.

### Ce qui reste UNKNOWN

- Tentative sans revendication physique : `physical_sends: unknown`. Jamais zéro, jamais `failed_before_send` (ADR 0024).
- Observation physique sans tentative revendiquée : `unattributed`, conservée au niveau cohorte comme F07a, sans répartition par tentative.
- Thread, tour, usage, durée, objectif : non joints par B1 ; ils restent ceux de l'instantané v2, donc `unknown`.
- Unicité inter-stores : l'observation n'expose aucune identité de store. L'autorité runtime est fournie et non vérifiée.

## 3. Points de changement de contrat partagé

À ne faire qu'après arbitrage. Aucun de ces points ne remplit un champ existant qui n'accepte que UNKNOWN.

| Surface | Changement nécessaire | Version qui évolue |
| --- | --- | --- |
| Observation | Tout fait natif qualifié (thread, tour, modèle ou effort appliqué, usage, durée, notifications) exige de nouveaux champs : les champs `native.*` sont `const: "unknown"` et `ObservationUnknown` n'a qu'une variante | Prochain schéma explicite, avec nouveau domaine et registre ; v3 qualifie seulement l'intervalle local F05b-a, sans champs natifs connus ; octets des versions précédentes inchangés |
| `router-types::AttemptEvidence` | Identités d'envoi physique, thread/tour, modèle déclaré, usage avec provenance. Le type refuse les clés inconnues (`router-types/src/escalation.rs:23`) | `AttemptEvidence` v2 et nouveau type d'événement `attempt.evidence.v2`, ajouté à la liste des détails 16 KiB (`store.rs:1236-1243`). Les lecteurs v1 restent intacts |
| `observed_tokens` / `observed_cost_microusd` | Ne pas y écrire d'usage hôte : l'escalade les consomme comme comptabilité faisant autorité à la place du plafond parent (`escalation.rs:499-507`), et l'usage hôte n'entre jamais dans l'admission (`router-types/src/execution.rs:224-225`) | Aucune ; nouveau champ à provenance typée dans `AttemptEvidence` v2 |
| Journal | Frontières temporelles : `RuntimeEvent` n'a pas de temps et son digest `runtime-event-v1` couvre ses champs. Ajouter un type d'événement dédié plutôt que modifier `RuntimeEvent` garde la chaîne lisible | Nouvel événement versionné (par exemple `attempt.timing.v1`) et ADR #117 ; `user_version` du store inchangé si seul le détail porte la donnée |
| Envoi physique | Identité de requête interne, ordinal physique, identité de règlement (#133) | Nouveau type dans `router-types`, propriété F03b |
| Feedback | Clé `(run_key, ordinal)` rompue par le retry ; modèle déclaré absent | Événement feedback v1 avec champ optionnel omis si absent (convention existante), ou migration de la jointure de l'inventaire vers `attempt_key` |
| Entrée F07a | `AcceptedWorkLink` ne lie aucune tentative (`accepted_changes.rs:113-119`) | Soit `accepted-changes` v2, soit un contrat de corrélation séparé (B1 ci-dessous) |
| Révision | Aucun mappage entre `result_revision` (OID Git) et `revision_sha256` F07a | Domaine de digest à définir, ou mappage fourni |

## 4. Premier lot B sans dépendre de A2 à A5 : B1 « attempt-correlation v1 »

### Périmètre

Calcul pur hors ligne, dans le style de F07a : aucune lecture du store, aucun changement à `router-runtime`, `router-types`, au journal ni aux schémas v1/v2. Contrat propre à la fonctionnalité, hors registre public, `evidence_trust: supplied_unverified`.

Entrée stricte (clés inconnues et doublons refusés, 4 MiB au plus) :

- `runtime` : `authority_sha256` fourni, `snapshot_sha256`, `run_key_sha256`, `definition_sha256`, et la liste des tentatives (`ordinal`, `attempt_key_sha256`, `evidence`, `requested`) recopiée depuis `observe --schema-version 2 --json` ;
- `physical` : observations avec clé `(authority_sha256, physical_sha256)`, `sequence`, `evidence_sha256`, `external_request_id` optionnel, `declared_model` optionnel, `claimed_attempt_key_sha256` optionnel.

Rapport : par tentative, les clés physiques liées triées, `physical_sends` (`unknown` ou liste), `selection_agreement` ; la liste `unattributed` ; `input_sha256` et `report_sha256` à domaines séparés, sortie octet-identique pour une entrée identique.

### Fichiers touchés

- `crates/router-feedback/src/attempt_correlation.rs` (nouveau) et une ligne `pub use` dans `crates/router-feedback/src/lib.rs`.
- `crates/router-feedback/tests/attempt_correlation.rs` (nouveau).
- `tests/fixtures/evaluation/attempt-correlation-v1.json` (nouveau, synthétique).
- Plus tard, B1b : sous-commande `eval attempt-correlation` dans `crates/agent-router-cli/src/command.rs` et `evaluation.rs`, sur le modèle d'`accepted-changes` (`evaluation.rs:66-77`).

### Tests négatifs

1. Même clé physique, tentative revendiquée différente : refus conflit.
2. Revendication vers une clé de tentative absente : refus substitution.
3. Deux tentatives de même clé ; ordinal 0, 6, dupliqué ou trou : refus.
4. Clé physique sous l'autorité runtime : refus.
5. `evidence: unknown` avec `requested` non nul : refus.
6. Champ inconnu, clé dupliquée, digest mal formé, entrée au-delà de la borne : refus typé, sans rapport partiel.
7. Deux clés physiques distinctes partageant `external_request_id` : deux appels, jamais fusionnés.
8. Tentative sans revendication : `physical_sends` vaut `unknown`, jamais `0` ni `failed_before_send`.

Chaque test négatif doit échouer quand la vérification correspondante est retirée ; c'est la preuve qu'il peut échouer.

### Contrôle positif

Fixture : deux tentatives (une `durable_attempt`, une `unknown`), quatre observations physiques dont une répétée à l'identique, une nouvelle clé de retry sous le même `external_request_id`, une sans revendication. Attendu : tentative 1 liée à deux clés, tentative 2 à `unknown`, une `unattributed`, la répétition comptée une fois, `selection_agreement` à `match` pour une clé et `mismatch` pour l'autre si son modèle déclaré diffère.

### Critère d'acceptation observable

- `cargo test -p router-feedback --test attempt_correlation --locked` passe, et deux appels sur la fixture produisent des octets identiques.
- `git diff --stat` ne montre aucun changement sous `crates/router-runtime/`, `crates/router-types/` ni dans `crates/agent-router-cli/src/schemas/`.
- Les tests existants `crates/router-runtime/tests/observation.rs`, `observation_v2_adversarial.rs`, `crates/agent-router-cli/tests/schema.rs` et `crates/router-feedback/tests/accepted_changes.rs` passent sans modification.
- La porte qualité du dépôt (`CLAUDE.md`) passe : fmt, clippy `-D warnings`, tests workspace, `xtask check`.

B1 établit la jointure et ses refus sur des preuves fournies. Il n'établit ni envoi physique réel, ni usage, ni gain.

## 5. Non vérifié

- Contenu exact des lots A2 à A5 : non défini dans les sources lues. B1 est construit pour ne dépendre d'aucun producteur natif nouveau, quel que soit ce découpage.
- Client de diagnostic et broker Python sous `docs/bench/` : non lus. Leur liaison thread/tour et leur réservation par envoi ne sont pas vérifiées ici.
- `integrations/bridge-contract/tests/loopback.rs` : non lu, fichier modifié en parallèle par un autre agent. `ProviderRouteEnvelope` (`integrations/bridge-contract/src/provider.rs:195-215`) lie run, reçu, modèle et effort demandés pour la voie bridge, sans persistance trouvée.
- Caractère monotone et bornes réelles d'`elapsed_ms` : déduit de la lecture de `process.rs:172-236`, non exécuté.
- Absence d'écriture journal dans `finite_client.rs` : établie par un `grep` sans résultat sur `self.store`, `append(` et `.event(`, pas par une exécution.
- Tables et fichiers sous `crates/router-runtime/src/admission/budget*.rs` : survolés seulement ; les réservations de budget par ticket n'ont pas été suivies jusqu'à leur lien avec un envoi.
- Aucun test n'a été lancé pour ce document.

## Décisions à arbitrer par le coordinateur

1. Emplacement de B1 : calcul pur à preuves fournies dans `router-feedback` (recommandé, zéro changement runtime) ou lecteur côté runtime qui lit l'instantané v2 dans la même transaction (preuves runtime non fournies, mais couplage et nouvelle surface CLI).
2. Revérification de `snapshot_sha256` par B1 : impossible sans la fonction de digest privée de `router-runtime`, dont `router-feedback` ne dépend pas. Accepter un instantané fourni, ou extraire la convention de digest dans une crate partagée.
3. Autorité runtime : fournie et non vérifiée en B1, ou identité de store exposée dans un prochain schéma explicite d’observation.
4. `observed_tokens` / `observed_cost_microusd` : les réserver à une comptabilité attestée (recommandé) et porter l'usage hôte dans un champ à provenance typée.
5. Jointure feedback ↔ runtime : garder `(run_key, ordinal)` malgré la rupture au retry, ou migrer vers `attempt_key` avec une nouvelle version de rapport.
6. Mappage révision : domaine de digest défini depuis l'OID Git publié, ou `revision_sha256` toujours fourni.

## Mise à jour F05b-a du 2026-10-11

La ligne 17 ci-dessus corrige la frontière historique : les deux lecteurs de flux
étaient déjà inclus dans `elapsed_ms`. [ADR 0030](../../adr/0030-qualified-local-capture-timing.md)
qualifie désormais cet intervalle local via `attempt.timing.v1`, lié à la preuve
de tentative dans la même transaction. Seule l'observation v3 explicite le publie ;
les données historiques sans marqueur restent inconnues. #117 reste ouvert pour
les autres frontières, phases et segments interrompus.
