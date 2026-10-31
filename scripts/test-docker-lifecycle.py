#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Florian Bruniaux
# SPDX-License-Identifier: Apache-2.0
"""Tests for docker_lifecycle. Unit tests use an in-memory Docker and need no daemon.
The integration test at the bottom creates only resources labelled with a fresh
session id and skips when Docker is unavailable."""
import contextlib
import io
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import docker_lifecycle as dl  # noqa: E402


class FakeDocker(dl.Docker):
    def __init__(self):
        super().__init__()
        self.c, self.v, self.sizes, self.fail_rm, self.hook = {}, {}, {}, set(), None
        self.calls = []

    def available(self):
        return True

    def add_container(self, name, labels, running=False, mounts=(), cid=None):
        cid = cid or uuid.uuid4().hex
        self.c[cid] = {"Id": cid, "Name": "/" + name, "State": {"Running": running}, "Config": {"Labels": dict(labels)},
                       "Mounts": [{"Type": "volume", "Name": m} for m in mounts]}
        return cid

    def add_volume(self, name, labels, size=None):
        self.v[name] = {"Name": name, "Labels": dict(labels), "CreatedAt": "t0"}
        if size is not None:
            self.sizes[name] = size

    @staticmethod
    def _match(labels, label):
        if not label:
            return True
        k, _, val = label.partition("=")
        return labels.get(k) == val if val else k in labels

    def containers(self, label=None):
        return [json.loads(json.dumps(c)) for c in self.c.values() if self._match(c["Config"]["Labels"], label)]

    def volumes(self, label=None):
        return [dict(v) for v in self.v.values() if self._match(v["Labels"], label)]

    def _inspect(self, cmd, names, sub=None):
        # Real `docker inspect` / `docker volume inspect` exit 1 with "No such object" for an absent name.
        store = self.c if cmd == "inspect" else self.v
        out = []
        for n in names:
            if n not in store:
                raise dl.DockerError("docker inspect: Error: No such object: " + n)
            out.append(json.loads(json.dumps(store[n])))
        return out

    def names(self):
        return [c["Name"].lstrip("/") for c in self.c.values()], list(self.v)

    def stop_container(self, cid):
        self.c[cid]["State"]["Running"] = False

    def remove_container(self, cid):
        self.calls.append(("rm", cid))
        if self.hook:
            self.hook("rm", cid)
        if cid in self.fail_rm:
            raise dl.DockerError("daemon error")
        self.c.pop(cid, None)

    def remove_volume(self, name):
        self.calls.append(("volume rm", name))
        if self.hook:
            self.hook("volume rm", name)
        if name in self.fail_rm:
            raise dl.DockerError("daemon error")
        if any(name in dl._mounted_volumes(c) for c in self.c.values()):
            raise dl.DockerError("volume is in use")
        self.v.pop(name, None)

    def volume_sizes(self):
        return {n: self.sizes.get(n) for n in self.v}


def labels(project, run, cls):
    return {dl.L_PROJECT: project, dl.L_RUN: run, dl.L_CLASS: cls, dl.L_SCHEMA: "1"}


