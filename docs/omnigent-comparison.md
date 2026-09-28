# Omnigent et Agent Router

Comparaison documentaire du 2026-09-28. Sources consultées :
[index officiel](https://omnigent.ai/llms.txt), puis les pages ci-dessous.
Aucune installation ni exécution d'Omnigent n'a été réalisée. Ses comportements
sont ceux décrits par sa documentation, pas des garanties auditées ici.

## Positionnement

Omnigent recoupe le périmètre d'Agent Router. Il documente un environnement
multi-harness avec sessions partagées, interfaces web/mobile/desktop et règles
contextuelles. Son orchestrateur [Polly](https://omnigent.ai/docs/use/builtin-agents/polly)
délègue dans des worktrees et fait relire les changements par un autre fournisseur.
Il ouvre des PR et laisse leur fusion à l'utilisateur.

Il possède aussi un [Smart Routing](https://omnigent.ai/docs/build/routing) :
un juge LLM, ou un routeur externe, choisit le couple modèle/harness à partir du
premier message pour la session. Notre différence se situe dans la granularité
par phase et dans une décision locale reproductible, sous contraintes, qui lie
modèle, effort et assets aux preuves et à l'approbation. La reprise du checkpoint
ajoute des plans avancés et une bibliothèque de catalogues signés ; leurs limites
de livraison sont dans le [registre d'acceptation](remaining-acceptance.md).
L'apprentissage et les gains de qualité restent soumis à des preuves humaines.

Leur [API de routage externe](https://omnigent.ai/docs/reference/routing-api)
rend une intégration possible : Agent Router pourrait fournir la décision,
Omnigent la session et l'interface. Le contrat transmet des couples autorisés,
un prompt tronqué et un sélecteur de stratégie ; il ne suffit pas à établir
l'autorité locale sur les permissions ou le budget. Cette compatibilité est
une proposition, pas une intégration livrée.

## Idées retenues et critères de livraison

| ID | Lots existants | Complément accepté | Preuve attendue |
| --- | --- | --- | --- |
| O1 | R11, R19 | Adaptateur opt-in `omnigent.api.routing.v1`, puis endpoint `POST /routes:select` du daemon. Le moteur conserve le contexte et les limites fournis par l'opérateur ; le message distant reste une donnée. | Fixtures bornées, stratégie inconnue refusée, mapping exact modèle/harness, sélection limitée à l'intersection des candidats offerts et localement éligibles. Aucun effet d'exécution. Test HTTP local ensuite. |
| O2 | R04, R09, R13 | Preset de revue par un fournisseur distinct, avec séparation implémenteur/reviewer et worktrees isolés. | Identité du fournisseur issue des métadonnées validées ; même fournisseur, identité inconnue et reviewer indisponible explicitement traités. Deux modèles différents ne prouvent pas deux fournisseurs. |
| O3 | R10, R12 | Budgets et limites cumulatifs persistés par session, seuils d'alerte et approbation de dépassement liée au nouvel état. | Redémarrage, concurrence et coût inconnu testés. Fusion restrictive monotone : une règle de session ne peut pas rétablir une permission interdite. |
| O4 | R10, R08 | Étudier un broker de credentials à jetons substitués, lié à une destination HTTP explicitement autorisée. | Canary local : secret absent de l'environnement enfant, des logs et des artifacts ; méthodes/hôtes/chemins bornés ; redirections et destinations privées refusées. Rien d'activé avant preuve de confinement. |
| O5 | R19, R23 | Reprise et fork locaux des sessions ; vue des différences et arrêt/reprise humains depuis l'interface. | Parent immuable, historique et artifacts bornés, nouvelle identité de fork, nouvelle vérification d'autorité, aucune approbation exécutable héritée. Collaboration distante évaluée séparément. |
| O6 | R06, R09, R23 | Registre explicite d'adaptateurs externes, avec version/provenance/signature et matrice des capacités. | Conflits de noms refusés, découverte sans import exécutable, installation opt-in et réversible ; validation d'un manifeste distincte de l'activation native. |

O1 dispose d'une [traduction hors ligne](../integrations/omnigent.md) avec tests
de contrat synthétiques ; le serveur et la preuve native restent à livrer.
O2 et O3 prolongent les contrats de planification/runtime existants. O4 dépend
du transport supervisé ; O5 et O6 suivent le daemon et les catalogues signés.
Le périmètre R01–R23 et les gates P1–P6 sont conservés avec leur état réel.

## Limites à conserver

- L'API Omnigent documente un repli vers le harness par défaut lorsque le routeur
  échoue. Ce repli n'est pas une preuve qu'un refus Agent Router est appliqué.
  O1 ne peut donc être présenté comme un contrôle d'exécution obligatoire
  avant vérification de ce chemin côté appelant.
- Les [politiques contextuelles](https://omnigent.ai/docs/policies/overview)
  inspirent l'état cumulé ; leur ordre d'évaluation ne remplace pas notre
  fusion restrictive ni une preuve d'interception native.
- [Omnibox](https://omnigent.ai/docs/omnibox) documente un proxy de sortie et la
  substitution de credentials. Réutiliser ce mécanisme exige un test, une
  revue de licence et une preuve de confinement dans chaque OS supporté.
- Les [forks et sessions partagées](https://omnigent.ai/docs/collaborate) exposent
  des droits distincts ; la collaboration distante et le SSO ne sont pas des
  dépendances du moteur local.
- Les [plugins communautaires](https://omnigent.ai/docs/build/harnesses/community)
  donnent un modèle d'extension, sans autoriser l'import de code non approuvé.
- Omnigent se présente encore comme alpha dans sa [FAQ](https://omnigent.ai/faq).
  La comparaison ne constitue ni un benchmark ni une certification de sécurité.
