import hashlib
import json
import subprocess
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[4]
PROJECTIONS = REPO / "docs" / "bench" / "runs" / "2026-09-30-egress-spike" / "results" / "v4-startup"
EXPECTED = ("20260930T190002Z", "20260930T190228Z", "20260930T190433Z")


class V4ProjectionTests(unittest.TestCase):
    def test_three_startup_projections_are_allowlisted_and_have_source_hashes(self):
        for run_id in EXPECTED:
            with self.subTest(run_id=run_id):
                path = PROJECTIONS / f"{run_id}.json"
                data = json.loads(path.read_text())
                self.assertEqual(data["schema"], "agent-router-egress-v4-startup-projection-v1")
                self.assertEqual(data["run_id"], run_id)
                self.assertEqual(set(data), {"schema", "run_id", "source", "observation"})
                self.assertEqual(set(data["source"]["sha256"]), {"manifest.json", "claude.json"})
                for digest in data["source"]["sha256"].values():
                    self.assertRegex(digest, r"^[0-9a-f]{64}$")
                observed = data["observation"]
                self.assertEqual(observed["calls"], 0)
                self.assertEqual(observed["list_cost_microusd"], 0)
                self.assertFalse(observed["startup"]["ok"])
                serialized = json.dumps(data).lower()
                for excluded in ("/users/", "private_config", "keychain", "argv", "read_roots", "write_roots"):
                    self.assertNotIn(excluded, serialized)

    def test_redactions_catalog_matches_projection_hashes(self):
        spike = PROJECTIONS.parents[1]
        catalog = json.loads((spike / "REDACTIONS").read_text())
        entries = {item["path"]: item for item in catalog["files"]}
        for run_id in EXPECTED:
            relative = f"results/v4-startup/{run_id}.json"
            raw = (spike / relative).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), entries[relative]["redacted_sha256"])

    def test_historical_redaction_entries_and_checksum_paths_are_preserved(self):
        spike = PROJECTIONS.parents[1]
        redactions_path = spike / "REDACTIONS"
        checksums_path = spike / "SHA256SUMS"

        def from_head(path):
            rel = path.relative_to(REPO).as_posix()
            return subprocess.run(["git", "show", f"HEAD:{rel}"], cwd=REPO, check=True,
                                  capture_output=True, text=True).stdout

        old_catalog = json.loads(from_head(redactions_path))
        new_catalog = json.loads(redactions_path.read_text())
        self.assertEqual(new_catalog["files"][:len(old_catalog["files"])], old_catalog["files"])

        def rows(text):
            return {line.partition("  ")[2]: line for line in text.splitlines() if line.strip()}

        old_rows = rows(from_head(checksums_path))
        current_rows = rows(checksums_path.read_text())
        for path, old_line in old_rows.items():
            if path not in ("README.md", "REDACTIONS"):
                self.assertEqual(current_rows[path], old_line)
        for run_id in EXPECTED:
            self.assertIn(f"results/v4-startup/{run_id}.json", current_rows)


if __name__ == "__main__":
    unittest.main()
