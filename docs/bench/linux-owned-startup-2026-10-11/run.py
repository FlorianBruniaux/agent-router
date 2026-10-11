#!/usr/bin/env python3
"""Re-use the retained bounded Linux collector; require producer-owned startup."""
import importlib.util
import re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('owned_startup_linux_base', ROOT/'docs/bench/linux-copied-client-2026-10-11/run.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
_validate_base = base.validate_complete

def validate_complete(record):
    _validate_base(record)
    row = record['provisional']
    assert row['startup_contract'] == 'producer-owned-v1'
    for key in ('startup_binding_sha256', 'native_environment_sha256'):
        assert isinstance(row[key], str) and re.fullmatch('[a-f0-9]{64}', row[key])

if __name__ == '__main__':
    # Base main retains its input pins, budgets and independent teardown oracle.
    # The stricter validator runs before a campaign can return passed.
    base.validate_complete = validate_complete
    base.main()

