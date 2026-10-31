#!/usr/bin/env python3
"""Docker resource lifecycle for qualification launchers (issue #212).

Managed containers and volumes carry lifecycle labels. A launcher holds an
flock-based lease for its whole life; the OS releases it on any exit, including
SIGKILL, so a free lease proves the owner is gone. Nothing here runs a global
prune: every removal targets one resource that was re-inspected just before.
Standard library only. POSIX only (fcntl). See docs/docker-resource-lifecycle.md.
"""
import argparse
import dataclasses
import datetime
import fcntl
import json
import os
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

LABEL = "org.agent-router.lifecycle."
L_PROJECT, L_RUN, L_CLASS, L_SCHEMA = (LABEL + k for k in ("project", "run", "class", "schema"))
SCHEMA = "1"
EPHEMERAL, SHARED_CACHE, PERSISTENT = "ephemeral", "shared-cache", "persistent"
CLASSES = (EPHEMERAL, SHARED_CACHE, PERSISTENT)
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$")
UNLABELLED_NAME_RE = re.compile(r"^(ar-|agent-router)")
RACE_NOTE = (
    "Remaining race window: a process that neither uses this module nor creates a "
    "Docker container referencing the resource can acquire it between the final "
    "inspect and the remove call. Docker itself refuses to remove a volume that any "
    "container references, which closes the case of an already-created container."
)


class LifecycleError(Exception):
    pass


class CacheBusy(LifecycleError):
    """The shared cache is being evicted right now."""


class DockerError(LifecycleError):
    pass


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")


def parse_size(text):
    """Docker prints rounded sizes such as '12.56GB'. None means unknown."""
    m = re.fullmatch(r"\s*([0-9.]+)\s*([kKMGT]?B)\s*", str(text or ""))
    if not m:
        return None
    unit = {"B": 1, "KB": 10**3, "MB": 10**6, "GB": 10**9, "TB": 10**12}[m.group(2).upper()]
    return int(float(m.group(1)) * unit)


# ---------------------------------------------------------------- Docker client

class Docker:
    """Thin read/remove wrapper. Every call targets named resources, never a filter sweep."""

    def __init__(self, binary="docker", timeout=60):
        self.binary, self.timeout = binary, timeout

    def _run(self, *args):
        try:
            p = subprocess.run([self.binary, *args], capture_output=True, text=True, timeout=self.timeout)
        except (OSError, subprocess.TimeoutExpired) as e:
            raise DockerError(f"docker {args[0]}: {e}") from e
        if p.returncode:
            raise DockerError(f"docker {' '.join(args[:2])}: {p.stderr.strip() or p.returncode}")
        return p.stdout

    def available(self):
        try:
            self._run("info", "--format", "{{.ServerVersion}}")
            return True
        except DockerError:
            return False

    def containers(self, label=None):
        args = ["ps", "-a", "-q", "--no-trunc"] + (["--filter", "label=" + label] if label else [])
        ids = self._run(*args).split()
        return self._inspect("inspect", ids)

    def volumes(self, label=None):
        args = ["volume", "ls", "-q"] + (["--filter", "label=" + label] if label else [])
        return self._inspect("volume", self._run(*args).split(), sub="inspect")

    def _inspect(self, cmd, names, sub=None):
        if not names:
            return []
        argv = [cmd, sub, *names] if sub else [cmd, *names]
        return json.loads(self._run(*argv))

    def _inspect_one(self, cmd, name, sub=None):
        try:
            r = self._inspect(cmd, [name], sub=sub)
        except DockerError as e:
            if "no such" in str(e).lower():  # docker inspect of an absent object exits 1 with "No such ..."
                return None
            raise
        return r[0] if r else None

    def container(self, cid):
        return self._inspect_one("inspect", cid)

    def volume(self, name):
        return self._inspect_one("volume", name, sub="inspect")

    def names(self):
        c = [l.split(" ", 1)[1] for l in self._run("ps", "-a", "--format", "{{.ID}} {{.Names}}").splitlines()]
        return c, self._run("volume", "ls", "--format", "{{.Name}}").split()

    def stop_container(self, cid):
        self._run("stop", "--time", "10", cid)

    def remove_container(self, cid):
        self._run("rm", cid)

    def remove_volume(self, name):
        self._run("volume", "rm", name)

    def volume_sizes(self):
        """name -> bytes or None, from `system df -v` (rounded; Docker allocation, not host free space)."""
        try:
            data = json.loads(self._run("system", "df", "-v", "--format", "json"))
        except (DockerError, ValueError):
            return {}
        return {v["Name"]: parse_size(v.get("Size")) for v in data.get("Volumes") or []}


