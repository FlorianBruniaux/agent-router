class LRUCache:
    """Least-recently-used cache with a fixed capacity."""

    def __init__(self, capacity):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self.capacity = capacity
        self._data = {}

    def get(self, key, default=None):
        return self._data.get(key, default)

    def put(self, key, value):
        self._data[key] = value
        if len(self._data) > self.capacity:
            oldest = next(iter(self._data))
            del self._data[oldest]

    def __len__(self):
        return len(self._data)
