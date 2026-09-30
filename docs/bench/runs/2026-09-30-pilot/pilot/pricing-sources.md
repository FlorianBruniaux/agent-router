# Sources des prix du pilote

Relevés le 2026-09-30. `pricing.json` garde une source courte : le produit refuse une chaîne `source` trop longue (un `source` de 728 caractères a donné `feedback.invalid`, un de 500 est accepté).

## Anthropic

https://platform.claude.com/docs/en/about-claude/pricing, table « Model pricing », lue en entier.

- Claude Opus 5.5 : base input $4 / MTok, 5m cache writes $5, 1h cache writes $8, cache hits $0.20, output $20.
- Claude Sonnet 5.5 : $2, $2.50, $4, $0.20, $10.
- Claude Haiku 4.5 : $1, $1.25, $2, $0.10, $5.

## OpenAI

https://developers.openai.com/api/docs/pricing, table « Standard pricing data ». https://openai.com/api/pricing/ répond 403. Lecture faite par un outil qui résume la page ; les lignes ci-dessous sont celles qu'il a rendues comme citation verbatim. Un humain doit les relire avant le lancement.

En-tête : `| Model | Short context input | Short context cached input | Short context cache writes | Short context output | Long context input | Long context cached input | Long context cache writes | Long context output |`

Seuil : `Short context: ≤272K input tokens. Long context: >272K input tokens.`

- `| gpt-6.1-sol | $2.00 | $0.10 | $2.50 | $10.00 | $4.00 | $0.20 | $5.00 | $15.00 |`
- `| gpt-5.6-terra | $2.00 | $0.20 | $2.50 | $12.00 | $4.00 | $0.40 | $5.00 | $18.00 |`

Le pilote applique le tarif court contexte. Les écritures cache OpenAI ne sont pas tarifées : l'usage Codex lu dans `turn.completed` ne déclare aucun compteur d'écriture cache.

## Non relu

- Grille de crédits Codex, https://help.openai.com/en/articles/20001106-codex-rate-card : 403. Aucune valeur en crédits dans le pilote.
