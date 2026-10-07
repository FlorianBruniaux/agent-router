#!/usr/bin/env python3
"""Adversarial checks for local binary export. No downloads or providers."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import contextlib
import io
import sys
import tarfile
import tempfile
import threading
import time
import unittest
from unittest import mock

sys_path = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("package_binary", sys_path / "package-binary.py")
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class BinaryPackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="agent-router-binary-test-")
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def source(self):
        files = {"Cargo.toml": b"[workspace]\nmembers=[]\n",
                 "Cargo.lock": b"version = 4\n\n[[package]]\nname='agent-router-cli'\nversion='0.1.0'\n",
                 "LICENSE-INVENTORY.json": b'{"packages":[]}\n',
                 "LICENSE.md": b"Project license\n",
                 "NOTICE": b"Project notice\n",
                 "THIRD_PARTY_NOTICES.md": b"No external package in this fixture.\n"}
        hashes = {name: hashlib.sha256(data).hexdigest() for name, data in sorted(files.items())}
        manifest = {"kind": "core-source", "content_sha256": PACKAGE.sha(PACKAGE.CORE.json_bytes(hashes)),
                    "files": [{"path": name, "sha256": value} for name, value in hashes.items()],
                    "evidence": {"license_texts": "complete"}}
        files["DISTRIBUTION.json"] = PACKAGE.CORE.json_bytes(manifest)
        return files

    def write_archive(self, files):
        path = self.root / "core.tar.gz"
        path.write_bytes(PACKAGE.CORE.archive_bytes(files, "core"))
        return path

    def test_source_requires_independent_exact_digest_and_core_kind(self):
        path = self.write_archive(self.source())
        with self.assertRaisesRegex(PACKAGE.PackageError, "source.sha256_mismatch"):
            PACKAGE.checked_archive(path, "0" * 64)
        self.assertEqual(PACKAGE.checked_archive(path, PACKAGE.sha(path.read_bytes()))["Cargo.lock"], self.source()["Cargo.lock"])

    def test_source_rejects_incomplete_license_proof(self):
        files = self.source()
        value = json.loads(files["DISTRIBUTION.json"])
        value["evidence"]["license_texts"] = "incomplete"
        files["DISTRIBUTION.json"] = PACKAGE.CORE.json_bytes(value)
        path = self.write_archive(files)
        with self.assertRaisesRegex(PACKAGE.PackageError, "source.license_evidence_incomplete"):
            PACKAGE.checked_archive(path, PACKAGE.sha(path.read_bytes()))

    def test_cargo_config_in_any_ancestor_or_symlink_is_refused(self):
        root = self.root / "ancestor"
        nested = root / "output" / "nested"
        nested.mkdir(parents=True)
        config_dir = root / ".cargo"
        config_dir.mkdir()
        (config_dir / "config.toml").write_text("[env]\nREVIEW_CARGO_ANCESTOR = 'injected'\n")
        with self.assertRaisesRegex(PACKAGE.PackageError, "cargo.inherited_config_refused"):
            PACKAGE.check_cargo_config_ancestors(nested)
        (config_dir / "config.toml").unlink()
        target = self.root / "linked-config.toml"
        target.write_text("[env]\nREVIEW_CARGO_ANCESTOR = 'injected'\n")
        (config_dir / "config").symlink_to(target)
        with self.assertRaisesRegex(PACKAGE.PackageError, "cargo.inherited_config_refused"):
            PACKAGE.check_cargo_config_ancestors(nested)

    def test_main_refuses_ancestor_cargo_config_before_tools_run(self):
        root = self.root / "main-case"
        (root / ".cargo").mkdir(parents=True)
        (root / ".cargo/config.toml").write_text("[env]\nREVIEW_CARGO_ANCESTOR = 'injected'\n")
        output_parent = root / "output"
        output_parent.mkdir()
        source_archive = self.write_archive(self.source())
        marker = root / "tool-ran"
        fake_cargo = root / "cargo"
        fake_rustc = root / "rustc"
        for executable in (fake_cargo, fake_rustc):
            executable.write_text("#!/bin/sh\nprintf ran >> "+repr(str(marker))+"\nexit 0\n")
            executable.chmod(0o755)
        argv = ["package-binary.py", "--source-archive", str(source_archive),
                "--source-sha256", PACKAGE.sha(source_archive.read_bytes()),
                "--target", "aarch64-apple-darwin", "--output", str(output_parent / "binary.tar.gz"),
                "--diagnostics-dir", str(output_parent / "diagnostics"),
                "--cargo", str(fake_cargo), "--rustc", str(fake_rustc)]
        old_argv = sys.argv
        sys.argv = argv
        try:
            with contextlib.redirect_stdout(io.StringIO()) as captured:
                self.assertEqual(PACKAGE.main(), 2)
            self.assertIn("cargo.inherited_config_refused", captured.getvalue())
        finally:
            sys.argv = old_argv
        self.assertFalse(marker.exists())

    def test_bounded_process_kills_writer_before_it_can_continue(self):
        marker = self.root / "continued-after-limit"
        program = ("import pathlib,time; "+
                   "import sys; sys.stdout.write('x'*2048); sys.stdout.flush(); "+
                   "time.sleep(.2); pathlib.Path("+repr(str(marker))+").write_text('bad')")
        started = time.monotonic()
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.log_limit"):
            PACKAGE.run_bounded_process([sys.executable, "-c", program], self.root, os.environ.copy(), 5, max_log=1024)
        self.assertLess(time.monotonic() - started, 2)
        self.assertFalse(marker.exists())

    def test_bounded_process_timeout_still_applies_after_both_pipes_close(self):
        marker = self.root / "continued-after-eof-timeout"
        program = ("import os,pathlib,time; os.close(1); os.close(2); time.sleep(1); "+
                   "pathlib.Path("+repr(str(marker))+").write_text('bad')")
        started = time.monotonic()
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.timeout"):
            PACKAGE.run_bounded_process([sys.executable, "-c", program], self.root, os.environ.copy(), .15, max_log=1024)
        self.assertLess(time.monotonic() - started, .8)
        self.assertFalse(marker.exists())

    def test_vendor_requires_lock_checksum_and_safe_regular_members(self):
        payload = io.BytesIO()
        with tarfile.open(fileobj=payload, mode="w:gz") as archive:
            content = b'[package]\nname="tiny"\nversion="1.0.0"\nlicense="MIT"\n'
            item = tarfile.TarInfo("tiny-1.0.0/Cargo.toml")
            item.size = len(content)
            archive.addfile(item, io.BytesIO(content))
        crate = payload.getvalue()
        cache = self.root / "cargo/registry/cache/test"
        cache.mkdir(parents=True)
        (cache / "tiny-1.0.0.crate").write_bytes(crate)
        lock = f'version=4\n[[package]]\nname="tiny"\nversion="1.0.0"\nsource="registry+https://example.invalid"\nchecksum="{PACKAGE.sha(crate)}"\n'.encode()
        records, _ = PACKAGE.vendor(lock, self.root / "cargo", self.root / "vendor")
        self.assertEqual(records[("tiny", "1.0.0", "registry+https://example.invalid")]["observed_license"], "MIT")
        bad_lock = lock.replace(PACKAGE.sha(crate).encode(), b"0" * 64)
        with self.assertRaisesRegex(PACKAGE.PackageError, "vendor.crate_missing_or_ambiguous"):
            PACKAGE.vendor(bad_lock, self.root / "cargo", self.root / "vendor-bad")

    def cargo_fixture(self):
        target = self.root / "target"
        executables = {name: target / "aarch64-apple-darwin/release" / name for name in PACKAGE.BINARY_PACKAGES}
        events = [{"reason": "compiler-artifact", "package_id": "path+file:///tmp/src/crates/" + package + "#0.1.0",
                   "target": {"name": name, "kind": ["bin"]},
                   "features": PACKAGE.DEFAULT_FEATURES if name == "agent-router" else [],
                   "executable": str(executables[name]), "filenames": [str(executables[name])]}
                  for name, package in PACKAGE.BINARY_PACKAGES.items()]
        return target, executables, events

    def check_events(self, events, exit_code=0, finished=None):
        target, executables, _ = self.cargo_fixture()
        packages, binaries, _ = PACKAGE.cargo_events("\n".join(json.dumps(event) for event in events).encode())
        return PACKAGE.check_build_event_contract(exit_code, [True] if finished is None else finished,
                    binaries, target, executables, PACKAGE.DEFAULT_FEATURES,
                    {package: "0.1.0" for package in PACKAGE.BINARY_PACKAGES.values()})

    def test_cargo_log_truncation_failure_extra_binary_and_outside_artifact_fail(self):
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.log_truncated_or_invalid"):
            PACKAGE.cargo_events(b'{"reason":"build-finished","success":')
        _, _, events = self.cargo_fixture()
        observed = self.check_events(events)
        self.assertEqual([event["target"] for event in observed], list(PACKAGE.BINARY_PACKAGES))
        self.assertEqual(observed[0]["features"], ["claude", "codex"])
        self.assertEqual(observed[1]["features"], [])
        for bad in (events[:1], events + [events[1]], [events[0], events[0]]):
            with self.subTest(events=bad), self.assertRaisesRegex(PACKAGE.PackageError, "build.expected_two_binary_artifacts"):
                self.check_events(bad)
        for field, value, error in (("package_id", "path+file:///tmp/src/crates/wrong#0.1.0", "build.binary_package_mismatch"),
                                    ("package_id", "path+file:///tmp/src/crates/router-sandbox-helper#9.0.0", "build.binary_package_mismatch"),
                                    ("filenames", ["/tmp/not-target/router-sandbox-helper"], "build.executable_not_in_cargo_event"),
                                    ("executable", "/tmp/not-target/router-sandbox-helper", "build.executable_not_in_cargo_event"),
                                    ("features", ["default"], "build.feature_set_mismatch")):
            bad = json.loads(json.dumps(events)); bad[1][field] = value
            with self.subTest(field=field, value=value), self.assertRaisesRegex(PACKAGE.PackageError, error):
                self.check_events(bad)
        bad = json.loads(json.dumps(events)); bad[0]["features"] = ["codex"]
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.feature_set_mismatch"):
            self.check_events(bad)
        extra = json.loads(json.dumps(events)); extra.append(dict(events[1], target={"name": "extra", "kind": ["bin"]}))
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.expected_two_binary_artifacts"):
            self.check_events(extra)
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.failed.exit_1"):
            self.check_events(events, exit_code=1)

    def test_helper_must_be_regular_nonempty_and_exact_mode(self):
        _, executables, _ = self.cargo_fixture()
        for path in executables.values():
            path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(b"binary"); path.chmod(0o755)
        self.assertEqual(set(PACKAGE.read_build_binaries(executables)), set(PACKAGE.BINARY_PACKAGES))
        helper = executables["router-sandbox-helper"]
        helper.unlink()
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.binary_missing_or_oversized"):
            PACKAGE.read_build_binaries(executables)
        helper.symlink_to(executables["agent-router"])
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.binary_missing_or_oversized"):
            PACKAGE.read_build_binaries(executables)
        helper.unlink(); helper.write_bytes(b""); helper.chmod(0o755)
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.binary_empty"):
            PACKAGE.read_build_binaries(executables)
        helper.write_bytes(b"helper"); helper.chmod(0o700)
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.binary_mode_mismatch"):
            PACKAGE.read_build_binaries(executables)

    def test_helper_only_reproducibility_difference_is_refused(self):
        binaries = {"agent-router": b"same", "router-sandbox-helper": b"helper"}
        PACKAGE.check_reproducibility([binaries, dict(binaries)])
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.reproducibility_mismatch"):
            PACKAGE.check_reproducibility([binaries, dict(binaries, **{"router-sandbox-helper": b"different"})])

    def test_main_preserves_both_hash_reports_when_only_helper_differs(self):
        output = self.root / "refused.tar.gz"
        diagnostics = self.root / "diagnostics"
        builds = []
        for helper in (b"helper first", b"helper second"):
            binaries = {"agent-router": b"same CLI", "router-sandbox-helper": helper}
            builds.append({"binaries": binaries,
                           "binaries_sha256": {name: PACKAGE.sha(data) for name, data in binaries.items()},
                           "features": PACKAGE.DEFAULT_FEATURES, "events": [], "exit_code": 0})
        expected = [{key: value for key, value in build.items() if key != "binaries"} for build in builds]
        argv = ["package-binary.py", "--source-archive", str(self.root / "source.tar.gz"),
                "--source-sha256", "a" * 64, "--target", "aarch64-apple-darwin",
                "--output", str(output), "--diagnostics-dir", str(diagnostics)]
        def fake_version(cmd, **kwargs):
            stdout = (b"release: 1.85.1\nhost: aarch64-apple-darwin\n" if Path(cmd[0]).name == "rustc"
                      else b"cargo 1.85.1 (fixture)\n")
            return mock.Mock(stdout=stdout, returncode=0)
        with mock.patch.object(sys, "argv", argv), mock.patch.object(PACKAGE, "checked_archive", return_value=self.source()), mock.patch.object(PACKAGE, "resolve_tool", side_effect=lambda path, name: self.root / name), mock.patch.object(PACKAGE.subprocess, "run", side_effect=fake_version), mock.patch.object(PACKAGE, "vendor", return_value=({}, "d" * 64)), mock.patch.object(PACKAGE, "run_build", side_effect=builds), contextlib.redirect_stdout(io.StringIO()) as captured:
            self.assertEqual(PACKAGE.main(), 2)
        self.assertEqual(json.loads(captured.getvalue())["error"], "build.reproducibility_mismatch")
        self.assertFalse(output.exists())
        for index, report in enumerate(expected, 1):
            report["vendor_sha256"] = "d" * 64
            path = diagnostics / f"run-{index}.json"
            self.assertEqual(json.loads(path.read_bytes()), report)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertNotIn("binaries", json.loads(path.read_bytes()))
        self.assertEqual(diagnostics.stat().st_mode & 0o777, 0o700)
        self.assertEqual(expected[0]["binaries_sha256"]["agent-router"], expected[1]["binaries_sha256"]["agent-router"])
        self.assertNotEqual(expected[0]["binaries_sha256"]["router-sandbox-helper"], expected[1]["binaries_sha256"]["router-sandbox-helper"])

    def test_run_build_selects_both_packages_and_scopes_cli_features(self):
        source = self.source()
        source["Cargo.lock"] += b"\n[[package]]\nname='router-sandbox-helper'\nversion='0.1.0'\n"
        run_root = self.root / "run"; run_root.mkdir()
        cargo = self.root / "cargo"; cargo.write_bytes(b"cargo fixture")
        rustc = self.root / "rustc"; rustc.write_bytes(b"rustc fixture")
        def fake_build(cmd, cwd, env, timeout):
            self.assertEqual([cmd[i + 1] for i, arg in enumerate(cmd) if arg == "-p"], list(PACKAGE.BINARY_PACKAGES.values()))
            self.assertEqual([cmd[i + 1] for i, arg in enumerate(cmd) if arg == "--bin"], list(PACKAGE.BINARY_PACKAGES))
            self.assertEqual(cmd[cmd.index("--features") + 1], "agent-router-cli/claude,agent-router-cli/codex")
            self.assertIn("--no-default-features", cmd)
            self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
            events = []
            for name, package in PACKAGE.BINARY_PACKAGES.items():
                binary = Path(env["CARGO_TARGET_DIR"]) / "x86_64-unknown-linux-gnu/release" / name
                binary.parent.mkdir(parents=True, exist_ok=True); binary.write_bytes(name.encode()); binary.chmod(0o755)
                events.append({"reason": "compiler-artifact", "package_id": "path+file://" + str(cwd / "crates" / package) + "#0.1.0",
                               "target": {"name": name, "kind": ["bin"]}, "features": PACKAGE.DEFAULT_FEATURES if name == "agent-router" else [],
                               "filenames": [str(binary)], "executable": str(binary)})
            events.append({"reason": "build-finished", "success": True})
            return 0, "\n".join(json.dumps(event) for event in events).encode(), b""
        def fake_version(cmd, **kwargs):
            stdout = b"release: 1.85.1\nhost: x86_64-unknown-linux-gnu\n" if cmd[0] == str(rustc) else b"cargo 1.85.1 (fixture)\n"
            return mock.Mock(stdout=stdout, returncode=0)
        with mock.patch.object(PACKAGE, "run_bounded_process", side_effect=fake_build), mock.patch.object(PACKAGE.subprocess, "run", side_effect=fake_version), mock.patch.object(PACKAGE.shutil, "which", return_value=None):
            result = PACKAGE.run_build(source, self.root / "vendor", run_root, cargo, rustc,
                                       self.root / "rustup", "x86_64-unknown-linux-gnu", PACKAGE.DEFAULT_FEATURES, {}, self.root / "run.log")
        self.assertEqual(result["binaries"], {name: name.encode() for name in PACKAGE.BINARY_PACKAGES})
        self.assertEqual(result["binaries_sha256"], {name: PACKAGE.sha(name.encode()) for name in PACKAGE.BINARY_PACKAGES})
        self.assertEqual(len(result["events"]), 2)
        self.assertEqual((self.root / "run.log").stat().st_mode & 0o777, 0o600)

    def test_binary_archive_rejects_extra_paths_symlinks_and_modes(self):
        proof = {"features": [], "target": "aarch64-apple-darwin", "vendor_inventory": {},
                 "build_packages": [], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": "not_inspected"}
        source = self.source()
        files = PACKAGE.verify_binary_archive(self.package(b"binary", proof, source))
        for member_name, mode, link_type in (("agent-router-binary/extra.txt", 0o644, None),
                                             ("agent-router-binary/../../escape", 0o644, None),
                                             ("agent-router-binary/extra-link", 0o777, tarfile.SYMTYPE),
                                             ("agent-router-binary/BUILD-RECIPE.json", 0o755, None),
                                             ("agent-router-binary/bin/router-sandbox-helper", 0o644, None),
                                             ("agent-router-binary/bin/router-sandbox-helper", 0o755, tarfile.SYMTYPE)):
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
                for name, data in sorted(files.items()):
                    item = tarfile.TarInfo("agent-router-binary/" + name)
                    item.size = len(data)
                    item.mode = 0o755 if name in {"bin/agent-router", "bin/router-sandbox-helper"} else 0o644
                    if member_name.endswith("BUILD-RECIPE.json") and name == "BUILD-RECIPE.json":
                        item.mode = 0o755
                    if member_name == "agent-router-binary/bin/router-sandbox-helper" and name == "bin/router-sandbox-helper":
                        item.mode = mode
                        if link_type:
                            item.type, item.linkname, item.size = link_type, "agent-router", 0
                            archive.addfile(item)
                            continue
                    archive.addfile(item, io.BytesIO(data))
                if member_name.endswith(("BUILD-RECIPE.json", "bin/router-sandbox-helper")):
                    pass
                else:
                    extra = tarfile.TarInfo(member_name)
                    extra.mode = mode
                    if link_type:
                        extra.type, extra.linkname, extra.size = link_type, "LICENSE", 0
                        archive.addfile(extra)
                    else:
                        extra.size = 1
                        archive.addfile(extra, io.BytesIO(b"x"))
            with self.assertRaises(PACKAGE.PackageError):
                PACKAGE.verify_binary_archive(buffer.getvalue())

    def test_output_integrity_and_required_modes(self):
        source = self.source()
        proof = {"features": [], "target": "aarch64-apple-darwin", "vendor_inventory": {},
                 "build_packages": [], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": "not_inspected"}
        payload = self.package(b"fake executable", proof, source)
        files = PACKAGE.verify_binary_archive(payload)
        self.assertEqual(files["bin/agent-router"], b"fake executable")
        changed = dict(files)
        changed["bin/agent-router"] = b"tampered executable"
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in sorted(changed.items()):
                item = tarfile.TarInfo("agent-router-binary/" + name)
                item.size = len(data)
                item.mode = 0o755 if name in {"bin/agent-router", "bin/router-sandbox-helper"} else 0o644
                archive.addfile(item, io.BytesIO(data))
        with self.assertRaisesRegex(PACKAGE.PackageError, "archive.integrity_mismatch"):
            PACKAGE.verify_binary_archive(buffer.getvalue())

    def test_build_sbom_maps_exact_lock_and_license_checksum(self):
        source = self.source()
        crate_hash = "e" * 64
        source["Cargo.lock"] += f'\n[[package]]\nname="tiny"\nversion="1.0.0"\nsource="registry+https://example.invalid"\nchecksum="{crate_hash}"\n'.encode()
        license_path = "licenses/dependencies/tiny-1.0.0/LICENSE-MIT"
        license_text = b"tiny third-party license text\n"
        source[license_path] = license_text
        source["LICENSE-INVENTORY.json"] = PACKAGE.CORE.json_bytes({"packages": [{"name": "tiny", "version": "1.0.0", "checksum": crate_hash, "license_declared": "MIT", "license_files": [{"path": license_path, "sha256": PACKAGE.sha(license_text)}]}]})
        proof = {"features": [], "target": "aarch64-apple-darwin", "vendor_inventory": {("tiny", "1.0.0", "registry+https://example.invalid"): {"checksum": crate_hash, "observed_license": "MIT"}},
                 "build_packages": [("tiny", "1.0.0")], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": "not_inspected"}
        files = PACKAGE.verify_binary_archive(self.package(b"binary", proof, source))
        sbom = json.loads(files["BUILD-SBOM.json"])
        self.assertEqual(sbom["packages"][0]["license_declared"], "MIT")
        self.assertEqual(files[license_path], license_text)
        self.assertEqual(sbom["packages"][0]["license_files"][0]["path"], license_path)
        self.assertTrue(sbom["packages"][0]["build_observed"])
        damaged = dict(files)
        damaged.pop(license_path)
        with self.assertRaisesRegex(PACKAGE.PackageError, "archive.integrity_mismatch"):
            PACKAGE.verify_binary_archive(self.make_archive(damaged))
        proof["vendor_inventory"][("tiny", "1.0.0", "registry+https://example.invalid")]["checksum"] = "f" * 64
        with self.assertRaisesRegex(PACKAGE.PackageError, "sbom.license_record_mismatch"):
            self.package(b"binary", proof, source)

    def test_dev_only_lock_entry_is_not_claimed_build_observed(self):
        source = self.source()
        crate_hash = "e" * 64
        source["Cargo.lock"] += f'\n[[package]]\nname="dev-only"\nversion="1.0.0"\nsource="registry+https://example.invalid"\nchecksum="{crate_hash}"\n'.encode()
        source["LICENSE-INVENTORY.json"] = PACKAGE.CORE.json_bytes({"packages": [{"name": "dev-only", "version": "1.0.0", "checksum": crate_hash, "license_declared": "MIT"}]})
        proof = {"features": [], "target": "aarch64-apple-darwin", "vendor_inventory": {("dev-only", "1.0.0", "registry+https://example.invalid"): {"checksum": crate_hash, "observed_license": "MIT"}},
                 "build_packages": [], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": "not_inspected"}
        sbom = json.loads(PACKAGE.verify_binary_archive(self.package(b"binary", proof, source))["BUILD-SBOM.json"])
        self.assertEqual(sbom["packages"], [])

    @staticmethod
    def package(binary, proof, source):
        binaries = {"agent-router": binary, "router-sandbox-helper": b"sandbox helper"}
        hashes = {name: PACKAGE.sha(data) for name, data in binaries.items()}
        proof = dict(proof, builds=[{"binaries_sha256": hashes}, {"binaries_sha256": hashes}])
        return PACKAGE.archive_payload(binaries, proof, source)

    def test_v2_archive_refuses_incomplete_or_inconsistent_helper_evidence(self):
        proof = {"features": PACKAGE.DEFAULT_FEATURES, "target": "aarch64-apple-darwin", "vendor_inventory": {},
                 "build_packages": [], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": {name: "not_inspected" for name in PACKAGE.BINARY_PACKAGES}}
        original = PACKAGE.verify_binary_archive(self.package(b"cli", proof, self.source()))
        self.assertEqual(original["bin/router-sandbox-helper"], b"sandbox helper")
        for mutation in ("missing", "empty", "changed_helper", "recipe_v1", "recipe_missing", "observation_hash", "second_build_hash", "cli_features"):
            files = dict(original)
            recipe = json.loads(files["BUILD-RECIPE.json"])
            observations = json.loads(files["BUILD-OBSERVATIONS.json"])
            if mutation == "missing": files.pop("bin/router-sandbox-helper")
            if mutation == "empty": files["bin/router-sandbox-helper"] = b""
            if mutation == "changed_helper": files["bin/router-sandbox-helper"] = b"changed helper"
            if mutation == "recipe_v1": recipe["schema"] = "agent-router-build-recipe-v1"
            if mutation == "recipe_missing": recipe["binaries"].pop()
            if mutation == "observation_hash": observations["binaries"][1]["sha256"] = "0" * 64
            if mutation == "second_build_hash": observations["builds"][1]["binaries_sha256"]["router-sandbox-helper"] = "0" * 64
            if mutation == "cli_features": recipe["binaries"][0]["features"] = ["codex"]
            files["BUILD-RECIPE.json"] = PACKAGE.canonical(recipe)
            files["BUILD-OBSERVATIONS.json"] = PACKAGE.canonical(observations)
            files["INTEGRITY.json"] = PACKAGE.canonical({name: PACKAGE.sha(data) for name, data in files.items() if name != "INTEGRITY.json"})
            with self.subTest(mutation=mutation), self.assertRaises(PACKAGE.PackageError):
                PACKAGE.verify_binary_archive(self.make_archive(files))
        binaries = {"agent-router": b"cli", "router-sandbox-helper": b"helper"}
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.reproducibility_evidence_mismatch"):
            PACKAGE.archive_payload(binaries, proof, self.source())
        with self.assertRaisesRegex(PACKAGE.PackageError, "archive.binary_inventory_mismatch"):
            PACKAGE.archive_payload({"agent-router": b"cli"}, proof, self.source())

    @staticmethod
    def make_archive(files):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in sorted(files.items()):
                item = tarfile.TarInfo("agent-router-binary/" + name)
                item.size = len(data)
                item.mode = 0o755 if name in {"bin/agent-router", "bin/router-sandbox-helper"} else 0o644
                archive.addfile(item, io.BytesIO(data))
        return buffer.getvalue()


if __name__ == "__main__":
    unittest.main()
