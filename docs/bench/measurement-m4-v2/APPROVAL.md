# Accord proposé pour M4 v2

Statut : en attente. Aucun essai v2 lancé.

Empreinte SHA-256 de `seal.json` :

```text
c4507b04725739cf6e1e416b24acbdd4f71fb7927b5a4f82ffdd84e22ab5cdb6
```

L’accord porterait sur trois réponses de patch, dans l’ordre Router, Sol fixe, Luna fixe, avec 100 000 tokens par participant et 300 000 au total comme seuils surveillés. Il autoriserait l’exception locale au chargement des skills et aux appels d’outils pour ces trois participants seulement. Les tests sont exécutés localement par l’opérateur et la revue finale appartient à Florian. Aucun coordinateur ni relecteur modèle supplémentaire, aucun retry, aucune escalade.

Le registre bloque l’admission suivante sur une preuve manquante, incohérente ou hors budget. Il ne peut pas interrompre automatiquement une réponse en cours ni empêcher un appel direct hors registre. Un dépassement reste possible et intégralement comptabilisé. Le contexte ajouté par l’hôte demeure hors du contrôle du prompt de tâche.

La préparation et la conversation de suivi restent un coût séparé incomplet ; les 279 563 tokens de la v1 restent au bilan antérieur. Aucun tarif ni montant monétaire ne suit de cette enveloppe. Ce diagnostic assisté ne qualifie pas le runtime natif.

Conserver le message d’accord de Florian avec cette empreinte. La fusion d’une PR, un état `ready` ou la connaissance de cette empreinte ne valent pas consentement. Toute modification du scellement exige une nouvelle présentation.
