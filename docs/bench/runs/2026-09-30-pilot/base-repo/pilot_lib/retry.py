def retry(fn, attempts, backoff=0.0, sleep=None):
    """Call `fn` until it succeeds, at most `attempts` times in total."""
    last = None
    for attempt in range(attempts - 1):
        try:
            return fn()
        except Exception as error:  # noqa: BLE001
            last = error
            if sleep is not None:
                sleep(backoff)
    raise last
