def retry(fn, attempts, backoff=0.0, sleep=None):
    if attempts < 1:
        raise ValueError("attempts must be positive")
    last = None
    for attempt in range(attempts):
        try:
            return fn()
        except Exception as error:  # noqa: BLE001
            last = error
            if sleep is not None and attempt + 1 < attempts:
                sleep(backoff * (2 ** attempt))
    raise last
