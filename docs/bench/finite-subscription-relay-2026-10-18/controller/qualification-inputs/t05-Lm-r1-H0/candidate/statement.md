# t05 : Revoir autorité et confiance sans fausses alertes

Relis review.rs sans le modifier. Contrat de cet exercice : seuls Supported
et les skills fiables autorisent respectivement inférence et augmentation de score.
La saturation arithmétique dans utility est intentionnelle et autorisée ici.
Les deux autres blocs ont pour contrat : refuser capacité absente ou false ;
classer utilité décroissante, coût et identifiant croissants.
Livre review.json avec une seule clé findings, tableau de {block, severity,
explanation}. severity vaut high pour violation d'autorité ou confiance.
Chaque explication doit donner un déclencheur et sa conséquence. Signale
uniquement des défauts démontrables, une fois par bloc, jamais des préférences.
Une liste vide est autorisée si aucun défaut.

Fichier autorisé : review.json (à créer). La revue humaine juge les explications.
