def _key(version):
    version = version.split("+", 1)[0]
    core, _, pre = version.partition("-")
    numbers = tuple(int(part) for part in core.split("."))
    if not pre:
        return numbers, (1,)
    ids = tuple((0, int(p), "") if p.isdigit() else (1, 0, p) for p in pre.split("."))
    return numbers, (0, ids)


def semver_compare(a, b):
    left, right = _key(a), _key(b)
    return (left > right) - (left < right)