class Base(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.state = dl.State(self._tmp.name)
        self.d = FakeDocker()
        self.addCleanup(self._tmp.cleanup)

    def lease(self, run):
        fd = self.state.try_lock(self.state.run_path(run), True, create=True)
        self.assertIsInstance(fd, int)
        self.addCleanup(dl.release, fd)
        return fd

    def outcomes(self, r):
        return {e["name"]: (e["outcome"], e["reason"]) for e in r["entries"]}


class RunTeardown(Base):
    def test_labels_cover_project_run_class(self):
        run = dl.Run("p", docker=self.d, state=self.state, run_id="r1")
        self.assertEqual(run.labels(dl.SHARED_CACHE), labels("p", "r1", dl.SHARED_CACHE))
        self.assertIn(dl.L_CLASS + "=ephemeral", run.label_args())

    def test_success_removes_own_ephemeral_and_keeps_the_rest(self):
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
            cid = self.d.add_container("c1", run.labels(), mounts=["v1"])
            self.d.add_volume("v1", run.labels(), size=5)
            self.d.add_volume("cache", run.labels(dl.SHARED_CACHE), size=9)
            self.d.add_volume("keep", run.labels(dl.PERSISTENT))
            self.d.add_container("other", labels("p", "r2", dl.EPHEMERAL))
            self.d.add_volume("bare", {})
        self.assertNotIn(cid, self.d.c)
        self.assertEqual(set(self.d.v), {"cache", "keep", "bare"})
        o = self.outcomes(run.receipt)
        self.assertEqual(o["c1"][0], "removed")
        self.assertEqual(o["v1"][0], "removed")
        self.assertEqual(run.receipt["disk"]["removed_bytes_reported"], 5)
        self.assertTrue(Path(run.receipt["receipt_path"]).is_file())
        self.assertTrue(run.receipt["ok"])
        # r2 belongs to another run: not listed as ours, still present
        self.assertEqual(len(self.d.c), 1)

    def test_failed_body_still_tears_down_and_keeps_the_error(self):
        with self.assertRaises(ZeroDivisionError):
            with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
                self.d.add_container("c1", run.labels())
                1 / 0
        self.assertEqual(self.d.c, {})

    def test_evidence_is_exported_before_removal(self):
        seen = []
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
            self.d.add_container("c1", run.labels())
            run.add_evidence_export(lambda: seen.append(len(self.d.c)))
        self.assertEqual(seen, [1])
        self.assertEqual(self.d.c, {})

    def test_failed_export_retains_resources_and_raises(self):
        def boom():
            raise OSError("disk full")
        with self.assertRaises(dl.LifecycleError):
            with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
                self.d.add_container("c1", run.labels())
                run.add_evidence_export(boom)
        self.assertEqual(len(self.d.c), 1)
        self.assertIn("disk full", run.receipt["failures"][0]["reason"])

    def test_sigterm_is_handled_and_tears_down(self):
        with self.assertRaises(KeyboardInterrupt):
            with dl.Run("p", docker=self.d, state=self.state, run_id="r1") as run:
                self.d.add_container("c1", run.labels())
                os.kill(os.getpid(), signal.SIGTERM)
                time.sleep(1)
        self.assertEqual(self.d.c, {})
        self.assertIs(signal.getsignal(signal.SIGTERM), signal.SIG_DFL)

    def test_own_running_container_is_stopped_then_removed(self):
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
            self.d.add_container("c1", run.labels(), running=True)
        self.assertEqual(self.d.c, {})

    def test_remove_failure_is_reported_and_retry_is_idempotent(self):
        with self.assertRaises(dl.LifecycleError):
            with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
                cid = self.d.add_container("c1", run.labels())
                self.d.fail_rm.add(cid)
        self.assertEqual(self.outcomes(run.receipt)["c1"][0], "failed")
        self.assertIn(cid, self.d.c)
        self.d.fail_rm.clear()
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(r)["c1"][0], "removed")
        again = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(again["entries"], [])
        self.assertTrue(again["ok"])

    def test_second_run_with_same_id_is_refused(self):
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False):
            with self.assertRaises(dl.LifecycleError):
                dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False).__enter__()


