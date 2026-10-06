# Critique du plan en cinq étapes, 6 octobre 2026

## Préparation à l'exécution

Verdict : à corriger avant une implémentation large. Le diagnostic local et
les tests de comptabilité peuvent commencer. L'installation quotidienne et les
appels réels restent conditionnés aux preuves et accords décrits ci-dessous.
Pas de score chiffré : cette revue ne dispose pas d'un barème calibré.

L'agent principal critique son propre résumé conversationnel : parcours Codex,
mesures, automatisation, comparaison quotidienne, publication. Ce n'est pas une
revue indépendante. Le [plan détaillé](status-and-action-plan-2026-10-05.md)
contient déjà plusieurs garde-fous omis dans ce résumé ; ils restent applicables.

Référence lue : HEAD `02e7d712a1ec3d27399468a12e63d6654b32d5e3`, worktree propre
avant cette révision. SHA-256 du plan avant modification :
`b1eaa027271b814387f3557d5f23eb6253bd758ca01976134ec4ade443fa593f`.
Cette critique n'exécute aucun test produit ni appel fournisseur.

## Ce qui tient

- Commencer par une seule surface, puis distinguer collecte et comparaison.
- Conserver les échecs, les données inconnues et la possibilité d'un gain nul.
- Réutiliser les validations locales sans dépendre d'un budget GitHub Actions.

## Blocages du résumé

### F7. Le premier livrable confond diagnostic natif et parcours Agent Router

« Codex exécute avec ce choix » ne nomme pas le raccord entre décision et
invocation. Le [diagnostic existant](native-integration-u02/canary-diagnosis/README.md)
n'a pas exécuté le callback code-mode. Son fournisseur est simulé et ses
paramètres sont pilotés par le probe. Même corrigé, ce diagnostic ne prouverait
pas qu'une décision produite par Agent Router arrive à Codex.

Correction : U02a prouve un aller-retour d'outil ; U02b identifie le contrat
de déclenchement/application ; U06 relie une vraie décision CLI à une seule
session native et au bilan. Seules des capacités positivement vérifiées
débloqueront ces transitions. Un verdict Unsupported termine le diagnostic,
pas l'intégration. Aucun test ne doit déduire une identité fournisseur du seul
nom de modèle envoyé.

### F8. La transition vers les appels réels perd ses conditions d'autorisation

Le résumé reporte le plafond à l'étape de comparaison alors que sa première
étape annonce déjà des conditions réelles. Le
[candidat de calibration](calibration-v2/README.md) refuse les appels réels
avant validation du contrôle des envois et approbation du manifeste. Ce n'est
pas un détail reportable au premier essai payant.

Correction : garder U02/U06/U07 locaux avec fournisseur simulé ; placer le
premier appel distant dans U09. Le manifeste précise avant accord s'il existe
un plafond garanti ou seulement une consommation suivie. Les appels physiques,
reprises et sondes sont inclus. Le runner historique reste désactivé tant que
sa propre condition de contrôle des envois n'est pas satisfaite.

## Risques majeurs

### F9. « Automatique » et « utilisable sur ton ordinateur » ne définissent pas la surface

Une intégration Codex CLI macOS ne valide ni l'application Codex ni Claude.
Le contrat produit actuel refuse `ParentConversation` dans
[execution.rs](../../crates/router-adapter-api/src/execution.rs). Cela ne prouve
pas une impossibilité de la plateforme, mais interdit de promettre que la
conversation ouverte changera de modèle grâce au code existant.

Correction : afficher la surface, la session visée et le geste utilisateur
restant. Prouver l'événement natif avant d'écrire un hook. Si le contrat manque,
conserver le conseil explicite et le diagnostic, sans annoncer l'usage
automatique disponible ni élargir les permissions.

### F10. « Comparer sur ton travail quotidien » ne suffit pas à attribuer un gain

Deux journées différentes ne fournissent pas le même ensemble de tâches.
Les reprises peuvent disparaître ; la personne qui voit la première solution
peut aussi favoriser le second essai. Les tokens ne donnent pas à eux seuls
une économie sur un abonnement.

Correction : garder le journal quotidien descriptif, puis utiliser des tâches
appariées, départs identiques, ordre consigné et revue masquée lorsque possible.
Séparer calibration et confirmation. Conserver les règles déjà détaillées dans
le candidat v2 : tentatives, inconnus, qualité, corrections et temps humain.
Qualifier séparément tokens, quota observable, estimation API et facture.

### F11. La publication abrège silencieusement le périmètre du projet

Le résumé ne rappelle pas U11-U16, qui prouvent la supervision complète, ni le
critère de démarrage à froid encore non satisfait dans
[completion.md](../completion.md). La
[distribution](../distribution.md) conserve aussi une décision de licence.
Installation, contrôles Linux et documentation ne suffisent donc pas seuls à
déclarer le projet terminé.

Correction : distinguer l'essai personnel, une éventuelle publication
expérimentale au périmètre accepté, et U17, livraison du périmètre complet.
Conserver les exigences restantes dans U18 avec leur déclencheur. Le présent
plan ne change ni licence ni visibilité du dépôt.

## Éléments à rendre concrets avant leur étape

- [ ] Événement natif, version et empreinte du binaire, limites de requêtes,
  d'octets, de temps et de processus pour le scénario retenu.
- [ ] Fichiers possédés et tests RED avant chaque correctif de comportement.
- [ ] Corpus, comparateur, budget, provenance et arrêts scellés avant U09 ;
  marges de qualité/coût/temps et analyse scellées avant U10.
- [ ] Retrait testé en nouvelle session ; aucune relance pour réparer une
  mesure perdue ; aucune reprise cachée après une réponse ambiguë.

## Périmètre et trois actions de départ

La première tranche livre une tâche Codex CLI traçable et un bilan fidèle.
Claude et la supervision complète restent dans le projet, avec leurs propres
admissions. Pixel, les dashboards, Windows et une fusion avec cc-bridge ne
bloquent pas cette première tranche ; leurs déclencheurs restent dans U18.

1. U02a : reproduire puis fermer l'aller-retour code-mode manquant.
2. U04b : reproduire une tâche avec reprise et tentative sans feedback, puis
   vérifier que son bilan n'en perd aucune.
3. U08 : préparer une seule comparaison routeur/modèle fixe et ses règles
   d'arrêt, sans lancer de fournisseur.

## Plan consolidé et graphe de livraison

La [révision exécutable du 6 octobre](status-and-action-plan-2026-10-05.md#plan-consolide-du-6-octobre)
intègre ces cinq constats. Elle porte les unités, dépendances, validations,
frontière de départ, modes de restauration et critères de succès. Elle
précise la tranche courante sans remplacer les exigences U01-U18 historiques.
