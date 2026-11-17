"""Seeded workload generator. The same seed always produces the same workload."""

import random


def generate_workload(num_processes, total_resources, max_claim, seed):
    rng = random.Random(seed)
    m = len(total_resources)

    maximum = []
    for _ in range(num_processes):
        row = [rng.randint(0, min(total_resources[j], max_claim)) for j in range(m)]
        if sum(row) == 0:
            row[rng.randrange(m)] = 1
        maximum.append(row)

    hold = [rng.randint(1, 5) for _ in range(num_processes)]
    # Per-process seeds drive the order in which each process requests units.
    seeds = [rng.randrange(2**31) for _ in range(num_processes)]

    return {
        "total": list(total_resources),
        "maximum": maximum,
        "hold": hold,
        "seeds": seeds,
    }