class Reconcile(Base):
    def test_dead_owner_is_reaped_live_owner_is_not(self):
        self.lease("live")
        self.free("dead", create=True)
        self.d.add_container("live-c", labels("p", "live", dl.EPHEMERAL))
        self.d.add_container("dead-c", labels("p", "dead", dl.EPHEMERAL))
        self.d.add_volume("dead-v", labels("p", "dead", dl.EPHEMERAL), size=7)
        self.d.add_container("lone-c", labels("p", "never-registered", dl.EPHEMERAL))
        self.d.add_container("run-c", labels("p", "dead", dl.EPHEMERAL), running=True)
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        o = self.outcomes(r)
        self.assertEqual(o["live-c"], ("retained", "live owner: run lease is held"))
        self.assertEqual(o["dead-c"][0], "removed")
        self.assertEqual(o["dead-v"][0], "removed")
        self.assertEqual(o["run-c"], ("retained", "container is running"))
        self.assertIn("no lease record", o["lone-c"][1])
        self.assertEqual(sorted(c["Name"] for c in self.d.c.values()), ["/live-c", "/lone-c", "/run-c"])
        self.assertEqual(self.d.v, {})

    def free(self, run, create=True):
        """Leave a free lease file, as an owner that exited does."""
        fd = self.state.try_lock(self.state.run_path(run), True, create=create)
        if isinstance(fd, int):
            os.close(fd)

    def test_abrupt_launcher_death_then_reconcile(self):
        code = ("import sys,time;sys.path.insert(0,sys.argv[1]);import docker_lifecycle as dl;"
                "s=dl.State(sys.argv[2]);fd=s.try_lock(s.run_path('crashed'),True,create=True);"
                "print('held',flush=True);time.sleep(60)")
        child = subprocess.Popen([sys.executable, "-c", code, str(Path(__file__).parent), self._tmp.name],
                                 stdout=subprocess.PIPE, text=True)
        self.addCleanup(child.stdout.close)
        self.addCleanup(child.wait)
        self.addCleanup(child.kill)
        self.assertEqual(child.stdout.readline().strip(), "held")
        self.d.add_container("c", labels("p", "crashed", dl.EPHEMERAL))
        before = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(before)["c"][0], "retained")
        child.send_signal(signal.SIGKILL)
        child.wait()
        after = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(after)["c"][0], "removed")

    def test_protection_cases(self):
        self.free("old")
        self.d.add_container("persist", labels("p", "old", dl.PERSISTENT))
        self.d.add_container("cachey", labels("p", "old", dl.SHARED_CACHE))
        self.d.add_container("weird", {**labels("p", "old", "bogus")})
        self.d.add_container("foreign", labels("other", "old", dl.EPHEMERAL))
        self.d.add_container("../evil", labels("p", "../evil", dl.EPHEMERAL))
        self.d.add_container("ar-bare", {})
        self.d.add_volume("ar-bare-vol", {})
        self.d.add_volume("mounted", labels("p", "old", dl.EPHEMERAL))
        self.d.add_container("keeper", labels("p", "old", dl.PERSISTENT), mounts=["mounted"])
        r = dl.reconcile(self.d, self.state, "p", apply=True)
        o = self.outcomes(r)
        for n in ("persist", "cachey", "weird", "ar-bare", "ar-bare-vol", "mounted"):
            self.assertEqual(o[n][0], "retained", n)
        self.assertEqual(o["persist"][1], "persistent resource")
        self.assertIn("shared cache", o["cachey"][1])
        self.assertIn("unknown lifecycle class", o["weird"][1])
        self.assertIn("unlabelled", o["ar-bare"][1])
        self.assertIn("still mounted", o["mounted"][1])
        self.assertIn("unsafe run label", [e for e in r["entries"] if e["run"] == "../evil"][0]["reason"])
        self.assertNotIn("foreign", o)
        self.assertEqual(len(self.d.c), 7)
        self.assertEqual(len(self.d.v), 2)

    def test_preview_changes_nothing_and_writes_no_receipt(self):
        self.free("gone")
        self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        r = dl.reconcile(self.d, self.state, "p", apply=False, report_unlabelled=False)
        self.assertEqual(self.outcomes(r)["c"][0], "would-remove")
        self.assertEqual(len(self.d.c), 1)
        self.assertEqual([c for c in self.d.calls], [])
        self.assertFalse(self.state.receipts.exists())

    def test_identity_change_before_removal_keeps_the_new_resource(self):
        self.free("gone")
        cid = self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        orig_container = self.d.container

        def replaced(c):
            cur = orig_container(c)
            if cur is not None and not getattr(self, "swapped", False):
                self.swapped = True
                self.d.c[c]["Config"]["Labels"][dl.L_CLASS] = dl.PERSISTENT  # re-labelled by someone else
                return orig_container(c)
            return cur
        self.d.container = replaced
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(r)["c"], ("retained", "identity changed since listing"))
        self.assertIn(cid, self.d.c)

    def test_vanished_resource_is_absent_not_failure(self):
        self.free("gone")
        cid = self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        orig = self.d.container
        self.d.container = lambda c: (self.d.c.pop(cid, None) and None) or orig(c)
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(r)["c"][0], "absent")
        self.assertTrue(r["ok"])

    def test_reaper_holds_the_run_lease_while_removing(self):
        self.free("gone")
        self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        probes = []
        self.d.hook = lambda op, t: probes.append(self.state.try_lock(self.state.run_path("gone"), True))
        dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(probes, ["busy"])


