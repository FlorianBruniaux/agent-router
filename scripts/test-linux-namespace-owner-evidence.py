#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Negative receipt witnesses only; these tests do not qualify an OS sandbox."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
FOLDER = ROOT / "docs/bench/linux-namespace-owner-2026-10-15"
spec = importlib.util.spec_from_file_location("namespace_owner", FOLDER / "run.py")
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


class Receipts(unittest.TestCase):
    def setUp(self):
        self.row = {"diagnostic_only": True, "f12a": "NO_GO", "case": "normal",
            "address_space_bytes": 8589934592, "protected_identities_after": "unchanged",
            "preparation_owner_transferred": True, "controller_still_owns_copies": False,
            "namespace_cleanup": "COMPLETE", "heartbeat_before_stop": [20, 30],
            "heartbeat_after_stop": [40, 40, 40], "namespace_owner": {"sha256": "a" * 64},
            "native_rpc": "initialize-and-thread-start-only", "provider_turns": 0,
            "broker_connections": 3, "other_listener_connections": 1,
            "canary": {"memory": "8GiB", "protected_assets": 7, "broker": "exact-port"},
            "copy_hashes": ["b" * 64] * 6, "helper_exit_code": 0,
            "lease": "router-runtime-lease-v1:namespace-clean"}
        self.record = {"image": r.IMAGE, "model_calls": 0, "provisional": self.row,
            "stop_reason": None, "diagnostic_only": True, "f12a": "NO_GO",
            "memory_bytes": 8589934592, "namespace_exit_code": 137,
            "namespace_stopped": True, "container_absent": True, "volume_removed": True,
            "heartbeat_before_teardown": [40, 40], "heartbeat_after_teardown": [40, 40, 40]}

    def reject(self):
        with self.assertRaises((AssertionError, KeyError)):
            r.validate_complete(self.record)

    def test_complete_synthetic_receipt(self):
        r.validate_complete(self.record)

    def test_no_positive_activity_before_stop(self):
        self.row["heartbeat_before_stop"] = [30, 30]
        self.reject()

    def test_writer_survives_inner_stop(self):
        self.row["heartbeat_after_stop"] = [40, 41, 42]
        self.reject()

    def test_outside_observer_disagrees(self):
        self.record["heartbeat_before_teardown"] = [40, 41]
        self.reject()

    def test_group_only_is_not_namespace_cleanup(self):
        self.row["lease"] = "router-runtime-lease-v1:group-clean"
        self.reject()

    def test_helper_death_cannot_release_resources(self):
        self.row["case"] = "helper-kill"
        self.row["helper_exit_code"] = None
        self.row["lease"] = "router-runtime-lease-v1:active"
        self.reject()

    def test_helper_death_stays_unknown(self):
        self.row.update(case="helper-kill", helper_exit_code=None,
            lease="router-runtime-lease-v1:active", controller_still_owns_copies=True,
            namespace_cleanup="UNKNOWN_RETAINED")
        r.validate_complete(self.record)

    def test_ownership_handoff_missing(self):
        self.row["preparation_owner_transferred"] = False
        self.reject()

    def test_namespace_wrapper_unprotected(self):
        self.row["canary"]["protected_assets"] = 6
        self.reject()

    def test_changed_limit_or_model_turn(self):
        for key, value in (("address_space_bytes", 17179869184), ("provider_turns", 1)):
            original = copy.deepcopy(self.row)
            self.row[key] = value
            self.reject()
            self.row.clear()
            self.row.update(original)

    def test_outer_cleanup_missing(self):
        self.record["volume_removed"] = False
        self.reject()

    def test_recorded_campaign(self):
        evidence = json.loads((FOLDER / "evidence.json").read_text())
        for campaign in (evidence["campaign"], evidence["integrated_campaign"]):
            self.assertEqual([x["case"] for x in campaign["cases"]], list(r.CASES))
            for row in campaign["cases"]:
                r.validate_complete(row)
                self.assertEqual(row["provisional"]["copy_hashes"][:4],
                    [campaign["binaries_sha256"][x] for x in ("client", "native", "helper", "wrapper")])
                self.assertEqual(row["provisional"]["namespace_owner"]["sha256"],
                    campaign["binaries_sha256"]["namespace"])
        # These are immutable build inputs, not a demand that future product HEADs match.
        for name in ("run.py", "inside.py", "isolate.c"):
            key = "docs/bench/linux-namespace-owner-2026-10-15/" + name
            self.assertEqual(hashlib.sha256((FOLDER / name).read_bytes()).hexdigest(), evidence["source_sha256"][key])


if __name__ == "__main__":
    unittest.main(verbosity=2)
