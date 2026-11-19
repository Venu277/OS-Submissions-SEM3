"""Recovery strategies once a deadlock has been detected."""


def choose_victim(deadlocked, allocation):
    """Abort the deadlocked process holding the fewest resource units.

    Holding fewer units is treated as the cheapest process to restart.
    Ties are broken by lowest process index.
    """
    return min(deadlocked, key=lambda i: (sum(allocation[i]), i))
