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
                 "LICENSE-MIT": b"MIT license\n",
                 "LICENSE-APACHE": b"Apache license\n",
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

    def test_cargo_log_truncation_failure_extra_binary_and_outside_artifact_fail(self):
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.log_truncated_or_invalid"):
            PACKAGE.cargo_events(b'{"reason":"build-finished","success":')
        event = {"reason": "compiler-artifact", "package_id": "path+file:///tmp/src/crates/agent-router-cli#0.1.0",
                 "target": {"name": "agent-router", "kind": ["bin"]}, "features": [],
                 "filenames": [str(self.root / "target/aarch64-apple-darwin/release/agent-router")]}
        finish = {"reason": "build-finished", "success": True}
        artifacts, binaries, finished = PACKAGE.cargo_events((json.dumps(event) + "\n" + json.dumps(finish)).encode())
        target = self.root / "target"
        executable = target / "aarch64-apple-darwin/release/agent-router"
        self.assertEqual(artifacts, [("agent-router-cli", "0.1.0")])
        PACKAGE.check_build_event_contract(0, finished, binaries, target, executable, [])
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.failed.exit_1"):
            PACKAGE.check_build_event_contract(1, [False], binaries, target, executable, [])
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.expected_single_binary_artifact"):
            PACKAGE.check_build_event_contract(0, [True], binaries * 2, target, executable, [])
        outside = json.loads(json.dumps(event))
        outside["filenames"] = ["/tmp/not-target/agent-router"]
        _, bad_binaries, _ = PACKAGE.cargo_events(json.dumps(outside).encode())
        with self.assertRaisesRegex(PACKAGE.PackageError, "build.executable_not_in_cargo_event"):
            PACKAGE.check_build_event_contract(0, [True], bad_binaries, target, executable, [])

    def test_binary_archive_rejects_extra_paths_symlinks_and_modes(self):
        proof = {"features": [], "target": "aarch64-apple-darwin", "vendor_inventory": {},
                 "build_packages": [], "source_sha256": "a" * 64, "source_manifest_sha256": "b" * 64,
                 "lock_sha256": "c" * 64, "vendor_sha256": "d" * 64, "cargo_version": "cargo test",
                 "rustc_version": "rustc test", "cargo_binary_sha256": "1" * 64,
                 "rustc_binary_sha256": "2" * 64, "c_compiler": "not_available", "sdk": "not_available",
                 "dynamic_libraries_declared": "not_inspected"}
        source = self.source()
        files = PACKAGE.verify_binary_archive(PACKAGE.archive_payload(b"binary", proof, source))
        for member_name, mode, link_type in (("agent-router-binary/extra.txt", 0o644, None),
                                             ("agent-router-binary/../../escape", 0o644, None),
                                             ("agent-router-binary/extra-link", 0o777, tarfile.SYMTYPE),
                                             ("agent-router-binary/BUILD-RECIPE.json", 0o755, None)):
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
                for name, data in sorted(files.items()):
                    item = tarfile.TarInfo("agent-router-binary/" + name)
                    item.size = len(data)
                    item.mode = 0o755 if name == "bin/agent-router" else 0o644
                    if member_name.endswith("BUILD-RECIPE.json") and name == "BUILD-RECIPE.json":
                        item.mode = 0o755
                    archive.addfile(item, io.BytesIO(data))
                if member_name.endswith("BUILD-RECIPE.json"):
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
        payload = PACKAGE.archive_payload(b"fake executable", proof, source)
        files = PACKAGE.verify_binary_archive(payload)
        self.assertEqual(files["bin/agent-router"], b"fake executable")
        changed = dict(files)
        changed["bin/agent-router"] = b"tampered executable"
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in sorted(changed.items()):
                item = tarfile.TarInfo("agent-router-binary/" + name)
                item.size = len(data)
                item.mode = 0o755 if name == "bin/agent-router" else 0o644
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
        files = PACKAGE.verify_binary_archive(PACKAGE.archive_payload(b"binary", proof, source))
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
            PACKAGE.archive_payload(b"binary", proof, source)

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
        sbom = json.loads(PACKAGE.verify_binary_archive(PACKAGE.archive_payload(b"binary", proof, source))["BUILD-SBOM.json"])
        self.assertEqual(sbom["packages"], [])

    @staticmethod
    def make_archive(files):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in sorted(files.items()):
                item = tarfile.TarInfo("agent-router-binary/" + name)
                item.size = len(data)
                item.mode = 0o755 if name == "bin/agent-router" else 0o644
                archive.addfile(item, io.BytesIO(data))
        return buffer.getvalue()


if __name__ == "__main__":
    unittest.main()
