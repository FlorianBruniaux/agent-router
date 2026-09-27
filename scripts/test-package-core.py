#!/usr/bin/env python3
"""Behavior checks for the source exporter. No network or provider is used."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("package_core", Path(__file__).with_name("package-core.py"))
CORE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CORE)


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="agent-router-package-test-")
        self.root = Path(self.directory.name).resolve()

    def tearDown(self):
        self.directory.cleanup()

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data if isinstance(data, bytes) else data.encode())
        return path

    def crate(self, *, symlink=False, unsafe=False, license_text=True):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:gz") as archive:
            files = {"example-1.0.0/Cargo.toml": b'[package]\nname="example"\nversion="1.0.0"\nlicense="MIT"\n'}
            if license_text:
                files["example-1.0.0/../LICENSE" if unsafe else "example-1.0.0/LICENSE"] = b"Test license text\n"
            for name, content in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(content)
                if symlink and name.endswith("/LICENSE"):
                    info.type, info.linkname, info.size = tarfile.SYMTYPE, "elsewhere", 0
                archive.addfile(info, io.BytesIO(content) if info.isfile() else None)
        payload = data.getvalue()
        self.write("registry/cache/test/example-1.0.0.crate", payload)
        return {"name": "example", "version": "1.0.0", "source": "registry+https://example.invalid/index",
                "checksum": hashlib.sha256(payload).hexdigest()}

    def test_cached_archive_is_bound_to_lock_checksum(self):
        record, files = CORE.crate_evidence(self.crate(), self.root)
        self.assertEqual(record["evidence"], "checksum_verified")
        self.assertEqual(record["license_declared"], "MIT")
        self.assertEqual(record["license_files"][0]["sha256"], CORE.digest(b"Test license text\n"))
        self.assertEqual(list(files), ["licenses/dependencies/example-1.0.0/LICENSE"])
        self.assertNotIn(str(self.root), json.dumps(record))

    def test_corrupt_or_missing_cache_preserves_unknown(self):
        package = self.crate()
        package["checksum"] = "0" * 64
        record, files = CORE.crate_evidence(package, self.root)
        self.assertEqual(record["evidence"], "archive_checksum_mismatch")
        self.assertEqual(record["license_declared"], "NOASSERTION")
        self.assertEqual(files, {})
        record, files = CORE.crate_evidence(package, self.root / "missing")
        self.assertEqual(record["evidence"], "missing_cached_archive")

    def test_license_declaration_does_not_substitute_for_text(self):
        record, files = CORE.crate_evidence(self.crate(license_text=False), self.root)
        self.assertEqual(record["evidence"], "license_text_missing")
        self.assertEqual(record["license_declared"], "MIT")
        self.assertFalse(files)

    def test_license_symlink_and_traversal_are_refused(self):
        for option in ({"symlink": True}, {"unsafe": True}):
            with self.subTest(option=option):
                package = self.crate(**option)
                with self.assertRaisesRegex(CORE.PackageError, "license.unsafe_file"):
                    CORE.crate_evidence(package, self.root)

    def test_private_paths_credentials_and_env_files_are_refused(self):
        sensitive = [b"/Users/real-user/private/project", b"/home/account/private/project",
                     b"C:\\Users\\account\\secret", b"-----BEGIN PRIVATE KEY-----",
                     b"api_key = '" + b"x" * 24 + b"'"]
        for value in sensitive:
            with self.assertRaises(CORE.PackageError) as failure:
                CORE.check_content("fixture.json", value)
            self.assertNotIn(value.decode(), str(failure.exception))
        for name in ("", ".", "a/./b", "a//b", "../secret", "/absolute", "crates/private/a.rs", ".env.prod", "data.db", "keys/auth.pem"):
            self.assertFalse(CORE.safe_path(name), name)

    def test_source_symlink_is_refused_even_inside_the_root(self):
        target = self.write("source.rs", "fn example() {}")
        (self.root / "link.rs").symlink_to(target)
        with self.assertRaisesRegex(CORE.PackageError, "path.symlink"):
            CORE.read_source(self.root, "link.rs")

    def test_workspace_projection_discovers_members_and_preserves_dependencies(self):
        data = b'''[workspace]
members = ["crates/one", "xtask"]
default-members = ["xtask"]
resolver = "2"

[workspace.package]
license = "MIT OR Apache-2.0"

[workspace.dependencies]
serde = "1"
'''
        result = CORE.projected_manifest(data, [{"relative": "crates/new"}, {"relative": "crates/one"}])
        parsed = CORE.tomllib.loads(result.decode())
        self.assertEqual(parsed["workspace"]["members"], ["crates/new", "crates/one"])
        self.assertNotIn("default-members", parsed["workspace"])
        self.assertEqual(parsed["workspace"]["dependencies"], {"serde": "1"})

    def test_core_dependency_cannot_pull_in_an_excluded_integration(self):
        packages = [{"name": "core", "manifest_path": str(self.root / "crates/core/Cargo.toml"),
                     "dependencies": [{"name": "plugin", "path": str(self.root / "integrations/plugin"), "optional": True}]},
                    {"name": "plugin", "manifest_path": str(self.root / "integrations/plugin/Cargo.toml"),
                     "metadata": {"agent-router": {"distribution": "integration"}}, "dependencies": []}]
        metadata = {"workspace_root": str(self.root), "packages": packages}
        with patch.object(CORE, "run", return_value=json.dumps(metadata).encode()):
            with self.assertRaisesRegex(CORE.PackageError, "workspace.excluded_path_dependency"):
                CORE.discover(self.root)

    def archive(self, extra=None):
        files = {"Cargo.toml": b"[workspace]\nmembers=[]\n"}
        hashes = {name: CORE.digest(data) for name, data in files.items()}
        files["DISTRIBUTION.json"] = CORE.json_bytes({
            "schema_version": 1, "content_sha256": CORE.digest(CORE.json_bytes(hashes)),
            "files": [{"path": name, "sha256": value} for name, value in hashes.items()],
        })
        if extra:
            files.update(extra)
        return files

    def test_archive_is_reproducible_and_has_portable_permissions(self):
        files = self.archive()
        first = CORE.archive_bytes(files)
        self.assertEqual(first, CORE.archive_bytes(dict(reversed(list(files.items())))))
        with tarfile.open(fileobj=io.BytesIO(first), mode="r:gz") as archive:
            for member in archive:
                self.assertEqual((member.uid, member.gid, member.mtime, member.mode), (0, 0, 0, 0o644))
        path = self.write("test.tar.gz", first)
        self.assertTrue(CORE.verify_archive(path)["verified"])

    def test_tampering_and_unlisted_files_fail_inventory_verification(self):
        for change in ({"Cargo.toml": b"changed"}, {"extra.txt": b"injected"}):
            path = self.write("tampered.tar.gz", CORE.archive_bytes(self.archive(change)))
            with self.assertRaisesRegex(CORE.PackageError, "archive.inventory_mismatch"):
                CORE.verify_archive(path)

    def test_inventory_keeps_other_platform_dependencies_without_claiming_success(self):
        package = self.crate()
        missing = dict(package, name="windows-only", version="2.0.0")
        inventory, files = CORE.license_inventory({"package": [package, missing]}, [], self.root)
        self.assertEqual(len(inventory), 2)
        self.assertEqual(inventory[1]["evidence"], "missing_cached_archive")
        self.assertTrue(files)
        document = CORE.sbom(inventory, "a" * 64, "2026-01-01T00:00:00Z")
        self.assertEqual(document["packages"][1]["licenseConcluded"], "NOASSERTION")

    def test_supplemental_license_must_match_archive_and_text_hashes(self):
        package = self.crate(license_text=False)
        record, _ = CORE.crate_evidence(package, self.root)
        path = "licenses/third-party/example-1.0.0/LICENSE"
        self.write(path, b"Published upstream license\n")
        evidence = {"packages": [{"name": "example", "version": "1.0.0", "crate_sha256": package["checksum"],
                                  "files": [{"path": path, "sha256": CORE.digest(b"Published upstream license\n"),
                                             "url": "https://example.invalid/pinned-commit/LICENSE"}]}]}
        self.write("licenses/third-party/evidence.json", CORE.json_bytes(evidence))
        self.assertEqual(len(CORE.supplemental_licenses(self.root, record)), 1)
        self.assertEqual(record["evidence"], "checksum_and_upstream_verified")
        self.write(path, b"altered")
        with self.assertRaisesRegex(CORE.PackageError, "license.supplement_hash_mismatch"):
            CORE.supplemental_licenses(self.root, record)

    def test_dependency_scopes_separate_dev_build_and_normal_paths(self):
        packages = [{"id": name, "name": name, "version": "1", "targets": [{"kind": ["lib"]}]}
                    for name in ("core", "runtime", "test-helper", "compiler-helper", "macro")]
        packages[-1]["targets"] = [{"kind": ["proc-macro"]}]
        nodes = [{"id": p["id"], "deps": []} for p in packages]
        nodes[0]["deps"] = [{"pkg": name, "dep_kinds": [{"kind": kind, "target": None}]}
                            for name, kind in (("runtime", None), ("test-helper", "dev"),
                                               ("compiler-helper", "build"), ("macro", None))]
        result = CORE.dependency_scopes({"packages": packages, "resolve": {"nodes": nodes}}, [packages[0]])
        self.assertEqual(result[("runtime", "1")], ["normal"])
        self.assertEqual(result[("test-helper", "1")], ["dev"])
        self.assertEqual(result[("compiler-helper", "1")], ["build"])
        self.assertEqual(result[("macro", "1")], ["build"])

    def test_complete_exports_are_identical_and_never_replace_an_artifact(self):
        self.write("Cargo.toml", '[workspace]\nmembers=["crates/core"]\n')
        self.write("Cargo.lock", 'version=4\n[[package]]\nname="core"\nversion="1.0.0"\n')
        self.write("crates/core/Cargo.toml", '[package]\nname="core"\nversion="1.0.0"\n')
        self.write("crates/core/src/lib.rs", "pub fn answer() -> u8 { 42 }\n")
        self.write("licenses/LICENSE-MIT", "Fixture MIT license\n")
        self.write("licenses/LICENSE-APACHE", "Fixture Apache license\n")
        package = {"id": "core", "name": "core", "version": "1.0.0", "license": "MIT", "features": {},
                   "manifest_path": str(self.root / "crates/core/Cargo.toml"), "dependencies": [],
                   "targets": [{"kind": ["lib"]}]}
        metadata = {"workspace_root": str(self.root), "packages": [package],
                    "resolve": {"nodes": [{"id": "core", "deps": []}]}}

        def command(argv, cwd):
            if argv[0] == "cargo":
                return CORE.json_bytes(metadata)
            return b"2026-01-01T00:00:00Z\n" if "show" in argv else b"abcdef123456\n"

        with patch.object(CORE, "run", side_effect=command), patch.object(CORE.shutil, "which", return_value=None):
            first, second = self.root / "first.tar.gz", self.root / "second.tar.gz"
            a = CORE.package(self.root, first, self.root / "cache", True, False)
            b = CORE.package(self.root, second, self.root / "cache", True, False)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            self.assertEqual(a["sha256"], b["sha256"])
            with self.assertRaises(FileExistsError):
                CORE.package(self.root, first, self.root / "cache", True, False)
            self.assertEqual(CORE.verify_archive(first)["sha256"], a["sha256"])


if __name__ == "__main__":
    unittest.main()
