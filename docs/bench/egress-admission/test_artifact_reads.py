"""Artifact refusal tests use synthetic data outside the child fixture tree."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import subprocess
import sys

import claude_local as driver


class ArtifactReads(unittest.TestCase):
    def test_sample_cannot_overwrite_external_file_through_workspace_symlink(self):
        with tempfile.TemporaryDirectory() as tree, tempfile.TemporaryDirectory() as external:
            root, outside = Path(tree), Path(external)
            workspace, supervisor = root / "workspace", root / "supervisor"
            workspace.mkdir()
            supervisor.mkdir(mode=0o700)
            target = outside / "synthetic-private-file"
            target.write_text("SYNTHETIC_KEEP_UNCHANGED")
            (workspace / "startup-sample.txt").symlink_to(target)
            destinations = []

            def simulated_sampler(argv, **kwargs):
                destination = Path(argv[argv.index("-file") + 1])
                destinations.append(destination)
                destination.write_text("SYNTHETIC_SAMPLE")
                return subprocess.CompletedProcess(argv, 0)

            with mock.patch.object(driver.subprocess, "run", side_effect=simulated_sampler):
                driver.bounded([sys.executable, "-B", "-c", "import time;time.sleep(10)"],
                               {"PATH": "/usr/bin:/bin"}, workspace, 3.1,
                               sample=True, sample_dir=supervisor)
            self.assertEqual(target.read_text(), "SYNTHETIC_KEEP_UNCHANGED")
            self.assertEqual(destinations, [supervisor / "startup-sample.txt"])
            self.assertEqual(driver.read_artifact(destinations[0], 24000, excerpt=True),
                             (b"SYNTHETIC_SAMPLE", "ok"))
            artifacts = driver.collect_artifacts(supervisor, workspace, supervisor)
            self.assertEqual(artifacts["sample_excerpt"], "SYNTHETIC_SAMPLE")
            self.assertNotIn("SYNTHETIC_KEEP_UNCHANGED", json.dumps(artifacts))

    @unittest.skipUnless(sys.platform == "darwin", "Seatbelt requires macOS")
    def test_seatbelt_child_cannot_mutate_supervisor_sample_directory(self):
        with tempfile.TemporaryDirectory(prefix="ar-sample-write-", dir="/private/tmp") as tree:
            root = Path(tree).resolve()
            home, state, workspace, workerhome, supervisor = [root / name for name in
                ["home", "state", "workspace", "worker-home", "supervisor"]]
            for path in [home, state, workspace, workerhome, supervisor]:
                path.mkdir(mode=0o700)
            sample = supervisor / "startup-sample.txt"
            sample.write_text("SYNTHETIC_PARENT_SAMPLE")
            code = """import json,sys
from pathlib import Path
root = Path(sys.argv[1]); results = {}
operations = {
 'write_existing': lambda: (root/'supervisor/startup-sample.txt').write_text('modified'),
 'create_symlink': lambda: (root/'supervisor/new-link').symlink_to(root/'workspace/target'),
 'unlink_sample': lambda: (root/'supervisor/startup-sample.txt').unlink(),
 'rename_directory': lambda: (root/'supervisor').rename(root/'workspace/moved'),
 'workspace_control': lambda: (root/'workspace/control').write_text('allowed')}
for name, operation in operations.items():
 try: operation(); results[name] = 'allowed'
 except OSError as error: results[name] = error.errno
