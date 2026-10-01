import re


_VERSION = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


def _parse(value):
    match = _VERSION.fullmatch(value)
    if match is None:
        raise ValueError("invalid semantic version")
    major, minor, patch, prerelease, _build = match.groups()
    parts = prerelease.split(".") if prerelease else None
    if parts and any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in parts):
        raise ValueError("numeric prerelease identifiers cannot have leading zeroes")
    return (int(major), int(minor), int(patch)), parts


def _compare_prerelease(left, right):
    if left is None:
        return 0 if right is None else 1
    if right is None:
        return -1
    for a, b in zip(left, right):
        if a == b:
            continue
        a_numeric, b_numeric = a.isdigit(), b.isdigit()
        if a_numeric and b_numeric:
            return -1 if int(a) < int(b) else 1
        if a_numeric != b_numeric:
            return -1 if a_numeric else 1
        return -1 if a < b else 1
    return (len(left) > len(right)) - (len(left) < len(right))


def compare(left, right):
    left_core, left_pre = _parse(left)
    right_core, right_pre = _parse(right)
    if left_core != right_core:
        return (left_core > right_core) - (left_core < right_core)
    return _compare_prerelease(left_pre, right_pre)
