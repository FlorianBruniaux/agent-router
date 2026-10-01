def merge_half_open(intervals, merge_touching=False):
    ordered = []
    for interval in intervals:
        if len(interval) != 2:
            raise ValueError("interval must contain two endpoints")
        start, end = interval
        if not isinstance(start, int) or not isinstance(end, int):
            raise ValueError("interval endpoints must be integers")
        if start > end:
            raise ValueError("reversed interval")
        if start < end:
            ordered.append((start, end))
    ordered.sort()
    merged = []
    for start, end in ordered:
        if not merged or (start > merged[-1][1] if merge_touching else start >= merged[-1][1]):
            merged.append((start, end))
        else:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
    return merged
