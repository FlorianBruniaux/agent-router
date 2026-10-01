#!/usr/bin/env python3
"""Print the pilot tables (Markdown) from the exported metrics in results/.

Usage: python3 summarize.py [--task t02]

Reads results/runs/*.json, results/overhead/*.json and results/manifest.json only.
Every acceptance figure is printed twice, and conclusions must be read on both.

- raw: the verdict of the sealed harness, as recorded.
- corrected ("pycache ignored"): a deviation from the sealed protocol, decided on
  2026-09-30 after observing the results. Harness defect: the base repository has
  no .gitignore, so an agent that runs the tests creates `tests/__pycache__/`,
  which the sealed harness counts as a write outside `pilot_lib/`. Exact rule: a
  run is accepted when its hidden test passed AND every out-of-scope path that
  remains has no path segment equal to `__pycache__` (`**/__pycache__/**` and the
  `__pycache__/` directory entry itself are ignored, nothing else). The hidden
  test result and the touched paths are recorded per run, so the rule is applied
  to recorded facts only. The runs it changes are listed in the output.

Costs are catalog list prices in USD, not invoices. An UNKNOWN cost stays UNKNOWN.
"""
from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path

RESULTS = Path(__file__).resolve().parent / "results"
ARMS = ("A", "B", "G", "C", "E", "D")


def usd(micro) -> str:
    return "UNKNOWN" if micro is None else f"{micro / 1e6:.4f}"


def is_pycache(path: str) -> bool:
    return "__pycache__" in path.strip("/").split("/")


def corrected(run: dict) -> bool:
    outside = [p for p in run["acceptance"]["touched_outside_scope"] if not is_pycache(p)]
    return bool(run["acceptance"]["passed"]) and not outside


