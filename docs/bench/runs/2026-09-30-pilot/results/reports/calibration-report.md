# Pilot report

Mode: host-direct. Manifest `06d93c6668898f9900325dc3f97aaf891b3feee14e2029a741c993b2f8844856`. Pricing `pilot-2026-09-30-b`.

| Provider | Arm | Valid/runs | Accepted | Cost list USD | Cost/accepted list | Cost/accepted product | p50 time s | Cost unknown |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| claude | A | 1/1 | 1 | 1.03282 | 1.03282 | 0.588916 | 67.581 | 0 |
| claude | B | 1/1 | 1 | 1.487081 | 1.487081 | 0.836701 | 79.51 | 0 |
| claude | G | 1/1 | 1 | 1.475819 | 1.475819 | 0.824795 | 77.292 | 0 |
| claude | C | 1/1 | 1 | 0.769532 | 0.769532 | 0.444762 | 50.237 | 0 |
| claude | E | 1/1 | 1 | 0.528709 | 0.528709 | 0.302069 | 32.02 | 0 |
| codex | A | 1/1 | 1 | 0.302501 | 0.302501 | 0.302501 | 138.991 | 0 |
| codex | B | 1/1 | 1 | 0.271577 | 0.271577 | 0.271577 | 236.037 | 0 |
| codex | G | 1/1 | 1 | 0.243209 | 0.243209 | 0.243209 | 143.414 | 0 |
| codex | C | 1/1 | 0 | 0.297441 | None | None | None | 0 |
| codex | E | 1/1 | 1 | 0.108726 | 0.108726 | 0.108726 | 113.37 | 0 |

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
      "estimated_overhead_usd": 1.057184,
      "arm_list_cost_usd": 1.03282,
      "estimated_share": 1.0236
    },
    "claude:B": {
      "estimated_overhead_usd": 1.606668,
      "arm_list_cost_usd": 1.487081,
      "estimated_share": 1.0804
    },
    "claude:G": {
      "estimated_overhead_usd": 1.606668,
      "arm_list_cost_usd": 1.475819,
      "estimated_share": 1.0887
    },
    "claude:C": {
      "estimated_overhead_usd": 0.782442,
      "arm_list_cost_usd": 0.769532,
      "estimated_share": 1.0168
    },
    "claude:E": {
      "estimated_overhead_usd": 0.535556,
      "arm_list_cost_usd": 0.528709,
      "estimated_share": 1.013
    },
    "codex:A": {
      "estimated_overhead_usd": 0.179233,
      "arm_list_cost_usd": 0.302501,
      "estimated_share": 0.5925
    },
    "codex:B": {
      "estimated_overhead_usd": 0.152583,
      "arm_list_cost_usd": 0.271577,
      "estimated_share": 0.5618
    },
    "codex:G": {
      "estimated_overhead_usd": 0.152583,
      "arm_list_cost_usd": 0.243209,
      "estimated_share": 0.6274
    },
    "codex:C": {
      "estimated_overhead_usd": 0.192558,
      "arm_list_cost_usd": 0.297441,
      "estimated_share": 0.6474
    },
    "codex:E": {
      "estimated_overhead_usd": 0.050861,
      "arm_list_cost_usd": 0.108726,
      "estimated_share": 0.4678
    }
  }
}
```

## Sizing inputs for N

```json
{
  "claude:A-vs-B": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "claude:A-vs-C": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "claude:B-vs-E": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "claude:B-vs-G": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "claude:G-vs-C": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "codex:A-vs-B": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "codex:A-vs-C": {
    "pairs": 1,
    "discordance": 1.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": 619,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": 951,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "codex:B-vs-E": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "codex:B-vs-G": {
    "pairs": 1,
    "discordance": 0.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": null,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": null,
    "n_cost_ratio_0_8_alpha_1_25": null
  },
  "codex:G-vs-C": {
    "pairs": 1,
    "discordance": 1.0,
    "log_cost_ratio_sd_per_task": null,
    "n_success_10pt_alpha_5": 619,
    "n_cost_ratio_0_8_alpha_5": null,
    "n_success_10pt_alpha_1_25": 951,
    "n_cost_ratio_0_8_alpha_1_25": null
  }
}
```

10 tasks give a direction and variance estimates to fix N; no adoption claim follows from this pilot.
