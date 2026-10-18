# t08 : Tester les coûts des tentatives ratées

lib.rs est une référence immuable. Écris tests.rs pour metrics : somme
de tous les coûts / nombre d'acceptés, ratio entier (numérateur,dénominateur),
pas de division arrondie. Une coût/résultat inconnu ou zéro accepté donne Ok(None).
Longueurs différentes : Err("length"), plus de 1000 lignes : Err("limit").
Inclure cas d'échec, unknown, zéro connu et somme supérieure à u64.
Tes tests passent sur référence et échouent par assertion sur : coûts des
échecs exclus, coût absent remplacé par zéro, zéro accepté remplacé par un.
Pas de réseau, dépendance ou test du texte source.

Fichier autorisé : tests.rs.
