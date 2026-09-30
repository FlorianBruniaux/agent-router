# Pilot report

Mode: host-direct. Manifest `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856`. Pricing `pilot-2026-09-30-b`.

| Provider | Arm | Valid/runs | Accepted | Cost list USD | Cost/accepted list | Cost/accepted product | p50 time s | Cost unknown |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| claude | A | 10/10 | 10 | 10.271026 | 1.027103 | 0.580778 | 83.931 | 0 |
| claude | B | 10/10 | 10 | 15.261344 | 1.526134 | 0.857435 | 90.279 | 0 |
| claude | G | 10/10 | 10 | 15.036753 | 1.503675 | 0.83612 | 94.187 | 0 |
| claude | C | 10/10 | 10 | 7.696024 | 0.769602 | 0.441387 | 67.396 | 0 |
| claude | E | 10/10 | 10 | 5.406817 | 0.540682 | 0.310322 | 36.019 | 0 |
| codex | A | 10/10 | 7 | 3.049168 | 0.435595 | 0.435595 | 146.537 | 0 |
| codex | B | 10/10 | 10 | 2.807491 | 0.280749 | 0.280749 | 256.778 | 0 |
| codex | G | 10/10 | 9 | 2.617799 | 0.290867 | 0.290867 | 201.361 | 0 |
| codex | C | 10/10 | 6 | 3.070362 | 0.511727 | 0.511727 | 148.508 | 0 |
| codex | E | 10/10 | 10 | 1.115098 | 0.11151 | 0.11151 | 128.53 | 0 |

## Fixed host overhead (active host config: user-config)

One `Reply with OK` probe per provider, model and host config, read-only, in a clone of the base repo. The per-arm share multiplies the probe's list cost by the arm's phase count; it is an estimate, since provider prompt caching changes the prefix cost from one call to the next.

```json
{
  "host_config": "user-config",
  "probes": {
    "claude:claude-opus-5-5:isolated": {
      "usage": {
        "cache_write_1h_tokens": 8053,
        "cache_write_5m_tokens": 0,
        "cache_write_input_tokens": 8053,
        "cached_input_tokens": 12407,
        "input_tokens": 20462,
        "output_tokens": 4,
        "reasoning_output_tokens": null,
        "thinking_tokens_reported": 0
      },
      "exit_code": 0,
      "host_cost_usd": 0.0669934,
      "list_microusd": 66994,
      "product_microusd": 34782
    },
    "claude:claude-opus-5-5:user-config": {
      "usage": {
        "cache_write_1h_tokens": 66326,
        "cache_write_5m_tokens": 0,
        "cache_write_input_tokens": 66326,
        "cached_input_tokens": 0,
        "input_tokens": 66328,
        "output_tokens": 247,
        "reasoning_output_tokens": null,
        "thinking_tokens_reported": 93
      },
      "exit_code": 0,
      "host_cost_usd": 0.5355559999999999,
      "list_microusd": 535556,
      "product_microusd": 270252
    },
    "claude:claude-sonnet-5-5:isolated": {
      "usage": {
        "cache_write_1h_tokens": 8074,
        "cache_write_5m_tokens": 0,
        "cache_write_input_tokens": 8074,
        "cached_input_tokens": 12407,
        "input_tokens": 20483,
        "output_tokens": 4,
        "reasoning_output_tokens": null,
        "thinking_tokens_reported": 0
      },
      "exit_code": 0,
      "host_cost_usd": 0.034821399999999995,
      "list_microusd": 34822,
      "product_microusd": 18674
    },
    "claude:claude-sonnet-5-5:user-config": {
      "usage": {
        "cache_write_1h_tokens": 65165,
        "cache_write_5m_tokens": 0,
        "cache_write_input_tokens": 65165,
        "cached_input_tokens": 0,
        "input_tokens": 65167,
        "output_tokens": 15,
        "reasoning_output_tokens": null,
        "thinking_tokens_reported": 0
      },
      "exit_code": 0,
      "host_cost_usd": 0.260814,
      "list_microusd": 260814,
      "product_microusd": 130484
    },
    "codex:gpt-5.6-terra:isolated": {
      "usage": {
        "cache_write_input_tokens": 0,
        "cached_input_tokens": 25088,
        "input_tokens": 41948,
        "output_tokens": 227,
        "reasoning_output_tokens": 23
      },
      "exit_code": 0,
      "host_cost_usd": null,
      "list_microusd": 41462,
      "product_microusd": 41462
    },
    "codex:gpt-5.6-terra:user-config": {
      "usage": {
        "cache_write_input_tokens": 0,
        "cached_input_tokens": 37376,
        "input_tokens": 64579,
        "output_tokens": 192,
        "reasoning_output_tokens": 46
      },
      "exit_code": 0,
      "host_cost_usd": null,
      "list_microusd": 64186,
      "product_microusd": 64186
    },
    "codex:gpt-6.1-sol:isolated": {
      "usage": {
        "cache_write_input_tokens": 0,
        "cached_input_tokens": 0,
        "input_tokens": 21235,
        "output_tokens": 5,
        "reasoning_output_tokens": 0
      },
      "exit_code": 0,
      "host_cost_usd": null,
      "list_microusd": 42520,
      "product_microusd": 42520
    },
    "codex:gpt-6.1-sol:user-config": {
      "usage": {
        "cache_write_input_tokens": 0,
        "cached_input_tokens": 7168,
        "input_tokens": 32215,
        "output_tokens": 5,
        "reasoning_output_tokens": 0
      },
      "exit_code": 0,
      "host_cost_usd": null,
      "list_microusd": 50861,
      "product_microusd": 50861
    }
  },
  "per_arm_estimate": {
    "claude:A": {
      "estimated_overhead_usd": 10.57184,
      "arm_list_cost_usd": 10.271026,
      "estimated_share": 1.0293
    },
    "claude:B": {
      "estimated_overhead_usd": 16.06668,
      "arm_list_cost_usd": 15.261344,
      "estimated_share": 1.0528
    },
    "claude:G": {
      "estimated_overhead_usd": 16.06668,
      "arm_list_cost_usd": 15.036753,
      "estimated_share": 1.0685
    },
    "claude:C": {
      "estimated_overhead_usd": 7.82442,
      "arm_list_cost_usd": 7.696024,
      "estimated_share": 1.0167
    },
    "claude:E": {
      "estimated_overhead_usd": 5.35556,
      "arm_list_cost_usd": 5.406817,
      "estimated_share": 0.9905
    },
    "codex:A": {
      "estimated_overhead_usd": 1.79233,
      "arm_list_cost_usd": 3.049168,
      "estimated_share": 0.5878
    },
    "codex:B": {
      "estimated_overhead_usd": 1.52583,
      "arm_list_cost_usd": 2.807491,
      "estimated_share": 0.5435
    },
    "codex:G": {
      "estimated_overhead_usd": 1.52583,
      "arm_list_cost_usd": 2.617799,
      "estimated_share": 0.5829
    },
    "codex:C": {
      "estimated_overhead_usd": 1.92558,
      "arm_list_cost_usd": 3.070362,
      "estimated_share": 0.6272
    },
    "codex:E": {
      "estimated_overhead_usd": 0.50861,
      "arm_list_cost_usd": 1.115098,
      "estimated_share": 0.4561
    }
  }
}
```