# ------------------------------------------------------------------ lease state

class State:
    """Lease files. runs/<run>.lock is held LOCK_EX by a live run; caches/<name>.gate
    is held LOCK_SH by each live consumer and LOCK_EX by an evictor."""

    def __init__(self, root=None):
        env = os.environ.get("AGENT_ROUTER_DOCKER_STATE")
        self.root = Path(root or env or Path.home() / ".local/state/agent-router/docker-lifecycle")

    @property
    def receipts(self):
        return self.root / "receipts"

    def _path(self, kind, name, suffix):
        if not NAME_RE.match(name or ""):
            raise LifecycleError(f"unsafe {kind} name: {name!r}")
        return self.root / f"{kind}s" / (name + suffix)

    def run_path(self, run):
        return self._path("run", run, ".lock")

    def gate_path(self, cache):
        return self._path("cache", cache, ".gate")

    @staticmethod
    def _private_dir(path):
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        st = os.lstat(path)
        if not stat.S_ISDIR(st.st_mode) or st.st_uid != os.geteuid() or st.st_mode & 0o022:
            raise LifecycleError(f"state directory is not private to this user: {path}")

    def open_lease(self, path, create):
        if create:
            self._private_dir(self.root)
            self._private_dir(path.parent)
        try:
            fd = os.open(path, os.O_RDWR | os.O_NOFOLLOW | (os.O_CREAT if create else 0), 0o600)
        except FileNotFoundError:
            raise
        except OSError as e:
            raise LifecycleError(f"cannot open lease {path}: {e}") from e
        if os.fstat(fd).st_uid != os.geteuid():
            os.close(fd)
            raise LifecycleError(f"lease file not owned by this user: {path}")
        return fd

    def try_lock(self, path, exclusive, create=False):
        """Return an fd holding the lock, 'busy' when held elsewhere, or 'missing'."""
        try:
            fd = self.open_lease(path, create)
        except FileNotFoundError:
            return "missing"
        try:
            fcntl.flock(fd, (fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH) | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            return "busy"
        return fd


def release(fd):
    if isinstance(fd, int):
        os.close(fd)


# -------------------------------------------------------------------- receipts

@dataclasses.dataclass
class Entry:
    kind: str
    name: str
    id: str
    cls: str
    run: str
    outcome: str  # removed | would-remove | retained | failed | absent
    reason: str
    bytes: int = None

    def as_dict(self):
        d = dataclasses.asdict(self)
        d["class"] = d.pop("cls")
        return d


def write_receipt(state, receipt, path=None):
    """Write atomically. Pass the earlier path to complete an intent receipt in place."""
    State._private_dir(state.root)
    State._private_dir(state.receipts)
    path = path or state.receipts / f"{receipt['created_at'].replace(':', '')}-{receipt['mode']}-{uuid.uuid4().hex[:8]}.json"
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    os.replace(tmp, path)
    receipt["receipt_path"] = str(path)
    return path


def new_receipt(mode, project, state, docker, **extra):
    host = shutil.disk_usage(state.root if state.root.exists() else Path.home())
    return {"schema": "agent-router.docker-lifecycle.receipt/1", "created_at": now(), "mode": mode,
            "project": project, "entries": [], "disk": {"host_free_bytes_before": host.free,
            "note": "Docker sizes are rounded allocations, not host free space recovered."},
            "race_note": RACE_NOTE, **extra}


def finish_receipt(receipt, state, docker, sizes_before):
    sizes_after = docker.volume_sizes()
    host = shutil.disk_usage(state.root if state.root.exists() else Path.home())
    receipt["disk"]["host_free_bytes_after"] = host.free
    receipt["disk"]["docker_volume_bytes_before"] = sum(v for v in sizes_before.values() if v)
    receipt["disk"]["docker_volume_bytes_after"] = sum(v for v in sizes_after.values() if v)
    receipt["disk"]["removed_bytes_reported"] = sum(e["bytes"] or 0 for e in receipt["entries"] if e["outcome"] == "removed")
    receipt["failures"] = [e for e in receipt["entries"] if e["outcome"] == "failed"]
    receipt["ok"] = not receipt["failures"]
    return receipt


# ------------------------------------------------------------ ephemeral cleanup

def _labels(obj):
    return (obj.get("Config", {}).get("Labels") if "Config" in obj else obj.get("Labels")) or {}


def _mounted_volumes(container):
    return {m["Name"] for m in container.get("Mounts", []) if m.get("Type") == "volume"}


def reap_ephemeral(docker, state, project, apply, own_run=None, run_filter=None, stop_own=False, sizes=None):
    """Plan or perform removal of ephemeral containers then volumes of `project`.

    A resource is removed only when its class is ephemeral, its run lease is free
    (owner dead) or it belongs to `own_run`, and identity is unchanged at the last
    inspect. Anything else is retained with a reason."""
    sizes = sizes if sizes is not None else {}
    out, held = [], {}
    sel = L_PROJECT + "=" + project

    def owner_state(run):
        """'ours', 'dead' (lease free), or a retain reason."""
        if own_run is not None and run == own_run:
            return "ours"
        if not NAME_RE.match(run or ""):
            return "ambiguous: missing or unsafe run label"
        if run not in held:
            held[run] = state.try_lock(state.run_path(run), apply)  # preview: shared probe, never blocks a launcher
        h = held[run]
        if h == "missing":
            return "ambiguous: no lease record for this run in the state directory"
        if h == "busy":
            return "live owner: run lease is held"
        return "dead"

    def classify(res, kind, name, ident):
        labels = _labels(res)
        cls, run = labels.get(L_CLASS), labels.get(L_RUN)
        base = dict(kind=kind, name=name, id=ident, cls=cls or "", run=run or "")
        if labels.get(L_PROJECT) != project:
            return Entry(**base, outcome="retained", reason="foreign project label"), None
        if cls == PERSISTENT:
            return Entry(**base, outcome="retained", reason="persistent resource"), None
        if cls == SHARED_CACHE:
            return Entry(**base, outcome="retained", reason="shared cache: only the eviction command may remove it"), None
        if cls != EPHEMERAL:
            return Entry(**base, outcome="retained", reason="ambiguous: unknown lifecycle class"), None
        if any(not labels.get(k) for k in (L_PROJECT, L_RUN, L_CLASS, L_SCHEMA)) or labels[L_SCHEMA] != SCHEMA:
            return Entry(**base, outcome="retained", reason="ambiguous: incomplete lifecycle labels or unsupported schema"), None
        if run_filter and run != run_filter:
            return None, None
        st = owner_state(run)
        if st not in ("ours", "dead"):
            return Entry(**base, outcome="retained", reason=st), None
        return Entry(**base, outcome="would-remove", reason="ephemeral resource, owner " + ("is this run" if st == "ours" else "is dead")), labels

    try:
        containers = docker.containers(sel)
        remaining_mounts = {}
        for c in containers:
            e, labels = classify(c, "container", c["Name"].lstrip("/"), c["Id"])
            if e is None:
                continue
            if labels is not None and c["State"].get("Running"):
                if stop_own and own_run is not None and labels.get(L_RUN) == own_run:
                    if apply:
                        try:
                            docker.stop_container(c["Id"])
                        except DockerError as ex:
                            e.outcome, e.reason = "failed", str(ex)
                else:
                    e.outcome, e.reason, labels = "retained", "container is running", None
            if e.outcome == "would-remove" and apply:
                _remove_container(docker, e, c, labels)
            if e.outcome in ("retained", "failed"):
                for v in _mounted_volumes(c):
                    remaining_mounts.setdefault(v, []).append(e.name)
            out.append(e)
        for v in docker.volumes(sel):
            e, labels = classify(v, "volume", v["Name"], v["Name"])
            if e is None:
                continue
            e.bytes = sizes.get(v["Name"])
            if e.outcome == "would-remove" and v["Name"] in remaining_mounts:
                e.outcome, e.reason = "retained", "still mounted by retained container " + remaining_mounts[v["Name"]][0]
            if e.outcome == "would-remove" and apply:
                _remove_volume(docker, e, v, labels)
            out.append(e)
    finally:
        for h in held.values():
            release(h)
    return out


def _same_identity(before, now_obj, kind):
    if kind == "container":
        same = before["Id"] == now_obj["Id"]
    else:
        same = before["Name"] == now_obj["Name"] and before.get("CreatedAt") == now_obj.get("CreatedAt")
    return same and _labels(before) == _labels(now_obj)


def _remove_container(docker, e, snapshot, labels):
    try:
        cur = docker.container(snapshot["Id"])
        if cur is None:
            e.outcome, e.reason = "absent", "already removed"
            return
        if not _same_identity(snapshot, cur, "container"):
            e.outcome, e.reason = "retained", "identity changed since listing"
            return
        if cur["State"].get("Running"):
            e.outcome, e.reason = "retained", "container is running"
            return
        docker.remove_container(cur["Id"])
        if docker.container(snapshot["Id"]) is not None:
            e.outcome, e.reason = "failed", "still present after remove"
        else:
            e.outcome, e.reason = "removed", "removed and verified absent"
    except DockerError as ex:
        e.outcome, e.reason = "failed", str(ex)


def _remove_volume(docker, e, snapshot, labels):
    try:
        cur = docker.volume(snapshot["Name"])
        if cur is None:
            e.outcome, e.reason = "absent", "already removed"
            return
        if not _same_identity(snapshot, cur, "volume"):
            e.outcome, e.reason = "retained", "identity changed since listing"
            return
        docker.remove_volume(snapshot["Name"])
        if docker.volume(snapshot["Name"]) is not None:
            e.outcome, e.reason = "failed", "still present after remove"
        else:
            e.outcome, e.reason = "removed", "removed and verified absent"
    except DockerError as ex:
        e.outcome, e.reason = "failed", str(ex)


def unlabelled_report(docker, project):
    """Read-only: names that look like Agent Router resources but carry no lifecycle labels."""
    cn, vn = docker.names()
    managed_c = {c["Name"].lstrip("/") for c in docker.containers(L_PROJECT)}
    managed_v = {v["Name"] for v in docker.volumes(L_PROJECT)}
    out = [Entry("container", n, "", "", "", "retained", "unlabelled: no lifecycle metadata, never removed")
           for n in cn if UNLABELLED_NAME_RE.match(n) and n not in managed_c]
    out += [Entry("volume", n, n, "", "", "retained", "unlabelled: no lifecycle metadata, never removed")
            for n in vn if UNLABELLED_NAME_RE.match(n) and n not in managed_v]
    return out


def reconcile(docker, state, project, apply=False, run_filter=None, own_run=None, report_unlabelled=True):
    sizes = docker.volume_sizes()
    r = new_receipt("apply" if apply else "preview", project, state, docker, operation="reconcile")
    path = None
    if apply:
        path = _write_intent(state, r, reap_ephemeral(docker, state, project, False, own_run=own_run,
                                                      run_filter=run_filter, sizes=sizes))
    entries = reap_ephemeral(docker, state, project, apply, own_run=own_run, run_filter=run_filter, sizes=sizes)
    if report_unlabelled:
        entries += unlabelled_report(docker, project)
    r["entries"] = [e.as_dict() for e in entries]
    return _persist(r, state, docker, sizes, apply, path)


def _persist(r, state, docker, sizes, apply, path=None):
    finish_receipt(r, state, docker, sizes)
    r["status"] = "complete" if apply else "preview"
    if apply:
        write_receipt(state, r, path)
    return r


def _write_intent(state, receipt, planned_entries, **extra):
    """Persist the planned removals before the first one runs, so a trace survives a crash."""
    receipt.update(status="started", entries=[e.as_dict() if hasattr(e, "as_dict") else e for e in planned_entries], **extra)
    return write_receipt(state, receipt)


# ----------------------------------------------------------------- cache budget

def evict(docker, state, project, budget_bytes, apply=False, min_idle_seconds=0, clock=time.time):
    """Evict least-recently-used unused shared caches until the project total fits the budget."""
    sizes = docker.volume_sizes()
    r = new_receipt("apply" if apply else "preview", project, state, docker, operation="evict-cache", budget_bytes=budget_bytes)
    caches = [v for v in docker.volumes(L_PROJECT + "=" + project)
              if _labels(v).get(L_PROJECT) == project and _labels(v).get(L_SCHEMA) == SCHEMA
              and _labels(v).get(L_CLASS) == SHARED_CACHE]
    path = None
    if apply:
        plan = evict(docker, state, project, budget_bytes, False, min_idle_seconds, clock)
        path = _write_intent(state, r, plan["entries"], budget=plan["budget"])
    mounts = {}
    for c in docker.containers():
        for m in _mounted_volumes(c):
            mounts.setdefault(m, set()).add(c["Name"].lstrip("/"))
    entries, candidates, active, unknown = [], [], [], False
    total = 0
    for v in caches:
        name, size = v["Name"], sizes.get(v["Name"])
        base = dict(kind="volume", name=name, id=name, cls=SHARED_CACHE, run="", bytes=size)
        if size is None:
            unknown = True
        else:
            total += size
        if not NAME_RE.match(name):
            entries.append(Entry(**base, outcome="retained", reason="ambiguous: unsafe cache name"))
            continue
        gate = state.gate_path(name)
        probe = state.try_lock(gate, True)
        if probe == "missing":
            entries.append(Entry(**base, outcome="retained", reason="ambiguous: no lease record, cannot prove the cache is unused"))
        elif probe == "busy":
            active.append(name)
            entries.append(Entry(**base, outcome="retained", reason="in use: a consumer holds the cache lease"))
        else:
            release(probe)
            idle = clock() - gate.stat().st_mtime
            if mounts.get(name):
                entries.append(Entry(**base, outcome="retained", reason="referenced by container " + sorted(mounts[name])[0]))
            elif size is None:
                entries.append(Entry(**base, outcome="retained", reason="size unknown: eviction would not be measurable"))
            elif idle < min_idle_seconds:
                entries.append(Entry(**base, outcome="retained", reason=f"idle {int(idle)}s below minimum retention {min_idle_seconds}s"))
            else:
                candidates.append((gate.stat().st_mtime, v, base))
    candidates.sort(key=lambda t: t[0])
    for _, v, base in candidates:
        if total <= budget_bytes:
            entries.append(Entry(**base, outcome="retained", reason="within budget"))
            continue
        e = Entry(**base, outcome="would-remove", reason="least recently used unused cache over budget")
        if apply:
            _evict_one(docker, state, e, v, project)
        if e.outcome in ("removed", "would-remove"):
            total -= base["bytes"]
        entries.append(e)
    r["entries"] = [e.as_dict() for e in entries]
    r["budget"] = {"limit_bytes": budget_bytes, "projected_total_bytes": total, "active_caches": sorted(active),
                   "met": "UNKNOWN" if unknown else total <= budget_bytes}
    if not unknown and total > budget_bytes:
        r["budget"]["explanation"] = "budget not met: remaining entries are active, referenced, recent, ambiguous or failed"
    return _persist(r, state, docker, sizes, apply, path)


def _evict_one(docker, state, e, snapshot, project):
    # Exclusive gate lock held through identity recheck and removal: a consumer's
    # shared non-blocking acquire fails with CacheBusy until we are done.
    fd = state.try_lock(state.gate_path(e.name), True)
    if not isinstance(fd, int):
        e.outcome, e.reason = "retained", "consumer acquired the cache before removal"
        return
    try:
        if _labels(snapshot).get(L_PROJECT) != project:
            e.outcome, e.reason = "retained", "project label changed"
            return
        if any(e.name in _mounted_volumes(c) for c in docker.containers()):
            e.outcome, e.reason = "retained", "a container references the cache"
            return
        _remove_volume(docker, e, snapshot, _labels(snapshot))
    finally:
        release(fd)


# --------------------------------------------------------------- launcher API

class Run:
    """Context manager for one launcher run.

    with Run("ar-qual") as run:
        argv = ["docker", "run", "-d", *run.label_args(), "--name", run.name("c1"), ...]
        cache = run.acquire_cache("ar-build-cache")   # holds a shared lease
        run.add_evidence_export(lambda: export_logs())
    On exit, evidence is exported first; only then are this run's ephemeral
    containers and volumes removed and verified absent, and a receipt is written."""

    def __init__(self, project, docker=None, state=None, run_id=None, handle_signals=True):
        self.project = project
        self.docker = docker or Docker()
        self.state = state or State()
        self.run_id = run_id or "r" + uuid.uuid4().hex[:12]
        if not NAME_RE.match(self.run_id) or not NAME_RE.match(project):
            raise LifecycleError("unsafe project or run id")
        self._fd, self._gates, self._exports, self._old = None, {}, [], {}
        self._signals = handle_signals
        self.receipt = None

    def __enter__(self):
        fd = self.state.try_lock(self.state.run_path(self.run_id), True, create=True)
        if not isinstance(fd, int):
            raise LifecycleError("run lease is already held: " + self.run_id)
        self._fd = fd
        if self._signals and threading_main():
            for s in (signal.SIGTERM, signal.SIGHUP):
                self._old[s] = signal.signal(s, _raise_interrupt)
        return self

    def __exit__(self, exc_type, exc, tb):
        for s in self._old:  # a second SIGTERM must not abort the removal half-way
            signal.signal(s, signal.SIG_IGN)
        try:
            failures = self.teardown()
        finally:
            for s, h in self._old.items():
                signal.signal(s, h)
            for cache in list(self._gates):
                self.release_cache(cache)
            release(self._fd)
        if failures and exc_type is None:
            raise LifecycleError(f"teardown left {len(failures)} resource(s); see {self.receipt.get('receipt_path')}")
        return False

    def labels(self, cls=EPHEMERAL):
        if cls not in CLASSES:
            raise LifecycleError("unknown class " + cls)
        return {L_PROJECT: self.project, L_RUN: self.run_id, L_CLASS: cls, L_SCHEMA: SCHEMA}

    def label_args(self, cls=EPHEMERAL):
        return [a for k, v in self.labels(cls).items() for a in ("--label", f"{k}={v}")]

    def name(self, suffix):
        return f"{self.project}-{self.run_id}-{suffix}"

    def acquire_cache(self, cache, wait=1.0):
        """Register as a consumer of a shared cache. Waits up to `wait` seconds for an
        eviction or an evictor probe to finish, then raises CacheBusy."""
        path = self.state.gate_path(cache)
        deadline = time.monotonic() + wait
        while True:
            fd = self.state.try_lock(path, False, create=True)
            if isinstance(fd, int):
                break
            if time.monotonic() >= deadline:
                raise CacheBusy(f"cache {cache} is being evicted")
            time.sleep(0.05)
        os.utime(path)
        self._gates[cache] = (fd, path)
        return cache

    def release_cache(self, cache):
        fd, path = self._gates.pop(cache)
        os.utime(path)
        release(fd)

    def add_evidence_export(self, fn):
        self._exports.append(fn)

    def teardown(self):
        """Export evidence, then remove this run's ephemeral resources. Returns failed entries."""
        sizes = self.docker.volume_sizes()
        r = new_receipt("apply", self.project, self.state, self.docker, operation="teardown", run=self.run_id)
        export_errors = []
        for fn in self._exports:
            try:
                fn()
            except Exception as ex:  # evidence export failed: keep the resources
                export_errors.append(f"{type(ex).__name__}: {ex}")
        path = None
        if not export_errors:
            path = _write_intent(self.state, r, reap_ephemeral(self.docker, self.state, self.project, False,
                                 own_run=self.run_id, run_filter=self.run_id, stop_own=True, sizes=sizes))
        if export_errors:
            entries = [Entry("run", self.run_id, self.run_id, EPHEMERAL, self.run_id, "failed",
                             "evidence export failed, resources retained: " + "; ".join(export_errors))]
        else:
            entries = reap_ephemeral(self.docker, self.state, self.project, True, own_run=self.run_id,
                                     run_filter=self.run_id, stop_own=True, sizes=sizes)
        r["entries"] = [e.as_dict() for e in entries]
        self.receipt = _persist(r, self.state, self.docker, sizes, True, path)
        return self.receipt["failures"]


def _raise_interrupt(signum, frame):
    raise KeyboardInterrupt(f"signal {signum}")


def threading_main():
    import threading
    return threading.current_thread() is threading.main_thread()


# ------------------------------------------------------------------------ CLI

def parse_budget(text):
    n = parse_size(text) if re.search(r"[A-Za-z]", text) else int(text)
    if n is None:
        raise argparse.ArgumentTypeError("budget must be bytes or a size like 20GB")
    return n


def main(argv=None, docker=None):
    p = argparse.ArgumentParser(description="Agent Router Docker resource lifecycle. Read-only unless --apply.")
    p.add_argument("command", choices=("preview", "reconcile", "evict-cache"))
    p.add_argument("--project", required=True)
    p.add_argument("--state-dir")
    p.add_argument("--run", help="limit reconcile to one run id")
    p.add_argument("--apply", action="store_true", help="perform removals (reconcile, evict-cache)")
    p.add_argument("--budget", type=parse_budget, help="shared-cache budget, bytes or e.g. 20GB (evict-cache)")
    p.add_argument("--min-idle-seconds", type=int, default=0)
    a = p.parse_args(argv)
    if not NAME_RE.match(a.project):
        p.error("--project must match " + NAME_RE.pattern)
    docker = docker or Docker()
    if not docker.available():
        print("docker unavailable: nothing inspected, nothing removed", file=sys.stderr)
        return 2
    state = State(a.state_dir)
    if a.command == "evict-cache":
        if a.budget is None:
            p.error("evict-cache needs --budget")
        r = evict(docker, state, a.project, a.budget, apply=a.apply, min_idle_seconds=a.min_idle_seconds)
    else:
        if a.command == "preview" and a.apply:
            p.error("preview is read-only")
        r = reconcile(docker, state, a.project, apply=a.apply and a.command == "reconcile", run_filter=a.run)
    print(json.dumps(r, indent=2, sort_keys=True))
    return 0 if r["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