def total(values):
    return None if any(v is None for v in values) else sum(values)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task", help="restrict to one task, e.g. t02 for the calibration block")
    args = parser.parse_args()
    manifest = json.loads((RESULTS / "manifest.json").read_text())
    loaded = {p.stem: json.loads(p.read_text()) for p in sorted((RESULTS / "runs").glob("*.json"))}
    loaded = {k: r for k, r in loaded.items() if args.task is None or r["task"] == args.task}
    # A retried or interrupted attempt keeps its own file (it was paid for) but is not the run's result.
    aside = {k: r for k, r in loaded.items() if ".interrupted-" in k or k.endswith(".retried")}
    runs = [r for k, r in loaded.items() if k not in aside]
    reps = sorted({r["rep"] for r in runs})
    print(f"Manifest `{manifest['manifest_sha256']}`, host config `{manifest['host_config']}`, "
          f"{len(runs)} run(s), repetitions present: {reps}. Variance between repetitions: not measured.")
    if aside:
        print(f"Attempts set aside (retried or interrupted, excluded from the tables but paid): {sorted(aside)}, "
              f"list cost {usd(total([r['cost_list_microusd'] for r in aside.values()]))} USD.")
    changed = sorted(r["run_id"] for r in runs if not r["invalid"] and corrected(r) != bool(r["accepted"]))
    print(f"Runs whose verdict the pycache correction changes: {changed if changed else 'none'}.")
    print()

    print("| Provider | Arm | Runs | Invalid | Accepted raw | Rate raw | List USD per accepted raw | "
          "Accepted corrected | Rate corrected | List USD per accepted corrected | List cost USD | Product cost USD | "
          "Input tokens | Cached input | Cache writes | Output | p50 time to acceptance s (corrected) |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for provider in sorted({r["provider"] for r in runs}):
        for arm in ARMS:
            rows = [r for r in runs if r["provider"] == provider and r["arm"] == arm]
            if not rows:
                continue
            valid = [r for r in rows if not r["invalid"]]
            ok = [r for r in valid if corrected(r)]
            raw = [r for r in valid if r["accepted"]]
            cost = total([r["cost_list_microusd"] for r in valid])
            product = total([r["cost_product_microusd"] for r in valid])
            per = None if cost is None or not ok else cost // len(ok)
            per_raw = None if cost is None or not raw else cost // len(raw)
            rate = lambda n: f"{n / len(valid):.0%}" if valid else "n/a"
            tokens = {k: total([r["tokens"][k] for r in valid]) for k in
                      ("input_tokens", "cached_input_tokens", "cache_write_input_tokens", "output_tokens")}
            times = [r["time_to_acceptance_ms"] / 1000 for r in ok]
            p50 = f"{statistics.median(times):.1f}" if times else "n/a"
            fmt = lambda v: "UNKNOWN" if v is None else str(v)
            print(f"| {provider} | {arm} | {len(rows)} | {len(rows) - len(valid)} | {len(raw)} | {rate(len(raw))} | {usd(per_raw)} | "
                  f"{len(ok)} | {rate(len(ok))} | {usd(per)} | "
                  f"{usd(cost)} | {usd(product)} | {fmt(tokens['input_tokens'])} | {fmt(tokens['cached_input_tokens'])} | "
                  f"{fmt(tokens['cache_write_input_tokens'])} | {fmt(tokens['output_tokens'])} | {p50} |")

    print("\nPaired comparisons per task (descriptive, no significance test). Discordance: share of paired tasks where "
          "exactly one of the two arms is accepted. Cost ratio: list cost of X summed over the paired tasks, divided by "
          "that of Y. The sizing section of the final report computes discordance on the raw column only.\n")
    print("| Provider | X vs Y | Paired tasks | Discordance raw | Discordance corrected | Tasks where X costs less | Cost ratio X/Y |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for provider in sorted({r["provider"] for r in runs}):
        index = {(r["arm"], r["task"]): r for r in runs if r["provider"] == provider and not r["invalid"]}
        for x, y in (("A", "B"), ("A", "G"), ("A", "C"), ("A", "E"), ("B", "E"), ("B", "G"), ("G", "C")):
            tasks = sorted(t for (arm, t) in index if arm == x and (y, t) in index)
            if not tasks:
                continue
            pairs = [(index[(x, t)], index[(y, t)]) for t in tasks]
            raw = sum(bool(a["accepted"]) != bool(b["accepted"]) for a, b in pairs) / len(pairs)
            fixed = sum(corrected(a) != corrected(b) for a, b in pairs) / len(pairs)
            costs = [(a["cost_list_microusd"], b["cost_list_microusd"]) for a, b in pairs]
            if any(a is None or b is None for a, b in costs):
                cheaper, ratio = "UNKNOWN", "UNKNOWN"
            else:
                cheaper = f"{sum(a < b for a, b in costs)}/{len(costs)}"
                ratio = f"{sum(a for a, _ in costs) / sum(b for _, b in costs):.3f}"
            print(f"| {provider} | {x} vs {y} | {len(pairs)} | {raw:.2f} | {fixed:.2f} | {cheaper} | {ratio} |")

    probes = {p.stem: json.loads(p.read_text()) for p in sorted((RESULTS / "overhead").glob("*.json"))}
    print("\nFixed host overhead, one `Reply with OK` call per model and host config:\n")
    print("| Provider | Model | Host config | Input tokens | Cached input | Cache writes | Output | List USD |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- |")
    for probe in probes.values():
        u = probe["usage"] or {}
        print(f"| {probe['provider']} | {probe['model']} | {probe['variant']} | {u.get('input_tokens')} | {u.get('cached_input_tokens')} | "
              f"{u.get('cache_write_input_tokens')} | {u.get('output_tokens')} | {usd(probe['cost']['list_microusd'])} |")

    active = manifest["host_config"]
    by_key = {(p["provider"], p["model"], p["variant"]): p["cost"]["list_microusd"] for p in probes.values()}
    print(f"\nCold-probe bound per arm: the probe cost of each phase model under `{active}`, summed over the arm's processes, "
          "divided by the arm list cost. The probe pays a cold cache write; a run that reuses a warm cache pays less for the "
          "same prefix, so the ratio can exceed 100%. It bounds the fixed prefix cost from above; it is not the true share, "
          "which these data do not identify (the number of turns per process is not recorded).\n")
    print("| Provider | Arm | Cold-probe bound USD | Arm list cost USD | Bound / cost |")
    print("| --- | --- | --- | --- | --- |")
    for provider in sorted({r["provider"] for r in runs}):
        for arm in ARMS:
            rows = [r for r in runs if r["provider"] == provider and r["arm"] == arm and not r["invalid"]]
            if not rows:
                continue
            overhead = total([by_key.get((provider, p["model"], active)) for r in rows for p in r["phases"]])
            cost = total([r["cost_list_microusd"] for r in rows])
            share = "UNKNOWN" if overhead is None or not cost else f"{overhead / cost:.1%}"
            print(f"| {provider} | {arm} | {usd(overhead)} | {usd(cost)} | {share} |")


if __name__ == "__main__":
    main()