print(json.dumps(results))
"""
            # Same controller write set as run_case; no network grant is needed.
            child_profile = driver.profile([root], [home, state, workspace, workerhome], None)
            result = driver.bounded(["/usr/bin/sandbox-exec", "-p", child_profile,
                                     "/usr/bin/python3", "-B", "-c", code, str(root)],
                                    {"PATH": "/usr/bin:/bin", "HOME": str(home)}, workspace, 3)
            self.assertEqual(result["exit_code"], 0, result["stderr"])
            self.assertEqual(json.loads(result["stdout"]), {
                "write_existing": 1, "create_symlink": 1, "unlink_sample": 1,
                "rename_directory": 1, "workspace_control": "allowed"})
            self.assertEqual(sample.read_text(), "SYNTHETIC_PARENT_SAMPLE")

    def test_sampling_requires_explicit_supervisor_directory(self):
        with self.assertRaisesRegex(ValueError, "supervisor-owned"):
            driver.bounded([], {}, Path("/private/tmp"), 1, sample=True)

    def test_child_symlinks_cannot_export_external_synthetic_artifacts(self):
        with tempfile.TemporaryDirectory() as tree, tempfile.TemporaryDirectory() as external:
            root, outside = Path(tree), Path(external)
            state, workspace = root / "state", root / "workspace"
            state.mkdir()
            workspace.mkdir()
            marker = "SYNTHETIC_EXTERNAL_ARTIFACT_NOT_FOR_EXPORT"
            for parent, name, content in [
                (state, "debug.log", marker),
                (workspace, "startup-sample.txt", marker),
                (workspace, "nonce.txt", driver.NONCE),
                (workspace, "worker-evidence.json", json.dumps({"external": marker})),
            ]:
                (outside / name).write_text(content)
                (parent / name).symlink_to(outside / name)
            result = driver.collect_artifacts(state, workspace, workspace)
            self.assertNotIn(marker, json.dumps(result))
            self.assertIsNone(result["debug_excerpt"])
            self.assertIsNone(result["sample_excerpt"])
            self.assertFalse(result["nonce_exact"])
            self.assertIsNone(result["worker_evidence"])

    def test_replaced_parent_directory_is_refused(self):
        with tempfile.TemporaryDirectory() as tree, tempfile.TemporaryDirectory() as external:
            root, outside = Path(tree), Path(external)
            (outside / "debug.log").write_text("SYNTHETIC_EXTERNAL_PARENT")
            (root / "state").symlink_to(outside, target_is_directory=True)
            workspace = root / "workspace"
            workspace.mkdir()
            result = driver.collect_artifacts(root / "state", workspace, workspace)
            self.assertIsNone(result["debug_excerpt"])
            self.assertEqual(result["artifact_reads"]["debug"], "refused")

    def test_fifo_and_directory_are_refused_without_waiting_for_writer(self):
        with tempfile.TemporaryDirectory() as tree:
            root = Path(tree)
            state, workspace = root / "state", root / "workspace"
            state.mkdir()
            workspace.mkdir()
            for parent, name in [(state, "debug.log"), (workspace, "startup-sample.txt"),
                                 (workspace, "nonce.txt"), (workspace, "worker-evidence.json")]:
                os.mkfifo(parent / name)
            code = "import json,sys;from pathlib import Path;import claude_local;print(json.dumps(claude_local.collect_artifacts(Path(sys.argv[1]),Path(sys.argv[2]),Path(sys.argv[2]))))"
            result = subprocess.run([sys.executable, "-B", "-c", code, str(state), str(workspace)],
                                    cwd=Path(driver.__file__).parent, capture_output=True, text=True, timeout=3, check=True)
            self.assertEqual(set(json.loads(result.stdout)["artifact_reads"].values()), {"refused"})
            (state / "debug.log").unlink()
            (state / "debug.log").mkdir()
            self.assertEqual(driver.collect_artifacts(state, workspace, workspace)["artifact_reads"]["debug"], "refused")

    def test_large_files_are_read_with_fixed_byte_budgets(self):
        with tempfile.TemporaryDirectory() as tree:
            root = Path(tree)
            state, workspace = root / "state", root / "workspace"
            state.mkdir()
            workspace.mkdir()
            for parent, name in [(state, "debug.log"), (workspace, "startup-sample.txt"),
                                 (workspace, "nonce.txt"), (workspace, "worker-evidence.json")]:
                with (parent / name).open("wb") as stream:
                    stream.write(b"HEAD")
                    stream.seek(1024 * 1024 * 1024)
                    stream.write(b"TAIL")
            sizes = []
            real_read = os.read

            def measured_read(fd, size):
                data = real_read(fd, size)
                sizes.append(len(data))
                return data

            with mock.patch.object(driver.os, "read", side_effect=measured_read):
                result = driver.collect_artifacts(state, workspace, workspace)
            self.assertEqual(sum(sizes), 20000 + 24000)
            self.assertEqual(len(result["debug_excerpt"]), 20000)
            self.assertTrue(result["debug_excerpt"].endswith("TAIL"))
            self.assertEqual(len(result["sample_excerpt"]), 24000)
            self.assertTrue(result["sample_excerpt"].startswith("HEAD"))
            self.assertFalse(result["nonce_exact"])
            self.assertIsNone(result["worker_evidence"])
            self.assertEqual(result["artifact_reads"]["nonce"], "too_large")
            self.assertEqual(result["artifact_reads"]["worker_evidence"], "too_large")

    def test_opened_file_stays_pinned_when_path_is_replaced(self):
        with tempfile.TemporaryDirectory() as tree, tempfile.TemporaryDirectory() as external:
            root, outside = Path(tree), Path(external)
            state, workspace = root / "state", root / "workspace"
            state.mkdir()
            workspace.mkdir()
            path = state / "debug.log"
            path.write_text("original controlled fixture")
            (outside / "private").write_text("SYNTHETIC_EXTERNAL_REPLACEMENT")
            real_open = os.open

            def replacing_open(path_arg, flags, *args, **kwargs):
                fd = real_open(path_arg, flags, *args, **kwargs)
                if path_arg == "debug.log":
                    path.unlink()
                    path.symlink_to(outside / "private")
                return fd

            with mock.patch.object(driver.os, "open", side_effect=replacing_open):
                result = driver.collect_artifacts(state, workspace, workspace)
            self.assertEqual(result["debug_excerpt"], "original controlled fixture")
            self.assertNotIn("SYNTHETIC_EXTERNAL_REPLACEMENT", json.dumps(result))

    def test_exact_nonce_and_valid_json_remain_readable(self):
        with tempfile.TemporaryDirectory() as tree:
            root = Path(tree)
            state, workspace = root / "state", root / "workspace"
            state.mkdir()
            workspace.mkdir()
            (workspace / "nonce.txt").write_text(driver.NONCE)
            (workspace / "worker-evidence.json").write_text('{"fixture":true}')
            result = driver.collect_artifacts(state, workspace, workspace)
            self.assertTrue(result["nonce_exact"])
            self.assertEqual(result["worker_evidence"], {"fixture": True})
            self.assertEqual(result["artifact_reads"]["debug"], "missing")
            (workspace / "worker-evidence.json").write_text('{"invalid"')
            result = driver.collect_artifacts(state, workspace, workspace)
            self.assertIsNone(result["worker_evidence"])
            self.assertEqual(result["artifact_reads"]["worker_evidence"], "invalid")

    def test_growth_after_stat_cannot_bypass_complete_artifact_limit(self):
        with tempfile.TemporaryDirectory() as tree:
            root = Path(tree)
            path = root / "nonce.txt"
            path.write_text(driver.NONCE)
            real_fstat = os.fstat

            def grow_after_stat(fd):
                info = real_fstat(fd)
                with path.open("ab") as stream:
                    stream.write(b"unexpected extra data")
                return info

            with mock.patch.object(driver.os, "fstat", side_effect=grow_after_stat):
                data, status = driver.read_artifact(path, len(driver.NONCE.encode()))
            self.assertIsNone(data)
            self.assertEqual(status, "too_large")


if __name__ == "__main__":
    unittest.main()
