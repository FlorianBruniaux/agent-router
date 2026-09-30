"""Writes the ten pilot tasks: statement, hidden acceptance test, reference solution."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
TASKS = [
    dict(id="t01", kind="implement", risk="low", module="pagination",
         statement="`pilot_lib.pagination.paginate(items, page, size)` returns the wrong items: page 1 must return the first `size` items (pages are 1-based). `page_count(total, size)` must count a last partial page. Fix both functions without changing their signatures. Keep the existing ValueError checks.",
         test='''
from pilot_lib.pagination import paginate, page_count
class T(unittest.TestCase):
    def test_first_page(self):
        self.assertEqual(paginate(list(range(10)), 1, 3), [0, 1, 2])
    def test_last_partial_page(self):
        self.assertEqual(paginate(list(range(10)), 4, 3), [9])
    def test_past_end(self):
        self.assertEqual(paginate(list(range(10)), 5, 3), [])
    def test_count(self):
        self.assertEqual(page_count(10, 3), 4)
        self.assertEqual(page_count(9, 3), 3)
        self.assertEqual(page_count(0, 3), 0)
    def test_invalid(self):
        with self.assertRaises(ValueError):
            paginate([1], 0, 1)
''',
         solution='''def paginate(items, page, size):
    if page < 1 or size < 1:
        raise ValueError("page and size must be positive")
    start = (page - 1) * size
    return list(items[start:start + size])


def page_count(total, size):
    if size < 1:
        raise ValueError("size must be positive")
    return -(-total // size)
'''),
    dict(id="t02", kind="implement", risk="low", module="text",
         statement="Implement `pilot_lib.text.slugify(text)`: lowercase, strip accents (é becomes e), replace every run of characters that are not ASCII letters or digits with a single hyphen, and remove leading and trailing hyphens. Leave `word_wrap` untouched.",
         test='''
from pilot_lib.text import slugify
class T(unittest.TestCase):
    def test_basic(self):
        self.assertEqual(slugify("Hello World"), "hello-world")
    def test_accents(self):
        self.assertEqual(slugify("Crème brûlée à Paris"), "creme-brulee-a-paris")
    def test_runs_and_edges(self):
        self.assertEqual(slugify("  --Rust & Go!! 2026--  "), "rust-go-2026")
    def test_empty(self):
        self.assertEqual(slugify("!!!"), "")
''',
         solution=None),
    dict(id="t03", kind="implement", risk="medium", module="durations",
         statement="`pilot_lib.durations.parse_duration(value)` must accept hours, minutes and seconds in any combination and order (for example '1h30m', '45s', '2m5s', '1h1s'), ignore surrounding whitespace, and raise ValueError for an empty string, an unknown unit, a unit without a number or a trailing number without a unit.",
         test='''
from pilot_lib.durations import parse_duration
class T(unittest.TestCase):
    def test_values(self):
        self.assertEqual(parse_duration("1h30m"), 5400)
        self.assertEqual(parse_duration("45s"), 45)
        self.assertEqual(parse_duration("2m5s"), 125)
        self.assertEqual(parse_duration(" 1h1s "), 3601)
    def test_errors(self):
        for bad in ["", "10", "5d", "h", "1h30"]:
            with self.assertRaises(ValueError, msg=bad):
                parse_duration(bad)
''',
         solution='''def parse_duration(value):
    units = {"h": 3600, "m": 60, "s": 1}
    value = value.strip()
    if not value:
        raise ValueError("empty duration")
    total = 0
    number = ""
    for char in value:
        if char.isdigit():
            number += char
        elif char in units and number:
            total += int(number) * units[char]
            number = ""
        else:
            raise ValueError("invalid duration")
    if number:
        raise ValueError("missing unit")
    return total
'''),
    dict(id="t04", kind="implement", risk="low", module="intervals",
         statement="Implement `pilot_lib.intervals.merge_intervals(intervals)`: take a list of `(start, end)` tuples with start <= end, in any order, and return a new sorted list of tuples where overlapping or touching intervals (end == next start) are merged. Do not modify the input list.",
         test='''
from pilot_lib.intervals import merge_intervals
class T(unittest.TestCase):
    def test_merge(self):
        self.assertEqual(merge_intervals([(5, 7), (1, 3), (2, 4)]), [(1, 4), (5, 7)])
    def test_touching(self):
        self.assertEqual(merge_intervals([(1, 2), (2, 3)]), [(1, 3)])
    def test_nested_and_empty(self):
        self.assertEqual(merge_intervals([(1, 10), (2, 3)]), [(1, 10)])
        self.assertEqual(merge_intervals([]), [])
    def test_input_unchanged(self):
        data = [(3, 4), (1, 2)]
        merge_intervals(data)
        self.assertEqual(data, [(3, 4), (1, 2)])
''',
         solution='''def merge_intervals(intervals):
    merged = []
    for start, end in sorted(intervals):
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged
'''),
    dict(id="t05", kind="implement", risk="medium", module="cache",
         statement="`pilot_lib.cache.LRUCache` evicts the oldest inserted key instead of the least recently used one. A successful `get` and a `put` on an existing key must both mark the key as most recently used. Keep the public interface (`get`, `put`, `__len__`, capacity check).",
         test='''
from pilot_lib.cache import LRUCache
class T(unittest.TestCase):
    def test_get_refreshes(self):
        c = LRUCache(2)
        c.put("a", 1); c.put("b", 2)
        self.assertEqual(c.get("a"), 1)
        c.put("c", 3)
        self.assertIsNone(c.get("b"))
        self.assertEqual(c.get("a"), 1)
    def test_put_refreshes(self):
        c = LRUCache(2)
        c.put("a", 1); c.put("b", 2); c.put("a", 10); c.put("c", 3)
        self.assertEqual(c.get("a"), 10)
        self.assertIsNone(c.get("b"))
        self.assertEqual(len(c), 2)
    def test_miss_does_not_refresh(self):
        c = LRUCache(1)
        c.put("a", 1)
        self.assertEqual(c.get("x", 0), 0)
        c.put("b", 2)
        self.assertIsNone(c.get("a"))
''',
         solution='''from collections import OrderedDict


class LRUCache:
    def __init__(self, capacity):
        if capacity < 1:
            raise ValueError("capacity must be positive")
        self.capacity = capacity
        self._data = OrderedDict()

    def get(self, key, default=None):
        if key not in self._data:
            return default
        self._data.move_to_end(key)
        return self._data[key]

    def put(self, key, value):
        self._data[key] = value
        self._data.move_to_end(key)
        if len(self._data) > self.capacity:
            self._data.popitem(last=False)

    def __len__(self):
        return len(self._data)
'''),
    dict(id="t06", kind="implement", risk="low", module="roman",
         statement="`pilot_lib.roman.roman_to_int(numeral)` ignores subtractive notation (IV, IX, XL, XC, CD, CM). Fix it, and raise ValueError for an empty string or a character that is not a Roman digit.",
         test='''
from pilot_lib.roman import roman_to_int
class T(unittest.TestCase):
    def test_values(self):
        for numeral, value in [("III", 3), ("IV", 4), ("IX", 9), ("LVIII", 58), ("MCMXCIV", 1994), ("MMXXVI", 2026)]:
            self.assertEqual(roman_to_int(numeral), value, numeral)
    def test_errors(self):
        for bad in ["", "ABC", "xiv"]:
            with self.assertRaises(ValueError, msg=bad):
                roman_to_int(bad)
''',
         solution='''VALUES = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100, "D": 500, "M": 1000}


def roman_to_int(numeral):
    if not numeral or any(char not in VALUES for char in numeral):
        raise ValueError("invalid numeral")
    total = 0
    for index, char in enumerate(numeral):
        value = VALUES[char]
        if index + 1 < len(numeral) and value < VALUES[numeral[index + 1]]:
            total -= value
        else:
            total += value
    return total
'''),
    dict(id="t07", kind="implement", risk="medium", module="tables",
         statement="`pilot_lib.tables.csv_sum(path, column)` crashes on the header row and on empty cells. It must skip the header, treat empty or whitespace-only cells as missing (not zero, just skipped), raise KeyError if the column does not exist, and return a float.",
         test='''
import os, tempfile
from pilot_lib.tables import csv_sum
class T(unittest.TestCase):
    def write(self, text):
        handle = tempfile.NamedTemporaryFile("w", suffix=".csv", delete=False)
        handle.write(text); handle.close()
        self.addCleanup(os.unlink, handle.name)
        return handle.name
    def test_sum(self):
        self.assertEqual(csv_sum(self.write("a,b\\n1,2\\n3,4.5\\n"), "b"), 6.5)
    def test_missing_cells(self):
        self.assertEqual(csv_sum(self.write("a,b\\n1,\\n2, \\n3,1\\n"), "b"), 1.0)
    def test_unknown_column(self):
        with self.assertRaises(KeyError):
            csv_sum(self.write("a\\n1\\n"), "z")
''',
         solution='''import csv


def csv_sum(path, column):
    with open(path, newline="") as handle:
        rows = list(csv.reader(handle))
    if not rows or column not in rows[0]:
        raise KeyError(column)
    index = rows[0].index(column)
    total = 0.0
    for row in rows[1:]:
        if index < len(row) and row[index].strip():
            total += float(row[index])
    return total
'''),
    dict(id="t08", kind="implement", risk="high", module="retry",
         statement="`pilot_lib.retry.retry(fn, attempts, backoff=0.0, sleep=None)` calls `fn` one time too few and sleeps a constant delay. It must call `fn` at most `attempts` times, return the first successful result, sleep between attempts only (never after the last), use exponential backoff (backoff, 2*backoff, 4*backoff, ...) when `sleep` is given, re-raise the last exception, and raise ValueError when attempts < 1.",
         test='''
from pilot_lib.retry import retry
class T(unittest.TestCase):
    def flaky(self, failures):
        state = {"calls": 0}
        def fn():
            state["calls"] += 1
            if state["calls"] <= failures:
                raise RuntimeError(state["calls"])
            return "ok"
        return fn, state
    def test_success_after_failures(self):
        fn, state = self.flaky(2)
        sleeps = []
        self.assertEqual(retry(fn, 3, backoff=1.0, sleep=sleeps.append), "ok")
        self.assertEqual(state["calls"], 3)
        self.assertEqual(sleeps, [1.0, 2.0])
    def test_exhausted(self):
        fn, state = self.flaky(5)
        sleeps = []
        with self.assertRaises(RuntimeError):
            retry(fn, 3, backoff=0.5, sleep=sleeps.append)
        self.assertEqual(state["calls"], 3)
        self.assertEqual(sleeps, [0.5, 1.0])
    def test_invalid(self):
        with self.assertRaises(ValueError):
            retry(lambda: 1, 0)
''',
         solution='''def retry(fn, attempts, backoff=0.0, sleep=None):
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
'''),
    dict(id="t09", kind="implement", risk="medium", module="semver",
         statement="`pilot_lib.semver.semver_compare(a, b)` must follow Semantic Versioning 2.0.0 precedence: compare MAJOR.MINOR.PATCH numerically, a pre-release version (`1.0.0-alpha`) has lower precedence than the release, pre-release identifiers are compared dot by dot (numeric identifiers numerically, alphanumeric ones lexically, numeric lower than alphanumeric, a shorter set lower when all preceding identifiers are equal), and build metadata after `+` is ignored.",
         test='''
from pilot_lib.semver import semver_compare
class T(unittest.TestCase):
    def test_core(self):
        self.assertEqual(semver_compare("1.10.0", "1.9.0"), 1)
        self.assertEqual(semver_compare("1.0.0", "1.0.0"), 0)
    def test_prerelease_chain(self):
        chain = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
        for low, high in zip(chain, chain[1:]):
            self.assertEqual(semver_compare(low, high), -1, (low, high))
            self.assertEqual(semver_compare(high, low), 1, (high, low))
    def test_build_ignored(self):
        self.assertEqual(semver_compare("1.0.0+build.5", "1.0.0+other"), 0)
''',
         solution='''def _key(version):
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
'''),
    dict(id="t10", kind="implement", risk="low", module="text",
         statement="Implement `pilot_lib.text.word_wrap(text, width)`: split on whitespace, greedily fill lines of at most `width` characters with words separated by single spaces, put a word longer than `width` alone on its own line without cutting it, return the list of lines (an empty list for blank text), and raise ValueError when width < 1. Leave `slugify` untouched.",
         test='''
from pilot_lib.text import word_wrap
class T(unittest.TestCase):
    def test_wrap(self):
        self.assertEqual(word_wrap("the quick brown fox jumps", 10), ["the quick", "brown fox", "jumps"])
    def test_long_word(self):
        self.assertEqual(word_wrap("a extraordinarily b", 5), ["a", "extraordinarily", "b"])
    def test_blank_and_invalid(self):
        self.assertEqual(word_wrap("   ", 4), [])
        with self.assertRaises(ValueError):
            word_wrap("x", 0)
''',
         solution=None),
]

TEXT_SOLUTION = '''import unicodedata


def slugify(text):
    ascii_text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    out, dash = [], False
    for char in ascii_text.lower():
        if char.isascii() and char.isalnum():
            out.append(char)
            dash = False
        elif not dash:
            out.append("-")
            dash = True
    return "".join(out).strip("-")


def word_wrap(text, width):
    if width < 1:
        raise ValueError("width must be positive")
    lines, current = [], ""
    for word in text.split():
        if not current:
            current = word
        elif len(current) + 1 + len(word) <= width:
            current += " " + word
        else:
            lines.append(current)
            current = word
    if current:
        lines.append(current)
    return lines
'''

for task in TASKS:
    folder = ROOT / "tasks" / task["id"]
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "statement.md").write_text(task["statement"] + "\n")
    (folder / "hidden_test.py").write_text("import unittest\n" + task["test"].lstrip("\n"))
    meta = {k: task[k] for k in ("id", "kind", "risk", "module")}
    meta["target"] = f"pilot_lib/{task['module']}.py"
    (folder / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    solution = task["solution"] or TEXT_SOLUTION
    (ROOT / "solutions" / f"{task['id']}.py").write_text(solution)
