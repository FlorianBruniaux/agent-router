def semver_compare(a, b):
    """Compare two semantic versions; return -1, 0 or 1."""
    left = [int(part) for part in a.split(".")]
    right = [int(part) for part in b.split(".")]
    return (left > right) - (left < right)
