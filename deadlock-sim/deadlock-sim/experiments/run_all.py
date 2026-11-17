"""Run both strategies on identical seeded workloads and write results/metrics.csv."""

import csv
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "src"))

from simulator import simulate          # noqa: E402
from workload import generate_workload  # noqa: E402

CONFIGS = [
    {"label": "low",    "n": 5, "total": [10, 10, 10], "max_claim": 6},
    {"label": "medium", "n": 6, "total": [8, 8, 8],    "max_claim": 6},
    {"label": "high",   "n": 8, "total": [6, 6, 6],    "max_claim": 6},
]
SEEDS = range(30)
DETECT_INTERVAL = 5


def main():
    rows = []
    for cfg in CONFIGS:
        for seed in SEEDS:
            wl = generate_workload(cfg["n"], cfg["total"], cfg["max_claim"], seed)
            for strategy in ("avoidance", "detection"):
                r = simulate(wl, strategy, detect_interval=DETECT_INTERVAL)
                r["config"] = cfg["label"]
                r["seed"] = seed
                rows.append(r)

    out_dir = os.path.join(ROOT, "results")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "metrics.csv")
    fields = ["config", "seed", "strategy", "processes", "completed", "unfinished",
              "ticks", "throughput", "deadlocks", "aborts", "mean_wait",
              "safety_checks", "detection_passes"]
    with open(out_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)

    print(f"Wrote {len(rows)} runs to {out_path}\n")
    print(f"{'config':<8}{'strategy':<12}{'throughput':>12}{'mean_wait':>11}"
          f"{'deadlocks':>11}{'aborts':>8}{'safety_chk':>12}")
    for cfg in CONFIGS:
        for strategy in ("avoidance", "detection"):
            sel = [r for r in rows if r["config"] == cfg["label"] and r["strategy"] == strategy]
            avg = lambda k: sum(r[k] for r in sel) / len(sel)  # noqa: E731
            print(f"{cfg['label']:<8}{strategy:<12}{avg('throughput'):>12.4f}"
                  f"{avg('mean_wait'):>11.2f}{avg('deadlocks'):>11.2f}"
                  f"{avg('aborts'):>8.2f}{avg('safety_checks'):>12.1f}")


if __name__ == "__main__":
    main()