class CacheBudget(Base):
    def cache(self, name, size, idle=0, held=None):
        self.d.add_volume(name, labels("p", "", dl.SHARED_CACHE), size=size)
        gate = self.state.gate_path(name)
        fd = self.state.try_lock(gate, False, create=True)
        os.close(fd)
        t = time.time() - idle
        os.utime(gate, (t, t))

    def test_lru_eviction_until_budget_fits(self):
        self.cache("old", 10, idle=300)
        self.cache("mid", 10, idle=200)
        self.cache("new", 10, idle=100)
        r = dl.evict(self.d, self.state, "p", 15, apply=True)
        self.assertEqual(set(self.d.v), {"new"})
        self.assertTrue(r["budget"]["met"])
        self.assertEqual(self.outcomes(r)["new"], ("retained", "within budget"))

    def test_active_consumer_blocks_eviction_and_is_reported(self):
        self.cache("busy", 10, idle=500)
        self.cache("free", 10, idle=100)
        run = dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False)
        run.acquire_cache("busy")
        self.addCleanup(run.release_cache, "busy")
        r = dl.evict(self.d, self.state, "p", 5, apply=True)
        self.assertIn("busy", self.d.v)
        self.assertNotIn("free", self.d.v)
        self.assertFalse(r["budget"]["met"])
        self.assertEqual(r["budget"]["active_caches"], ["busy"])
        self.assertIn("budget not met", r["budget"]["explanation"])

    def test_two_consumers_share_and_evictor_waits_for_both(self):
        self.cache("c", 10, idle=500)
        a = dl.Run("p", docker=self.d, state=self.state, run_id="ra", handle_signals=False)
        b = dl.Run("p", docker=self.d, state=self.state, run_id="rb", handle_signals=False)
        a.acquire_cache("c")
        b.acquire_cache("c")
        a.release_cache("c")
        self.assertEqual(self.outcomes(dl.evict(self.d, self.state, "p", 0, apply=True))["c"][0], "retained")
        b.release_cache("c")
        self.assertEqual(self.outcomes(dl.evict(self.d, self.state, "p", 0, apply=True))["c"][0], "removed")

    def test_acquire_during_eviction_is_refused_and_cache_is_removed_cleanly(self):
        self.cache("c", 10, idle=500)
        run = dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False)
        errs = []

        def hook(op, name):
            try:
                run.acquire_cache(name, wait=0)
            except dl.CacheBusy as e:
                errs.append(e)
        self.d.hook = hook
        r = dl.evict(self.d, self.state, "p", 0, apply=True)
        self.assertEqual(len(errs), 1)
        self.assertEqual(self.outcomes(r)["c"][0], "removed")

    def test_retained_reasons(self):
        self.cache("recent", 10, idle=1)
        self.cache("mounted", 10, idle=900)
        self.d.add_container("user", labels("p", "x", dl.EPHEMERAL), mounts=["mounted"])
        self.d.add_volume("unleased", labels("p", "", dl.SHARED_CACHE), size=10)
        self.d.add_volume("nosize", labels("p", "", dl.SHARED_CACHE))
        fd = self.state.try_lock(self.state.gate_path("nosize"), False, create=True)
        os.close(fd)
        r = dl.evict(self.d, self.state, "p", 0, apply=True, min_idle_seconds=60)
        o = self.outcomes(r)
        self.assertIn("below minimum retention", o["recent"][1])
        self.assertIn("referenced by container", o["mounted"][1])
        self.assertIn("no lease record", o["unleased"][1])
        self.assertIn("size unknown", o["nosize"][1])
        self.assertEqual(r["budget"]["met"], "UNKNOWN")
        self.assertEqual(len(self.d.v), 4)

    def test_preview_reports_without_removing(self):
        self.cache("old", 10, idle=500)
        r = dl.evict(self.d, self.state, "p", 0, apply=False)
        self.assertEqual(self.outcomes(r)["old"][0], "would-remove")
        self.assertIn("old", self.d.v)
        self.assertEqual(self.d.calls, [])

    def test_ephemeral_run_teardown_never_removes_a_cache(self):
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
            self.d.add_volume("cache", run.labels(dl.SHARED_CACHE), size=3)
            run.acquire_cache("cache")
        self.assertIn("cache", self.d.v)


