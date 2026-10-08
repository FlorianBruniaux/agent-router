#!/usr/bin/env python3
"""Behavioral tests for the local verification runner (no Cargo/provider calls)."""

import importlib.util
import json
import os
from pathlib import Path
import signal
import shutil
import select
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

SCRIPT = Path(__file__).with_name("check-local.py")


class LocalCheckTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(SCRIPT.is_file(), "local verification runner is missing")
        # Git's environment override disables an inherited trace2.eventTarget
        # collector for disposable repos only. Register restoration first so
        # unittest's LIFO cleanups remove the repo before restoring tracing.
        trace = mock.patch.dict(os.environ, {"GIT_TRACE2_EVENT": "0"})
        trace.start()
        self.addCleanup(trace.stop)
        spec = importlib.util.spec_from_file_location("check_local", SCRIPT)
        self.runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.runner)
        self.temp = tempfile.TemporaryDirectory(prefix="check-local-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "checkout"
        self.root.mkdir()
        (self.root / "Cargo.lock").write_text("fixture lock\n")
        self.out = Path(self.temp.name) / "evidence"
        self.init_git()

    def check(self, name, code):
        return {"id": name, "group": "quality", "argv": [sys.executable, "-c", code]}

    def execute(self, checks, **kwargs):
        return self.runner.execute(self.root, self.out, checks, proxy=None, **kwargs)

    def report(self):
        return json.loads((self.out / "summary.json").read_text())

    def init_git(self, files=None):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "--", "Cargo.lock", *(files or [])], check=True)
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "fixture"], check=True)

    def test_nonzero_child_status_is_preserved_and_other_checks_continue(self):
        code = self.execute([self.check("fail", "print('failure detail'); raise SystemExit(7)"),
                             self.check("pass", "print('success detail')")])
        self.assertEqual(code, 1)
        report = self.report()
        self.assertEqual([(r["status"], r["exit_code"]) for r in report["checks"]],
                         [("failed", 7), ("passed", 0)])
        result = report["checks"][0]
        self.assertIn("failure detail", (self.out / result["log"]).read_text())
        self.assertEqual(len(result["log_sha256"]), 64)
        self.assertGreaterEqual(result["duration_seconds"], 0)
        self.assertEqual(report["before"]["lock_sha256"], report["after"]["lock_sha256"])

    def test_unavailable_executable_is_not_a_pass(self):
        code = self.execute([{"id": "missing", "group": "quality", "argv": ["check-local-never-installed"]}])
        self.assertEqual(code, 1)
        self.assertEqual(self.report()["checks"][0]["status"], "unavailable")
        self.assertIsNone(self.report()["checks"][0]["exit_code"])

    def test_timeout_terminates_child_and_retains_report(self):
        code = self.execute([self.check("slow", "import time; print('started', flush=True); time.sleep(30)")], timeout=0.1)
        self.assertEqual(code, 1)
        result = self.report()["checks"][0]
        self.assertEqual(result["status"], "timed_out")
        self.assertIsNotNone(result["exit_code"])
        # Timeout can occur during interpreter startup; even an empty log survives.
        self.assertTrue((self.out / result["log"]).is_file())
        self.assertEqual(len(result["log_sha256"]), 64)

    def test_stop_handles_child_that_already_exited(self):
        proc = subprocess.Popen([sys.executable, "-c", "pass"], start_new_session=os.name == "posix")
        self.assertEqual(proc.wait(timeout=5), 0)
        try:
            self.runner.stop_process(proc)
        except ProcessLookupError:
            self.fail("cleanup must handle a child exiting just before termination")

    @unittest.skipUnless(os.name == "posix", "descendant cleanup needs POSIX")
    def test_stop_kills_term_ignoring_descendant_after_leader_exits(self):
        marker = self.root / "descendant"
        read_fd, write_fd = os.pipe()
        self.addCleanup(os.close, read_fd)
        child_code = ("import os,signal,time; from pathlib import Path; "
                      "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                      f"Path({str(marker)!r}).write_text(str(os.getpid())); time.sleep(30)")
        leader_code = ("import subprocess,time; "
                       f"subprocess.Popen([{sys.executable!r},'-c',{child_code!r}],pass_fds=({write_fd},)); time.sleep(30)")
        leader = subprocess.Popen([sys.executable, "-c", leader_code], start_new_session=True, pass_fds=(write_fd,))
        os.close(write_fd)
        pipe_holders_exited = False

        def cleanup():
            if not pipe_holders_exited:
                try:
                    os.killpg(leader.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            leader.wait(timeout=5)

        self.addCleanup(cleanup)
        deadline = time.monotonic() + 5
        while not marker.exists() and leader.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(marker.exists(), "TERM-ignoring descendant did not start")
        self.runner.stop_process(leader)
        self.assertIsNotNone(leader.returncode, "leader must have been reaped")
        # EOF proves both pipe holders exited, including an unreaped zombie.
        self.assertTrue(select.select([read_fd], [], [], 1)[0], "descendant still holds its live pipe after leader exited")
        self.assertEqual(os.read(read_fd, 1), b"")
        pipe_holders_exited = True

    def test_existing_output_directory_is_never_overwritten(self):
        self.out.mkdir()
        sentinel = self.out / "summary.json"
        sentinel.write_text("preserve me")
        with self.assertRaises(FileExistsError):
            self.execute([self.check("pass", "print('must not run')")])
        self.assertEqual(sentinel.read_text(), "preserve me")
        self.assertEqual(list(self.out.iterdir()), [sentinel])

    def test_success_and_platform_gap_do_not_claim_full_ci(self):
        code = self.execute([self.check("pass", "pass"),
                             {"id": "linux-only", "group": "native", "argv": [],
                              "not_applicable": "requires Linux"}])
        self.assertEqual(code, 0)
        report = self.report()
        self.assertEqual(report["verdict"], "selected_checks_passed")
        self.assertFalse(report["full_ci_equivalent"])
        self.assertTrue(report["coverage_gaps"])
        self.assertEqual(report["checks"][1]["status"], "not_applicable")

    def test_entirely_not_applicable_selection_is_incomplete(self):
        code = self.execute([{"id": "other-host", "group": "native", "argv": [], "not_applicable": "requires another OS"}])
        self.assertEqual(code, 1)
        self.assertEqual(self.report()["verdict"], "incomplete_or_failed")

    def test_changed_already_dirty_source_refuses_passing_verdict(self):
        source = self.root / "tracked.txt"
        source.write_text("committed")
        self.init_git(["tracked.txt"])
        source.write_text("already dirty")
        check = self.check("mutation", f"from pathlib import Path; Path({str(source)!r}).write_text('different dirty content')")
        self.assertEqual(self.execute([check]), 1)
        report = self.report()
        self.assertEqual(report["before"]["git_status"], report["after"]["git_status"])
        self.assertNotEqual(report["before"]["source_sha256"], report["after"]["source_sha256"])

    def test_changed_existing_untracked_source_refuses_passing_verdict(self):
        self.init_git()
        source = self.root / "new.txt"
        source.write_text("untracked before")
        check = self.check("mutation", f"from pathlib import Path; Path({str(source)!r}).write_text('untracked after')")
        self.assertEqual(self.execute([check]), 1)
        report = self.report()
        self.assertEqual(report["before"]["git_status"], report["after"]["git_status"])
        self.assertNotEqual(report["before"]["source_sha256"], report["after"]["source_sha256"])

    def test_source_fingerprint_captures_mode_and_symlink_target(self):
        self.init_git()
        source = self.root / "mode.txt"
        source.write_text("same bytes")
        source.chmod(0o600)
        link = self.root / "link.txt"
        link.symlink_to("mode.txt")
        before = self.runner.snapshot(self.root)
        source.chmod(0o700)
        changed_mode = self.runner.snapshot(self.root)
        self.assertNotEqual(before.get("source_sha256"), changed_mode.get("source_sha256"))
        link.unlink()
        link.symlink_to("Cargo.lock")
        changed_link = self.runner.snapshot(self.root)
        self.assertNotEqual(changed_mode["source_sha256"], changed_link["source_sha256"])
        self.assertEqual(before["source_inventory_sha256"], changed_link["source_inventory_sha256"])

    def test_head_change_refuses_passing_verdict(self):
        self.init_git()
        git_args = ["git", "-C", str(self.root), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "new head"]
        check = self.check("head-change", f"import subprocess; subprocess.run({git_args!r},check=True)")
        self.assertEqual(self.execute([check]), 1)
        self.assertNotEqual(self.report()["before"]["head_sha"], self.report()["after"]["head_sha"])

    def test_list_mode_runs_no_commands_or_creates_output(self):
        env = dict(os.environ, PATH=str(self.root))
        result = subprocess.run([sys.executable, str(SCRIPT), "--list", "--group", "packaging",
                                 "--output-dir", str(self.out)], env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("packaging", result.stdout)
        self.assertIn("package-core.py", result.stdout)
        self.assertFalse(self.out.exists())

    def test_python_matrix_uses_current_absolute_interpreter(self):
        scripts = self.root / "scripts"
        scripts.mkdir()
        (scripts / "test-foo.py").write_text("")
        checks = self.runner.build_checks(self.root, self.out, ["quality"])
        python_calls = [c["argv"] for c in checks if "scripts/check-host-syntax.py" in c["argv"]]
        self.assertEqual(len(python_calls), 1)
        self.assertEqual(python_calls[0][0], str(Path(sys.executable).resolve()))
        script_calls = [c["argv"] for c in checks if "scripts/test-foo.py" in c["argv"]]
        self.assertEqual(script_calls, [[str(Path(sys.executable).resolve()), "-B", "scripts/test-foo.py"]])

    def test_hyphenated_scripts_all_execute_and_keep_individual_failures(self):
        scripts = self.root / "scripts"
        scripts.mkdir()
        fail_marker = Path(self.temp.name) / "fail-ran"
        pass_marker = Path(self.temp.name) / "pass-ran"
        (scripts / "test-aaa-fail.py").write_text(
            "import unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
            "    def test_failure(self):\n"
            f"        Path({str(fail_marker)!r}).touch()\n"
            "        self.fail('expected fixture failure')\nunittest.main()\n")
        (scripts / "test-zzz-pass.py").write_text(
            "import unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
            "    def test_pass(self):\n"
            f"        Path({str(pass_marker)!r}).touch()\n"
            "        self.assertEqual(2 + 3, 5)\nunittest.main()\n")
        checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"]) if c["id"].startswith("script-")]
        self.assertEqual(self.execute(checks), 1)
        self.assertTrue(fail_marker.exists(), "hyphenated failing script must actually execute")
        self.assertTrue(pass_marker.exists(), "subsequent script must execute after a failed script")
        results = self.report()["checks"]
        self.assertEqual([(r["status"], r["exit_code"]) for r in results], [("failed", 1), ("passed", 0)])
        self.assertNotEqual(results[0]["log"], results[1]["log"])
        self.assertIn("expected fixture failure", (self.out / results[0]["log"]).read_text())
        self.assertEqual(results[1]["tests_passed_or_run"], 1)

    def test_versioned_native_probe_suite_is_executed(self):
        folder = self.root / "docs/bench/native-integration-u02"
        folder.mkdir(parents=True)
        marker = Path(self.temp.name) / "native-probe-ran"
        (folder / "test_probe_v5.py").write_text(
            "import unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
            "    def test_canary(self):\n"
            f"        Path({str(marker)!r}).touch()\n")
        checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"])
                  if c["id"] == "native-probe-tests"]
        self.assertEqual(self.execute(checks), 0)
        self.assertTrue(marker.exists(), "versioned probe regression must execute")
        self.assertEqual(self.report()["checks"][0]["tests_passed_or_run"], 1)

    def test_hook_failure_diagnostic_executes_and_retains_failure(self):
        folder = self.root / "docs/bench/native-integration-u07"
        folder.mkdir(parents=True)
        marker = Path(self.temp.name) / "hook-failure-probe-ran"
        (folder / "test_failure_probe.py").write_text(
            "import os, unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
            "    def test_canary(self):\n"
            f"        self.assertEqual(os.environ.get('U07_REPO_ROOT'), {str(self.root)!r})\n"
            f"        Path({str(marker)!r}).touch()\n"
            "        self.fail('hook diagnostic failure sentinel')\n")
        checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"])
                  if c["id"] == "native-hook-failure-tests"]
        self.assertEqual(len(checks), 1, "hook failure diagnostic must be registered")
        self.assertEqual(self.execute(checks), 1)
        self.assertTrue(marker.exists(), "hook diagnostic must execute")
        result = self.report()["checks"][0]
        self.assertEqual(result["status"], "failed")
        self.assertIn("hook diagnostic failure sentinel", (self.out / result["log"]).read_text())

    def test_calibration_failure_is_executed_and_retained(self):
        folder = self.root / "docs/bench/calibration-v2/tests"
        folder.mkdir(parents=True)
        marker = Path(self.temp.name) / "calibration-ran"
        (folder / "test_manifest.py").write_text(
            "import unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
            "    def test_invalid_manifest(self):\n"
            f"        Path({str(marker)!r}).touch()\n"
            "        self.fail('invalid manifest sentinel')\n")
        checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"])
                  if c["id"] == "calibration-tests"]
        self.assertEqual(self.execute(checks), 1)
        self.assertTrue(marker.exists(), "calibration regressions must execute")
        result = self.report()["checks"][0]
        self.assertEqual(result["status"], "failed")
        self.assertIn("invalid manifest sentinel", (self.out / result["log"]).read_text())

    def test_owned_entry_and_send_regressions_execute_and_retain_failure(self):
        cases = [
            ("physical-send-tests", "docs/bench/physical-send-feasibility-2026-10-07", "test_probe.py"),
            ("startup-outage-tests", "docs/bench/physical-send-feasibility-2026-10-07/outage-cancellation", "test_outage.py"),
            ("owned-entry-tests", "docs/bench/owned-entry-qualification-2026-10-08", "test_direct_client.py"),
        ]
        for identifier, relative, filename in cases:
            with self.subTest(check=identifier):
                folder = self.root / relative
                folder.mkdir(parents=True, exist_ok=True)
                marker = Path(self.temp.name) / (identifier + "-executed")
                (folder / filename).write_text(
                    "import unittest\nfrom pathlib import Path\nclass Suite(unittest.TestCase):\n"
                    "    def test_regression(self):\n"
                    f"        Path({str(marker)!r}).touch()\n"
                    "        self.fail('send boundary regression sentinel')\n")
                checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"])
                          if c["id"] == identifier]
                self.assertEqual(len(checks), 1, "send boundary suite must be registered")
                self.out = Path(self.temp.name) / (identifier + "-evidence")
                self.assertEqual(self.execute(checks), 1)
                self.assertTrue(marker.exists(), "registered regression must execute")
                result = self.report()["checks"][0]
                self.assertEqual(result["status"], "failed")
                self.assertIn("send boundary regression sentinel", (self.out / result["log"]).read_text())

    def test_missing_script_suites_are_unavailable(self):
        checks = [c for c in self.runner.build_checks(self.root, self.out, ["quality"]) if c["id"].startswith("script-")]
        self.assertEqual(self.execute(checks), 1)
        self.assertEqual(self.report()["checks"][0]["status"], "unavailable")

    def test_fuzz_group_reports_gap_without_mutating_corpus(self):
        code = self.execute(self.runner.build_checks(self.root, self.out, ["fuzz"]))
        self.assertEqual(code, 1)
        self.assertEqual(self.report()["checks"][0]["status"], "unavailable")
        self.assertIn("isolated", self.report()["checks"][0]["reason"])

    def test_zero_executed_tests_is_failure_even_with_exit_zero(self):
        check = self.check("empty-tests", "print('Ran 0 tests in 0.000s'); print('OK')")
        check["require_tests"] = True
        self.assertEqual(self.execute([check]), 1)
        result = self.report()["checks"][0]
        self.assertEqual((result["status"], result["exit_code"]), ("failed", 0))
        self.assertEqual(result["tests_passed_or_run"], 0)

    def test_entirely_skipped_python_suite_is_not_a_pass(self):
        code = "import unittest; case=unittest.FunctionTestCase(unittest.skip('fixture')(lambda: None)); unittest.TextTestRunner().run(unittest.TestSuite([case]))"
        check = self.check("skipped-tests", code)
        check["require_tests"] = True
        self.assertEqual(self.execute([check]), 1)
        result = self.report()["checks"][0]
        self.assertEqual((result["status"], result["exit_code"]), ("failed", 0))
        self.assertEqual(result["tests_passed_or_run"], 0)

    def test_mixed_python_suite_counts_only_executed_tests(self):
        code = "import unittest; skipped=unittest.FunctionTestCase(unittest.skip('fixture')(lambda: None)); unittest.TextTestRunner().run(unittest.TestSuite([skipped,unittest.FunctionTestCase(lambda: None)]))"
        check = self.check("mixed-tests", code)
        check["require_tests"] = True
        self.assertEqual(self.execute([check]), 0)
        self.assertEqual(self.report()["checks"][0]["tests_passed_or_run"], 1)

    def test_failed_version_blocks_dependent_command(self):
        marker = self.root / "must-not-run"
        version = self.check("version", "print('wrong version')")
        version["expected_version"] = "required version"
        dependent = self.check("dependent", f"from pathlib import Path; Path({str(marker)!r}).touch()")
        dependent["depends_on"] = "version"
        self.assertEqual(self.execute([version, dependent]), 1)
        self.assertEqual([r["status"] for r in self.report()["checks"]], ["failed", "blocked"])
        self.assertFalse(marker.exists())

    def test_cargo_target_environment_is_preserved(self):
        target = str(self.root / "existing-target")
        env = dict(os.environ, CARGO_TARGET_DIR=target)
        check = self.check("environment", "import os; print(os.environ['CARGO_TARGET_DIR']); print(os.environ['PYTHONDONTWRITEBYTECODE'])")
        self.assertEqual(self.execute([check], env=env), 0)
        self.assertEqual(self.report()["cargo_target_dir"], target)
        self.assertEqual((self.out / "environment.log").read_text().splitlines(), [target, "1"])

    @unittest.skipUnless(shutil.which("rtk"), "rtk is optional")
    def test_rtk_proxy_retains_child_exit_status_and_log(self):
        code = self.runner.execute(self.root, self.out, [self.check("proxy", "print('proxy evidence'); raise SystemExit(9)")], proxy=shutil.which("rtk"))
        self.assertEqual(code, 1)
        result = self.report()["checks"][0]
        self.assertEqual(result["exit_code"], 9)
        self.assertEqual(result["executed_argv"][1], "proxy")
        self.assertIn("proxy evidence", (self.out / "proxy.log").read_text())

    def test_advisory_database_requires_its_own_git_repository(self):
        # A surrounding checkout's SHA must not count as retrieved advisories.
        self.init_git()
        db = self.root / "empty-db"
        db.mkdir()
        check = self.check("audit", "print('audit returned zero without retrieval')")
        check["advisory_db"] = str(db)
        self.assertEqual(self.execute([check]), 1)
        result = self.report()["checks"][0]
        self.assertFalse(result["fresh_advisory_db"])
        self.assertIsNone(result["advisory_db_sha"])

    @unittest.skipUnless(os.name == "posix", "process group interruption needs POSIX")
    def test_interruption_stops_child_and_cancels_remaining_checks(self):
        marker = self.root / "started"
        late = self.root / "late"
        checks = [self.check("wait", f"import os,time; from pathlib import Path; Path({str(marker)!r}).write_text(str(os.getpid())); time.sleep(30)"),
                  self.check("late", f"from pathlib import Path; Path({str(late)!r}).touch()")]
        code = ("import importlib.util,json; from pathlib import Path; "
                f"s=importlib.util.spec_from_file_location('runner',{str(SCRIPT)!r}); "
                "m=importlib.util.module_from_spec(s); s.loader.exec_module(m); "
                f"raise SystemExit(m.execute(Path({str(self.root)!r}),Path({str(self.out)!r}),json.loads({json.dumps(checks)!r}),proxy=None))")
        proc = subprocess.Popen([sys.executable, "-c", code], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: proc.kill() if proc.poll() is None else None)
        deadline = time.monotonic() + 5
        while not marker.exists() and proc.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(marker.exists(), "synthetic child did not start")
        proc.send_signal(signal.SIGINT)
        stdout, stderr = proc.communicate(timeout=5)
        self.assertEqual(proc.returncode, 130, (stdout, stderr))
        self.assertEqual([r["status"] for r in self.report()["checks"]], ["interrupted", "cancelled"])
        self.assertFalse(late.exists())
        with self.assertRaises(ProcessLookupError):
            os.kill(int(marker.read_text()), 0)


