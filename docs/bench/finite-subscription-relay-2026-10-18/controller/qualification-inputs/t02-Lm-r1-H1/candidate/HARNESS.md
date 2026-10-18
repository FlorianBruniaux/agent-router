# Procédure et feedback locaux

1. Relis le contrat et identifie pour chaque exigence ce qui permettra de la
   vérifier. Sépare données connues, cas limites et attentes encore ambiguës.
2. Effectue le changement dans les seuls fichiers de sortie autorisés.
3. Lance `python3 -B feedback.py`. Lis le diagnostic, corrige le résultat
   plutôt que les attentes du contrôle, puis relance avant de terminer.

Le script ne contient aucun test final caché ni solution attendue.
Pour l'arithmétique, ses exemples publics sont issus de l'énoncé.
Pour la revue, il vérifie seulement la forme du JSON et ne juge pas les
défauts signalés. Pour l'écriture de tests, il vérifie leur exécution sur
la référence ; il ne livre ni les mutants réservés ni leurs résultats.
Un contrôle local vert ne suffit donc pas à établir l'acceptation finale.

Pour une revue, vérifie chaque alerte dans le bloc concerné et donne son
déclencheur et sa conséquence. Pour des tests, fais correspondre assertions
et invariants de l'énoncé, avec cas valides et invalides. N'affirme pas une
vérification que tu n'as pas exécutée.