class Cli(Base):
    def run_cli(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = dl.main([*argv, "--state-dir", self._tmp.name], docker=self.d)
            except SystemExit as e:
                code = e.code
        return code, out.getvalue(), err.getvalue()

    def test_preview_prints_json_and_is_read_only(self):
        self.d.add_container("c", labels("p", "x", dl.EPHEMERAL))
        code, out, _ = self.run_cli("preview", "--project", "p")
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out)["mode"], "preview")
        self.assertEqual(len(self.d.c), 1)

    def test_preview_refuses_apply(self):
        self.assertEqual(self.run_cli("preview", "--project", "p", "--apply")[0], 2)

    def test_exit_code_reflects_failed_cleanup(self):
        self.d.add_container("c", labels("p", "x", dl.EPHEMERAL), cid="cid1")
        os.close(self.state.try_lock(self.state.run_path("x"), True, create=True))
        self.d.fail_rm.add("cid1")
        self.assertEqual(self.run_cli("reconcile", "--project", "p", "--apply")[0], 1)

    def test_evict_needs_budget_and_parses_sizes(self):
        self.assertEqual(self.run_cli("evict-cache", "--project", "p")[0], 2)
        self.assertEqual(dl.parse_budget("20GB"), 20 * 10**9)
        self.assertEqual(dl.parse_size("12.56GB"), 12_560_000_000)
        self.assertIsNone(dl.parse_size("N/A"))

    def test_docker_unavailable_does_nothing(self):
        self.d.available = lambda: False
        self.assertEqual(self.run_cli("reconcile", "--project", "p", "--apply")[0], 2)


