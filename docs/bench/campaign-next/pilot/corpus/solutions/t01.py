def collect_pages(fetch_page, max_pages=100):
    if type(max_pages) is not int or max_pages < 1:
        raise ValueError("max_pages must be a positive integer")
    cursor = None
    seen = {None}
    items = []
    for _ in range(max_pages):
        page_items, next_cursor = fetch_page(cursor)
        items.extend(page_items)
        if next_cursor is None:
            return items
        try:
            repeated = next_cursor in seen
        except TypeError as exc:
            raise ValueError("cursor must be hashable") from exc
        if repeated:
            raise ValueError("repeated cursor")
        seen.add(next_cursor)
        cursor = next_cursor
    raise ValueError("max_pages exceeded")
