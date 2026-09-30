def paginate(items, page, size):
    """Return the items of 1-based page `page` with `size` items per page."""
    if page < 1 or size < 1:
        raise ValueError("page and size must be positive")
    start = page * size
    return list(items[start:start + size])


def page_count(total, size):
    """Number of pages needed to show `total` items."""
    if size < 1:
        raise ValueError("size must be positive")
    return total // size