class GitTraceIsolationTests(unittest.TestCase):
    def test_fixture_git_does_not_emit_to_inherited_trace_sink_and_restores_tracing(self):
        with tempfile.TemporaryDirectory(prefix="check-local-trace-") as directory:
            root = Path(directory)
            sink = root / "events.jsonl"
            config = root / "gitconfig"
            # Model an inherited global collector without contacting the user's
            # daemon or editing any real Git configuration.
            config.write_text(f'[trace2]\n\teventTarget = "{sink}"\n')
            with mock.patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config), "GIT_CONFIG_NOSYSTEM": "1"}):
                os.environ.pop("GIT_TRACE2_EVENT", None)
                def ordinary_git():
                    subprocess.run(["git", "--version"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

                ordinary_git()
                before = sink.read_bytes()
                self.assertTrue(any(json.loads(line).get("event") == "start" for line in before.splitlines()),
                                "ordinary Git must actually reach the inherited trace sink")
                fixture = LocalCheckTests("test_stop_handles_child_that_already_exited")
                try:
                    fixture.setUp()
                    # Git in a runner child and its snapshots must inherit the
                    # same isolation as init/add/commit and fixture cleanups.
                    fixture.addCleanup(ordinary_git)
                    check = fixture.check("git-child", "import subprocess; subprocess.run(['git','status','--porcelain'],check=True)")
                    self.assertEqual(fixture.execute([check]), 0)
                finally:
                    self.assertTrue(fixture.doCleanups(), "fixture cleanup must still fail on real errors")
                self.assertEqual(sink.read_bytes(), before, "disposable repository operations escaped to the collector")
                ordinary_git()
                self.assertGreater(len(sink.read_bytes()), len(before), "ordinary Git tracing must resume after fixture cleanup")


if __name__ == "__main__":
    unittest.main()
