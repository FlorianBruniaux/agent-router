#!/usr/bin/env python3
"""Behavior checks for the source exporter. No network or provider is used."""

import hashlib
import importlib.util
import io
import json
import shutil
from contextlib import redirect_stdout
from pathlib import Path
import tarfile
import tempfile
import unittest
import sys
from unittest.mock import patch


sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("package_core", Path(__file__).with_name("package-core.py"))
CORE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CORE)


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="agent-router-package-test-")
        self.root = Path(self.directory.name).resolve()
        self.write("rust-toolchain.toml", '[toolchain]\nchannel="1.85"\nprofile="minimal"\ncomponents=["rustfmt", "clippy"]\n')

    def tearDown(self):
        self.directory.cleanup()

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data if isinstance(data, bytes) else data.encode())
        return path

    def crate(self, *, symlink=False, unsafe=False, license_text=True, license_names=("LICENSE",),
              name="example", license_declared="MIT"):
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:gz") as archive:
            manifest = f'[package]\nname="{name}"\nversion="1.0.0"\n'
            if license_declared is not None:
                manifest += f'license="{license_declared}"\n'
            files = {f"{name}-1.0.0/Cargo.toml": manifest.encode()}
            if license_text:
                for license_name in license_names:
                    prefix = f"{name}-1.0.0/../" if unsafe else f"{name}-1.0.0/"
                    files[prefix + license_name] = b"Test license text\n"
            for relative, content in files.items():
                info = tarfile.TarInfo(relative)
                info.size = len(content)
                if symlink and any(relative.endswith("/" + license) for license in license_names):
                    info.type, info.linkname, info.size = tarfile.SYMTYPE, "elsewhere", 0
                archive.addfile(info, io.BytesIO(content) if info.isfile() else None)
        payload = data.getvalue()
        self.write(f"registry/cache/test/{name}-1.0.0.crate", payload)
        return {"name": name, "version": "1.0.0", "source": "registry+https://example.invalid/index",
                "checksum": hashlib.sha256(payload).hexdigest()}

    def test_cached_archive_is_bound_to_lock_checksum(self):
        record, files = CORE.crate_evidence(self.crate(), self.root)
        self.assertEqual(record["evidence"], "checksum_verified")
        self.assertEqual(record["license_declared"], "MIT")
        self.assertEqual(record["license_files"][0]["sha256"], CORE.digest(b"Test license text\n"))
        self.assertEqual(list(files), ["licenses/dependencies/example-1.0.0/LICENSE"])
        self.assertNotIn(str(self.root), json.dumps(record))

    def test_suffix_license_names_preserve_exact_text_and_checksum_evidence(self):
        names = ("MIT-LICENSE", "Apache-LICENSE", "BSD-LICENCE")
        record, files = CORE.crate_evidence(self.crate(license_names=names), self.root)
        self.assertEqual(record["evidence"], "checksum_verified")
        expected = {"licenses/dependencies/example-1.0.0/" + name for name in names}
        self.assertEqual(set(files), expected)
        self.assertEqual({entry["path"] for entry in record["license_files"]}, expected)
        for entry in record["license_files"]:
            self.assertEqual(files[entry["path"]], b"Test license text\n")
            self.assertEqual(entry["sha256"], CORE.digest(files[entry["path"]]))
        spdx = CORE.sbom([record], "fixture", "2026-01-01T00:00:00Z")
        self.assertTrue(all(p["licenseConcluded"] == "NOASSERTION" for p in spdx["packages"]))

    def test_suffix_lookalikes_do_not_substitute_for_license_text(self):
        package = self.crate(license_names=("parser-license.rs", "notes-on-licenses.txt"))
        record, files = CORE.crate_evidence(package, self.root)
        self.assertEqual(record["evidence"], "license_text_missing")
        self.assertFalse(record["license_files"])
        self.assertFalse(files)

    def test_suffix_license_symlink_and_traversal_are_refused(self):
        for option in ({"symlink": True}, {"unsafe": True}):
            with self.subTest(option=option):
                package = self.crate(license_names=("MIT-LICENSE",), **option)
                with self.assertRaisesRegex(CORE.PackageError, "license.unsafe_file"):
                    CORE.crate_evidence(package, self.root)

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
license = "FSL-1.1-ALv2"

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

    def test_integration_selection_closes_public_dependencies_and_excludes_private_members(self):
        def member(name, relative, dependencies=(), declared=None):
            return {"name": name, "manifest_path": str(self.root / relative / "Cargo.toml"),
                    "metadata": {"agent-router": {"distribution": declared}} if declared else {},
                    "dependencies": [{"name": name, "path": str(self.root / path), "optional": True}
                                     for name, path in dependencies]}
        packages = [member("adapter-new", "crates/adapter-new", [("api", "crates/api")]),
                    member("api", "crates/api", [("types", "crates/types")]),
                    member("types", "crates/types"), member("cli", "crates/cli"),
                    member("bridge", "integrations/bridge", [("api", "crates/api")], "integration"),
                    member("private", "integrations/private", declared="private"),
                    member("xtask", "xtask")]
        metadata = {"workspace_root": str(self.root), "packages": packages}
        with patch.object(CORE, "run", return_value=CORE.json_bytes(metadata)):
            _, selected, excluded = CORE.discover(self.root, "integrations")
        self.assertEqual([p["name"] for p in selected], ["adapter-new", "api", "bridge", "types"])
        self.assertEqual([p["name"] for p in excluded], ["cli", "private", "xtask"])
        packages[0]["dependencies"].append({"name": "private", "path": str(self.root / "integrations/private")})
        with patch.object(CORE, "run", return_value=CORE.json_bytes(metadata)):
            with self.assertRaisesRegex(CORE.PackageError, "workspace.excluded_path_dependency"):
                CORE.discover(self.root, "integrations")

    def test_integration_assets_and_conformance_are_selected_without_private_or_global_files(self):
        self.write("Cargo.toml", '[workspace]\nmembers=["integrations/bridge"]\n')
        self.write("Cargo.lock", "version=4\n")
        for name in ("LICENSE.md", "NOTICE"):
            self.write(name, "Fixture license")
        package = {"relative": "integrations/bridge"}
        self.write("integrations/bridge/Cargo.toml", '[package]\nname="bridge"\nversion="0.1.0"\n')
        self.write("integrations/bridge/src/lib.rs", "pub fn example() {}")
        self.write("integrations/bridge/tests/conformance.rs", "#[test] fn contract() {}")
        self.write("integrations/bridge/protocol-sources.json", '{"schema_version":1}')
        self.write("integrations/host/adapter.json", '{"activation":"not_installed"}')
        self.write("integrations/README.md", "Inert integration descriptors.")
        self.write("integrations/private/secret.json", "Never distribute")
        self.write(".codex/config.toml", "Never distribute")
        self.write("fixtures/examples/public.json", "{}")
        files = CORE.collect_sources(self.root, [package], "integrations", [{"relative": "integrations/private"}])
        self.assertIn("integrations/bridge/tests/conformance.rs", files)
        self.assertIn("integrations/bridge/protocol-sources.json", files)
        self.assertIn("integrations/host/adapter.json", files)
        self.assertIn("fixtures/examples/public.json", files)
        self.assertNotIn("integrations/private/secret.json", files)
        self.assertNotIn(".codex/config.toml", files)
        self.write("integrations/host/.env.local", "Never distribute")
        with self.assertRaises(CORE.PackageError):
            CORE.collect_sources(self.root, [package], "integrations", [{"relative": "integrations/private"}])

    def public_score_fixture(self):
        self.write("Cargo.toml", '[workspace]\nmembers=["crates/router-runtime"]\n')
        self.write("Cargo.lock", "version=4\n")
        for name in ("LICENSE.md", "NOTICE"):
            self.write(name, "Fixture license")
        self.write("crates/router-runtime/Cargo.toml", '[package]\nname="router-runtime"\nversion="0.1.0"\n')
        self.write("crates/router-runtime/src/lib.rs", "pub fn fixture() {}")
        assets = {
            "docs/bench/native-useful-score-2026-10-21/verify.py": b"# reviewed public verifier\n",
            "docs/bench/native-useful-score-2026-10-21/required-cases.rs": b"// reviewed public cases\n",
            "docs/bench/native-useful-score-2026-10-21/acceptance.json": b'{"fixture":"public"}',
            "docs/bench/native-useful-score-2026-10-21/change.patch": b"reviewed public patch\n",
            "docs/bench/native-useful-score-2026-10-21/submission/lib.rs": b"// public authored solution\n",
            "docs/bench/linux-finite-dispatch-2026-10-16/inputs/t02/controller/control-output.json": b'{"lib.rs":"public solution"}',
        }
        for name, data in assets.items():
            self.write(name, data)
        self.write("docs/bench/private/unrelated.json", "Never distribute")
        return {"relative": "crates/router-runtime"}, assets

    def test_export_preserves_exact_toolchain_pin_and_refuses_missing_pin(self):
        command = self.export_fixture()
        pin = (self.root / "rust-toolchain.toml").read_bytes()
        output = self.root / "pinned.tar.gz"
        status, _ = self.strict_export(output, command)
        self.assertEqual(status, 0)
        self.assertEqual(self.archive_files(output).get("rust-toolchain.toml"), pin)
        (self.root / "rust-toolchain.toml").unlink()
        missing = self.root / "missing-pin.tar.gz"
        status, result = self.strict_export(missing, command)
        self.assertEqual(status, 2)
        self.assertIn("source.missing_or_oversized", result["error"])
        self.assertFalse(missing.exists())

    def test_selected_tests_carry_only_their_reviewed_external_inputs(self):
        cases = {
            "crates/agent-router-cli/src/generation_proof_tests.rs": ("tests/support/capability_store.rs",),
            "crates/agent-router-cli/tests/support/native_fixture.rs": ("tests/support/capability_store.rs",),
            "crates/router-runtime/src/finite_tools/linux_tests.rs": (
                "docs/bench/linux-finite-tools-guard-2026-10-16/feedback.py",),
            "crates/router-adapter-api/tests/registry.rs": (
                "integrations/codex/adapter.json", "integrations/claude-code/adapter.json"),
            "crates/router-workflow/tests/handoff.rs": ("assets/prompts/autonomous-handoff.md",),
        }
        self.public_score_fixture()
        for anchor, inputs in cases.items():
            with self.subTest(anchor=anchor):
                relative = "/".join(anchor.split("/")[:2])
                self.write(relative + "/Cargo.toml", '[package]\nname="fixture"\nversion="0.1.0"\n')
                self.write(anchor, "// public test input consumer")
                for name in inputs:
                    self.write(name, "public test fixture")
                    self.write(str(Path(name).with_name("unrelated.txt")), "unclassified neighbor")
                files = CORE.collect_sources(self.root, [{"relative": relative}])
                for name in inputs:
                    self.assertEqual(files.get(name), b"public test fixture")
                    self.assertNotIn(str(Path(name).with_name("unrelated.txt")), files)
                (self.root / anchor).unlink()
                files = CORE.collect_sources(self.root, [{"relative": relative}])
                self.assertTrue(set(inputs).isdisjoint(files))

    def test_external_test_input_retains_missing_content_and_symlink_checks(self):
        package, _ = self.public_score_fixture()
        self.write("crates/router-runtime/src/finite_tools/linux_tests.rs", "// test consumer")
        name = "docs/bench/linux-finite-tools-guard-2026-10-16/feedback.py"
        with self.assertRaisesRegex(CORE.PackageError, "source.missing_or_oversized"):
            CORE.collect_sources(self.root, [package])
        target = self.write(name, "# /Users/private-account/secret")
        with self.assertRaisesRegex(CORE.PackageError, "content.private_home_path"):
            CORE.collect_sources(self.root, [package])
        target.unlink()
        target.symlink_to(self.write("public.txt", "safe"))
        with self.assertRaisesRegex(CORE.PackageError, "path.symlink"):
            CORE.collect_sources(self.root, [package])

    def test_selected_public_example_carries_its_exact_compile_inputs(self):
        package, assets = self.public_score_fixture()
        self.write("crates/router-runtime/examples/campaign/public_score.rs", "// compiler-embedded public assets")
        files = CORE.collect_sources(self.root, [package])
        for name, data in assets.items():
            self.assertIn(name, files)
            self.assertEqual(files[name], data)
        self.assertNotIn("docs/bench/private/unrelated.json", files)

    def test_unselected_public_example_does_not_export_bench_data(self):
        package, assets = self.public_score_fixture()
        files = CORE.collect_sources(self.root, [package])
        self.assertFalse(set(assets) & set(files))

    def test_missing_public_compile_input_refuses_incomplete_distribution(self):
        package, _ = self.public_score_fixture()
        self.write("crates/router-runtime/examples/campaign/public_score.rs", "// compiler-embedded public assets")
        (self.root/"docs/bench/native-useful-score-2026-10-21/verify.py").unlink()
        with self.assertRaisesRegex(CORE.PackageError, "source.missing_or_oversized"):
            CORE.collect_sources(self.root, [package])

    def test_versioned_t02_engine_is_the_only_python_example_exception(self):
        package, _ = self.public_score_fixture()
        known = "crates/router-runtime/examples/campaign/t02-v2/verify.py"
        self.write(known, "# public versioned evaluator")
        files = CORE.collect_sources(self.root, [package])
        self.assertEqual(files[known], b"# public versioned evaluator")
        self.write("crates/router-runtime/examples/campaign/t02-v2/unrelated.py", "# not an export input")
        with self.assertRaisesRegex(CORE.PackageError, "source.unclassified_file"):
            CORE.collect_sources(self.root, [package])

    def test_versioned_t02_exception_keeps_content_and_symlink_guards(self):
        package, _ = self.public_score_fixture()
        known = "crates/router-runtime/examples/campaign/t02-v2/verify.py"
        target = self.write(known, "# /Users/private-account/secret")
        with self.assertRaisesRegex(CORE.PackageError, "content.private_home_path"):
            CORE.collect_sources(self.root, [package])
        target.unlink()
        target.symlink_to(self.write("public.txt", "safe"))
        with self.assertRaisesRegex(CORE.PackageError, "path.symlink"):
            CORE.collect_sources(self.root, [package])

    def test_finite_example_carries_its_external_test_support(self):
        package, _ = self.public_score_fixture()
        self.write("crates/router-runtime/examples/finite-campaign.rs", "// explicit external test support")
        support = "tests/support/capability_verifier.rs"
        self.write(support, "// synthetic capability verifier")
        files = CORE.collect_sources(self.root, [package])
        self.assertEqual(files.get(support), b"// synthetic capability verifier")
        self.assertNotIn("docs/bench/private/unrelated.json", files)

    @unittest.skipUnless(shutil.which("gitleaks"), "gitleaks is not installed")
    def test_distribution_scan_applies_existing_policy_and_still_rejects_secrets(self):
        repository = Path(__file__).resolve().parent.parent
        self.write(".gitleaks.toml", (repository / ".gitleaks.toml").read_bytes())
        fixture = "tests/fixtures/evaluation/attempt-correlation-v1.json"
        self.assertEqual(CORE.scan_sources(self.root, {fixture: (repository / fixture).read_bytes()}, True), "passed")
        secret = b'api_key = "' + b"ghp_" + b"b7Jq9Vt2Nk4Lm6Xr8Wz0Cs3Df5Hg1Pa7Qv9Y" + b'"\n'
        with self.assertRaisesRegex(CORE.PackageError, "gitleaks.failed"):
            CORE.scan_sources(self.root, {"unexpected.txt": secret}, True)

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

    def test_integration_archive_prefix_and_manifest_kind_are_bound(self):
        files = self.archive()
        with self.assertRaisesRegex(CORE.PackageError, "archive.distribution_mismatch"):
            CORE.verify_archive(self.write("mismatch.tar.gz", CORE.archive_bytes(files, "integrations")))
        manifest = json.loads(files["DISTRIBUTION.json"])
        manifest["kind"] = "integrations-source"
        files["DISTRIBUTION.json"] = CORE.json_bytes(manifest)
        first = CORE.archive_bytes(files, "integrations")
        self.assertEqual(first, CORE.archive_bytes(files, "integrations"))
        result = CORE.verify_archive(self.write("integrations.tar.gz", first))
        self.assertEqual(result["kind"], "integrations-source")
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode="w:gz") as archive:
            for prefix in ("agent-router-core", "agent-router-integrations"):
                entry = tarfile.TarInfo(prefix + "/test.txt")
                entry.size = 1
                archive.addfile(entry, io.BytesIO(b"x"))
        with self.assertRaisesRegex(CORE.PackageError, "archive.invalid_prefix"):
            CORE.verify_archive(self.write("mixed.tar.gz", data.getvalue()))

    def test_lock_projection_preserves_pinned_versions_checksums_and_public_members(self):
        original = b'''version=4
[[package]]
name="core"
version="1.0.0"
dependencies=["external", "test-only"]
[[package]]
name="external"
version="2.0.0"
source="registry+https://example.invalid/index"
checksum="aaaaaaaa"
[[package]]
name="test-only"
version="1.0.0"
'''
        projected = original.replace(b'dependencies=["external", "test-only"]', b'dependencies=["external"]')
        projected = projected[:projected.index(b'[[package]]\nname="test-only"')]
        selected = [{"name": "core", "version": "1.0.0"}]
        CORE.validate_projected_lock(original, projected, selected)
        for changed in (projected.replace(b'2.0.0', b'2.0.1'),
                        projected.replace(b'aaaaaaaa', b'bbbbbbbb'),
                        projected.replace(b'["external"]', b'["unreviewed"]'), original):
            with self.assertRaises(CORE.PackageError):
                CORE.validate_projected_lock(original, changed, selected)

    def test_lock_projection_accepts_cargo_unambiguous_dependency_abbreviations(self):
        original = b'''version=4
[[package]]
name="core"
version="1.0.0"
dependencies=["external 2.0.0"]
[[package]]
name="external"
version="2.0.0"
source="registry+https://example.invalid/index"
checksum="aaaaaaaa"
[[package]]
name="external"
version="1.0.0"
source="registry+https://example.invalid/index"
checksum="bbbbbbbb"
'''
        projected = original[:original.rindex(b'[[package]]')].replace(b'"external 2.0.0"', b'"external"')
        CORE.validate_projected_lock(original, projected, [{"name": "core", "version": "1.0.0"}])

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

    def export_fixture(self, externals=()):
        """Synthetic Cargo discovery only; cache/license/archive work stays real."""
        self.write("Cargo.toml", '[workspace]\nmembers=["crates/core"]\n')
        lock = 'version=4\n[[package]]\nname="core"\nversion="1.0.0"\n'
        for external in externals:
            lock += '\n[[package]]\n' + ''.join(f'{key}={json.dumps(value)}\n' for key, value in external.items())
        self.write("Cargo.lock", lock)
        self.write("crates/core/Cargo.toml", '[package]\nname="core"\nversion="1.0.0"\n')
        self.write("crates/core/src/lib.rs", "pub fn answer() -> u8 { 42 }\n")
        self.write("LICENSE.md", "Fixture license\n")
        self.write("NOTICE", "Fixture notice\n")
        package = {"id": "core", "name": "core", "version": "1.0.0", "license": "MIT", "features": {},
                   "manifest_path": str(self.root / "crates/core/Cargo.toml"), "dependencies": [],
                   "targets": [{"kind": ["lib"]}]}
        metadata = {"workspace_root": str(self.root), "packages": [package],
                    "resolve": {"nodes": [{"id": "core", "deps": []}]}}

        def command(argv, cwd):
            if argv[0] == "cargo":
                return CORE.json_bytes(metadata)
            return b"2026-01-01T00:00:00Z\n" if "show" in argv else b"abcdef123456\n"
        return command

    def strict_export(self, output, command):
        capture = io.StringIO()
        argv = ["package-core.py", "--root", str(self.root), "--cargo-home", str(self.root),
                "--strict-licenses", "--output", str(output)]
        with patch.object(CORE, "run", side_effect=command), patch.object(CORE.shutil, "which", return_value=None), \
                patch.object(sys, "argv", argv), redirect_stdout(capture):
            status = CORE.main()
        return status, json.loads(capture.getvalue())

    def archive_files(self, path):
        with tarfile.open(path, "r:gz") as archive:
            return {member.name.removeprefix("agent-router-core/"): archive.extractfile(member).read()
                    for member in archive}

    def test_strict_export_includes_all_locked_external_evidence(self):
        externals = [self.crate(), self.crate(name="other-platform", license_names=("LICENSE", "NOTICE"))]
        # Both are deliberately outside Cargo's selected graph: original lock
        # inventory must not silently omit optional/other-platform dependencies.
        command = self.export_fixture(externals)
        output = self.root / "complete.tar.gz"
        status, result = self.strict_export(output, command)
        self.assertEqual(status, 0, result)
        self.assertTrue(result["verified"])
        self.assertEqual(CORE.verify_archive(output)["sha256"], result["sha256"])
        self.assertEqual(result["evidence"]["missing_licenses"], [])
        self.assertEqual(result["evidence"]["license_texts"], "complete")
        self.assertEqual(result["evidence"]["legal_compatibility_review"], "UNKNOWN")
        self.assertEqual(result["evidence"]["security_review"], "UNKNOWN")
        files = self.archive_files(output)
        inventory = json.loads(files["LICENSE-INVENTORY.json"])["packages"]
        external_inventory = {item["name"]: item for item in inventory if item["source"] != "workspace"}
        self.assertEqual(set(external_inventory), {p["name"] for p in externals})
        for external in externals:
            record = external_inventory[external["name"]]
            self.assertEqual(record["checksum"], external["checksum"])
            self.assertEqual(record["evidence"], "checksum_verified")
            self.assertEqual(record["dependency_scopes"], ["outside_selected_workspace"])
            self.assertIn(f"| {external['name']} 1.0.0 | MIT | checksum_verified |",
                          files["THIRD_PARTY_NOTICES.md"].decode())
            for text in record["license_files"]:
                self.assertEqual(files[text["path"]], b"Test license text\n")
                self.assertEqual(CORE.digest(files[text["path"]]), text["sha256"])
        self.assertEqual(len(external_inventory["other-platform"]["license_files"]), 2)
        self.assertTrue(all(p["licenseConcluded"] == "NOASSERTION"
                            for p in json.loads(files["SBOM.spdx.json"])["packages"]))

    def test_core_readme_distinguishes_cli_and_example_python_requirements(self):
        command = self.export_fixture()
        output = self.root / "readme.tar.gz"
        status, result = self.strict_export(output, command)
        self.assertEqual(status, 0, result)
        readme = self.archive_files(output)["README.md"].decode()
        self.assertIn("The Rust routing CLI does not require Python", readme)
        self.assertIn("finite-campaign t02 verification example requires Python 3", readme)
        self.assertNotIn("Python is only used by repository release tooling", readme)

    def test_strict_export_refuses_incomplete_external_evidence_without_output(self):
        for missing in ("cache", "checksum", "declaration", "text"):
            with self.subTest(missing=missing):
                external = self.crate(license_text=missing != "text",
                                      license_declared=None if missing == "declaration" else "MIT")
                cache = self.root / "registry/cache/test/example-1.0.0.crate"
                if missing == "cache":
                    cache.unlink()
                elif missing == "checksum":
                    cache.write_bytes(cache.read_bytes() + b"changed")
                command = self.export_fixture([external])
                output = self.root / f"incomplete-{missing}.tar.gz"
                status, result = self.strict_export(output, command)
                self.assertEqual(status, 2, result)
                self.assertEqual(result["error"], "license.evidence_incomplete: 1 dependencies")
                self.assertFalse(output.exists())
                self.assertEqual(list(self.root.glob(".agent-router-package-*")), [])

    def test_strict_export_refuses_tampered_supplement_without_output(self):
        external = self.crate(license_text=False)
        command = self.export_fixture([external])
        path = "licenses/third-party/example-1.0.0/LICENSE"
        published = b"Published upstream license\n"
        self.write(path, published)
        self.write("licenses/third-party/evidence.json", CORE.json_bytes({"packages": [{
            "name": "example", "version": "1.0.0", "crate_sha256": external["checksum"],
            "files": [{"path": path, "sha256": CORE.digest(published),
                       "url": "https://example.invalid/pinned-commit/LICENSE"}]}]}))
        valid = self.root / "supplement-complete.tar.gz"
        status, result = self.strict_export(valid, command)
        self.assertEqual(status, 0, result)
        self.assertEqual(self.archive_files(valid)[path], published)
        original = valid.read_bytes()
        self.write(path, b"tampered")
        output = self.root / "supplement-tampered.tar.gz"
        status, result = self.strict_export(output, command)
        self.assertEqual(status, 2, result)
        self.assertEqual(result["error"], "license.supplement_hash_mismatch")
        self.assertFalse(output.exists())
        self.assertEqual(valid.read_bytes(), original)

    def test_strict_export_license_and_inventory_tampering_fail_verification(self):
        command = self.export_fixture([self.crate()])
        output = self.root / "untampered.tar.gz"
        status, result = self.strict_export(output, command)
        self.assertEqual(status, 0, result)
        original = self.archive_files(output)
        for name in ("licenses/dependencies/example-1.0.0/LICENSE", "LICENSE-INVENTORY.json"):
            with self.subTest(member=name):
                files = dict(original)
                files[name] += b"changed"
                tampered = self.write("tampered.tar.gz", CORE.archive_bytes(files))
                capture = io.StringIO()
                with patch.object(sys, "argv", ["package-core.py", "--verify", str(tampered)]), \
                        redirect_stdout(capture):
                    self.assertEqual(CORE.main(), 2)
                self.assertEqual(json.loads(capture.getvalue())["error"], "archive.inventory_mismatch")

    def test_complete_exports_are_identical_and_never_replace_an_artifact(self):
        command = self.export_fixture()

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
