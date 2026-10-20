# t02 : Calculer le score sans perdre les bornes

Corrige lib.rs. Formule : quality - floor(sum(costs)/4) - floor(sum(latencies)/10)
+ floor(sum(skills)/20). Somme avant division, avec intermédiaires assez larges.
Plus de 64 éléments dans une composante : Err("work_limit"). Résultat hors i64 :
Err("score_out_of_range"), jamais panic, cast tronqué ou saturation. Les sommes
vides valent zéro. Types et signature publics conservés.

Fichier autorisé : lib.rs.