class ReviewFixes(Base):
    def test_ephemeral_without_run_label_is_never_removed(self):
        for i, run in enumerate((None, "")):
            lab = labels("p", run, dl.EPHEMERAL)
            if run is None:
                del lab[dl.L_RUN]
            self.d.add_container(f"norun{i}", lab)
            self.d.add_volume(f"norunv{i}", lab)
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(len(self.d.c), 2)
        self.assertEqual(len(self.d.v), 2)
        self.assertTrue(all(e["outcome"] == "retained" for e in r["entries"]))

    def test_incomplete_labels_or_other_schema_are_retained(self):
        self.free("old")
        no_schema = labels("p", "old", dl.EPHEMERAL)
        del no_schema[dl.L_SCHEMA]
        self.d.add_container("noschema", no_schema)
        self.d.add_container("schema2", {**labels("p", "old", dl.EPHEMERAL), dl.L_SCHEMA: "2"})
        self.d.add_volume("v-noschema", no_schema)
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(len(self.d.c), 2)
        self.assertEqual(len(self.d.v), 1)
        for e in r["entries"]:
            self.assertIn("incomplete lifecycle labels", e["reason"])

    def free(self, run):
        os.close(self.state.try_lock(self.state.run_path(run), True, create=True))

    def test_inspect_of_absent_object_is_absent_other_errors_propagate(self):
        self.assertIsNone(self.d.container("nope"))
        self.assertIsNone(self.d.volume("nope"))
        real = dl.Docker()
        real._run = lambda *a: (_ for _ in ()).throw(dl.DockerError("docker inspect: Error: No such container: x"))
        self.assertIsNone(real.container("x"))
        real._run = lambda *a: (_ for _ in ()).throw(dl.DockerError("docker inspect: permission denied"))
        with self.assertRaises(dl.DockerError):
            real.container("x")
        with self.assertRaises(dl.DockerError):
            real.volume("x")

    def test_verification_error_is_reported_as_failure(self):
        self.free("gone")
        cid = self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        calls = {"n": 0}
        orig = self.d._inspect

        def flaky(cmd, names, sub=None):
            calls["n"] += 1
            if calls["n"] == 2:  # the post-remove verification
                raise dl.DockerError("docker inspect: permission denied")
            return orig(cmd, names, sub)
        self.d._inspect = flaky
        r = dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        self.assertEqual(self.outcomes(r)["c"][0], "failed")
        self.assertFalse(r["ok"])

    def test_preview_takes_only_shared_run_probes(self):
        self.free("gone")
        self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        seen = []
        orig = self.state.try_lock
        self.state.try_lock = lambda path, exclusive, create=False: (seen.append(exclusive), orig(path, exclusive, create))[1]
        dl.reconcile(self.d, self.state, "p", apply=False, report_unlabelled=False)
        self.assertEqual(seen, [False])

    def test_acquire_cache_waits_briefly_for_a_finishing_eviction(self):
        import threading
        run = dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False)
        gate = self.state.gate_path("c")
        ex = self.state.try_lock(gate, True, create=True)
        with self.assertRaises(dl.CacheBusy):
            run.acquire_cache("c", wait=0)
        threading.Timer(0.2, dl.release, [ex]).start()
        run.acquire_cache("c", wait=3)
        run.release_cache("c")

    def test_cli_rejects_unsafe_project(self):
        code = Cli.run_cli(self, "preview", "--project", "../x")
        self.assertEqual(code[0], 2)

    def test_evict_ignores_cache_of_another_project(self):
        self.d.add_volume("foreign-cache", labels("other", "r", dl.SHARED_CACHE), size=9)
        gate = self.state.gate_path("foreign-cache")
        os.close(self.state.try_lock(gate, False, create=True))
        self.d.volumes = lambda label=None: [dict(v) for v in self.d.v.values()]
        r = dl.evict(self.d, self.state, "p", 0, apply=True)
        self.assertIn("foreign-cache", self.d.v)
        self.assertEqual(r["entries"], [])

    def test_lease_files_refuse_symlinks_and_unsafe_directories(self):
        runs = self.state.root / "runs"
        runs.mkdir(parents=True, mode=0o700)
        target = Path(self._tmp.name) / "victim"
        target.write_text("x")
        os.symlink(target, runs / "evil.lock")
        with self.assertRaises(dl.LifecycleError):
            self.state.try_lock(self.state.run_path("evil"), True, create=True)
        self.assertEqual(target.read_text(), "x")
        os.chmod(runs, 0o777)
        with self.assertRaises(dl.LifecycleError):
            self.state.try_lock(self.state.run_path("other"), True, create=True)

    def test_state_directories_are_private(self):
        os.close(self.state.try_lock(self.state.run_path("a"), True, create=True))
        self.assertEqual(os.stat(self.state.root / "runs").st_mode & 0o777, 0o700)

    def test_sigterm_during_teardown_does_not_interrupt_it(self):
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1") as run:
            self.d.add_container("c1", run.labels())
            run.add_evidence_export(lambda: os.kill(os.getpid(), signal.SIGTERM))
        self.assertEqual(self.d.c, {})

    def test_intent_receipt_exists_before_removal_and_is_completed(self):
        seen = []

        def hook(op, target):
            files = sorted(self.state.receipts.glob("*.json"))
            seen.append([(json.loads(f.read_text())["status"], [e["outcome"] for e in json.loads(f.read_text())["entries"]]) for f in files])
        self.d.hook = hook
        with dl.Run("p", docker=self.d, state=self.state, run_id="r1", handle_signals=False) as run:
            self.d.add_container("c1", run.labels())
        self.assertEqual(seen[0], [("started", ["would-remove"])])
        files = list(self.state.receipts.glob("*.json"))
        self.assertEqual(len(files), 1)
        self.assertEqual(json.loads(files[0].read_text())["status"], "complete")
        self.assertEqual(run.receipt["receipt_path"], str(files[0]))

    def test_intent_receipt_for_reconcile_and_evict(self):
        self.free("gone")
        self.d.add_container("c", labels("p", "gone", dl.EPHEMERAL))
        self.d.add_volume("cache", labels("p", "r", dl.SHARED_CACHE), size=5)
        os.close(self.state.try_lock(self.state.gate_path("cache"), False, create=True))
        statuses = []
        self.d.hook = lambda op, t: statuses.append(sorted(json.loads(f.read_text())["status"] for f in self.state.receipts.glob("*.json")))
        dl.reconcile(self.d, self.state, "p", apply=True, report_unlabelled=False)
        dl.evict(self.d, self.state, "p", 0, apply=True)
        self.assertEqual(statuses, [["started"], ["complete", "started"]])

    def test_volume_sizes_parses_observed_df_format_and_tolerates_other_output(self):
        d = dl.Docker()
        d._run = lambda *a: json.dumps({"Volumes": [{"Name": "v", "Size": "12.56GB"}, {"Name": "w", "Size": "N/A"}]})
        self.assertEqual(d.volume_sizes(), {"v": 12_560_000_000, "w": None})
        d._run = lambda *a: "not json"
        self.assertEqual(d.volume_sizes(), {})