## Sizing inputs for N

```json
{
  "claude:A-vs-B": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.0225,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 1,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 1
  },
  "claude:A-vs-C": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.0188,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 1,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 1
  },
  "claude:B-vs-E": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.0447,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 1,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 1
  },
  "claude:B-vs-G": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.0188,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 1,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 1
  },
  "claude:G-vs-C": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.013,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 1,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 1
  },
  "codex:A-vs-B": {
    "pairs": 10,
    "discordance": 0.3,
    "log_cost_ratio_sd_per_task": 0.1473,
    "n_success_10pt_alpha_5": 186,
    "n_cost_ratio_0_8_alpha_5": 3,
    "n_success_10pt_alpha_1_25": 286,
    "n_cost_ratio_0_8_alpha_1_25": 5
  },
  "codex:A-vs-C": {
    "pairs": 10,
    "discordance": 0.5,
    "log_cost_ratio_sd_per_task": 0.2001,
    "n_success_10pt_alpha_5": 310,
    "n_cost_ratio_0_8_alpha_5": 5,
    "n_success_10pt_alpha_1_25": 476,
    "n_cost_ratio_0_8_alpha_1_25": 8
  },
  "codex:B-vs-E": {
    "pairs": 10,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": 0.1704,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": 4,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": 6
  },
  "codex:B-vs-G": {
    "pairs": 10,
    "discordance": 0.1,
    "log_cost_ratio_sd_per_task": 0.0963,
    "n_success_10pt_alpha_5": 62,
    "n_cost_ratio_0_8_alpha_5": 2,
    "n_success_10pt_alpha_1_25": 96,
    "n_cost_ratio_0_8_alpha_1_25": 2
  },
  "codex:G-vs-C": {
    "pairs": 10,
    "discordance": 0.5,
    "log_cost_ratio_sd_per_task": 0.2054,
    "n_success_10pt_alpha_5": 310,
    "n_cost_ratio_0_8_alpha_5": 6,
    "n_success_10pt_alpha_1_25": 476,
    "n_cost_ratio_0_8_alpha_1_25": 9
  }
}
```

10 tasks give a direction and variance estimates to fix N; no adoption claim follows from this pilot.

## Repetition filter

Only repetition(s) <= 1 of the sealed order were run: variance between repetitions is not measured, and the sizing inputs mix task variance with run-to-run noise. Filter journal: `run-filter.jsonl` (1 record(s)). The manifest was not resealed.