@unittest.skipUnless(dl.Docker().available(), "docker daemon not reachable")
class RealDocker(unittest.TestCase):
    """Creates only resources labelled with a fresh session id; removal code is pointed
    at that session's project label and nothing else."""

    def setUp(self):
        self.sid = uuid.uuid4().hex[:10]
        self.project = "artest-" + self.sid
        self.d = dl.Docker()
        self.tmp = tempfile.TemporaryDirectory()
        self.state = dl.State(self.tmp.name)
        self.image = next((i for i in ("bash:5", "python:3.11-slim-bookworm", "debian:bookworm-slim")
                           if subprocess.run(["docker", "image", "inspect", i], capture_output=True).returncode == 0), None)
        if not self.image:
            self.skipTest("no small local image; refusing to pull")
        self.created = []

    def tearDown(self):
        for kind, name in reversed(self.created):  # exact names created by this test only
            subprocess.run(["docker", kind, "rm", name], capture_output=True)
        self.tmp.cleanup()

    def docker(self, *a):
        return subprocess.run(["docker", *a], capture_output=True, text=True, check=True).stdout.strip()

    def test_teardown_removes_own_and_keeps_persistent(self):
        keep = self.project + "-keep"
        with dl.Run(self.project, docker=self.d, state=self.state, handle_signals=False) as run:
            vol, cname = run.name("vol"), run.name("c")
            self.docker("volume", "create", *run.label_args(), vol)
            self.created.append(("volume", vol))
            cid = self.docker("create", *run.label_args(), "--name", cname, "-v", vol + ":/w", self.image, "true")
            self.created.append(("container", cname))
            self.docker("volume", "create", *run.label_args(dl.PERSISTENT), keep)
            self.created.append(("volume", keep))
        o = {e["name"]: e["outcome"] for e in run.receipt["entries"]}
        self.assertEqual(o[cname], "removed")
        self.assertEqual(o[vol], "removed")
        self.assertIsNone(self.d.container(cid))
        self.assertIsNone(self.d.volume(vol))
        self.assertIsNotNone(self.d.volume(keep))
        self.assertEqual(self.docker("volume", "ls", "-q", "--filter", "label=" + dl.L_PROJECT + "=" + self.project), keep)

    def test_real_docker_refuses_volume_in_use(self):
        run = dl.Run(self.project, docker=self.d, state=self.state, handle_signals=False)
        vol = run.name("vol")
        self.docker("volume", "create", *run.label_args(dl.SHARED_CACHE), vol)
        self.created.append(("volume", vol))
        fd = self.state.try_lock(self.state.gate_path(vol), False, create=True)
        os.close(fd)
        cname = run.name("user")
        self.docker("create", "--name", cname, "-v", vol + ":/w", self.image, "true")  # unlabelled consumer
        self.created.append(("container", cname))
        self.assertTrue(dl.evict(self.d, self.state, self.project, 0, apply=True)["ok"])
        self.assertIsNotNone(self.d.volume(vol))


if __name__ == "__main__":
    unittest.main()
