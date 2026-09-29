#!/usr/bin/env python3
"""Behavioral tests for local release provenance verification."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import io
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/release-local.py"
OPENSSL = Path("/opt/homebrew/bin/openssl")
PACKAGE_SPEC = importlib.util.spec_from_file_location("package_binary", ROOT / "scripts/package-binary.py")
PACKAGE = importlib.util.module_from_spec(PACKAGE_SPEC)
PACKAGE_SPEC.loader.exec_module(PACKAGE)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class LocalReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="agent-router-release-test-", dir=tempfile.gettempdir())
        self.root = Path(self.temp.name)
        self.source_archive, self.source_sha, self.source_commit = self.make_source()
        self.candidate, self.candidate_sha = self.make_candidate()
        self.out = self.root / "prepared"
        self.manifest = self.out / "MANIFEST.json"
        self.signing_message = self.out / "SIGNING-MESSAGE.bin"
        self.signature = self.root / "MANIFEST.sig"
        self.public_key = self.root / "release-public.der"
        self.private_key = self.root / "test-only-private.pem"
        self.formula = self.root / "agent-router.rb"
        self.make_key_pair()

    def tearDown(self):
        self.temp.cleanup()

    def make_source(self):
        source_commit = "a" * 40
        files = {
            "Cargo.toml": b"[workspace.package]\nversion = \"0.1.0\"\n",
            "Cargo.lock": b'version = 4\n\n[[package]]\nname = "agent-router-cli"\nversion = "0.1.0"\n',
            "LICENSE-INVENTORY.json": b'{"packages":[]}\n',
            "SBOM.spdx.json": b'{"spdxVersion":"SPDX-2.3","packages":[]}\n',
            "licenses/LICENSE-MIT": b"MIT license text\n",
            "licenses/LICENSE-APACHE": b"Apache license text\n",
            "THIRD_PARTY_NOTICES.md": b"No external dependency notices in this fixture.\n",
        }
        hashes = {name: sha(data) for name, data in sorted(files.items())}
        manifest = {
            "kind": "core-source",
            "source_commit": source_commit,
            "content_sha256": sha(PACKAGE.CORE.json_bytes(hashes)),
            "files": [{"path": name, "sha256": digest} for name, digest in hashes.items()],
            "evidence": {"license_texts": "complete"},
        }
        files["DISTRIBUTION.json"] = PACKAGE.CORE.json_bytes(manifest)
        path = self.root / "agent-router-core.tar.gz"
        path.write_bytes(PACKAGE.CORE.archive_bytes(files, "core"))
        return path, sha(path.read_bytes()), source_commit

    def make_candidate(self, name="agent-router-aarch64-apple-darwin-core.tar.gz", features=None):
        source = PACKAGE.checked_archive(self.source_archive, self.source_sha)
        features = [] if features is None else features
        proof = {
            "features": features,
            "target": "aarch64-apple-darwin",
            "vendor_inventory": {},
            "build_packages": [],
            "source_sha256": self.source_sha,
            "source_manifest_sha256": sha(source["DISTRIBUTION.json"]),
            "lock_sha256": sha(source["Cargo.lock"]),
            "vendor_sha256": "c" * 64,
            "cargo_version": "cargo 1.85.1 fixture",
            "rustc_version": "rustc 1.85.1 fixture",
            "cargo_binary_sha256": "1" * 64,
            "rustc_binary_sha256": "2" * 64,
            "c_compiler": "clang fixture",
            "sdk": "macOS SDK fixture",
            "dynamic_libraries_declared": [],
        }
        path = self.root / name
        path.write_bytes(PACKAGE.archive_payload(b"fixture binary bytes", proof, source))
        return path, sha(path.read_bytes())

    def make_key_pair(self):
        self.openssl(["genpkey", "-algorithm", "ED25519", "-out", str(self.private_key)])
        self.openssl(["pkey", "-in", str(self.private_key), "-pubout", "-outform", "DER", "-out", str(self.public_key)])

    def openssl(self, args):
        result = subprocess.run([str(OPENSSL), *map(str, args)], capture_output=True, timeout=10, check=False)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
        return result.stdout

    def cli(self, args, script=SCRIPT):
        return subprocess.run([sys.executable, str(script), *map(str, args)], cwd=script.parent.parent,
                              capture_output=True, timeout=15, check=False)

    def prepare(self, candidates=None, output=None, script=SCRIPT):
        candidates = candidates or [self.candidate]
        output = self.out if output is None else output
        args = ["prepare", "--source-archive", self.source_archive, "--source-sha256", self.source_sha,
                "--source-commit", self.source_commit, "--output-dir", output]
        for candidate in candidates:
            args.extend(["--candidate", candidate])
        return self.cli(args, script)

    def sign(self, message=None, signature=None, private_key=None):
        return self.openssl(["pkeyutl", "-sign", "-rawin", "-inkey", private_key or self.private_key,
                             "-in", message or self.signing_message, "-out", signature or self.signature])

    def verify(self, *, manifest=None, signature=None, public_key=None, expected_manifest_sha=None,
               expected_candidate_sha=None, source_commit=None, formula=None, candidates=None, script=SCRIPT):
        manifest = manifest or self.manifest
        signature = signature or self.signature
        public_key = public_key or self.public_key
        formula = formula or self.formula
        candidates = candidates or [self.candidate]
        args = ["verify", "--source-archive", self.source_archive, "--source-sha256", self.source_sha,
                "--expected-source-commit", source_commit or self.source_commit,
                "--manifest", manifest, "--signature", signature, "--public-key", public_key,
                "--public-key-sha256", sha(public_key.read_bytes()),
                "--expected-manifest-sha256", expected_manifest_sha or sha(manifest.read_bytes()),
                "--expected-candidate-sha256", expected_candidate_sha or self.candidate_sha,
                "--target", "aarch64-apple-darwin", "--features-json", "[]", "--formula-output", formula]
        for candidate in candidates:
            args.extend(["--candidate", candidate])
        return self.cli(args, script)

    def make_mirror(self):
        mirror = self.root / "mirror"
        (mirror / "scripts").mkdir(parents=True)
        (mirror / "distribution/homebrew").mkdir(parents=True)
        for name in ("release-local.py", "package-binary.py", "package-core.py"):
            shutil.copy2(ROOT / "scripts" / name, mirror / "scripts" / name)
        shutil.copy2(ROOT / "distribution/homebrew/agent-router.rb.in", mirror / "distribution/homebrew/agent-router.rb.in")
        return mirror

    def assert_fails(self, result, code, formula=None):
        self.assertEqual(result.returncode, 2, (result.stdout + result.stderr).decode(errors="replace"))
        self.assertEqual(json.loads(result.stdout), {"error": code, "ok": False})
        self.assertNotIn(b"PRIVATE KEY", result.stdout + result.stderr)
        if formula is not None:
            self.assertFalse(formula.exists())

    def signed_preparation(self):
        prepared = self.prepare()
        self.assertEqual(prepared.returncode, 0, (prepared.stdout + prepared.stderr).decode(errors="replace"))
        self.assertLessEqual(self.manifest.stat().st_size, 1024 * 1024)
        self.sign()

    def test_prepare_emits_canonical_path_free_manifest_and_domain_separated_signing_bytes(self):
        result = self.prepare()
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode(errors="replace"))
        data = self.manifest.read_bytes()
        manifest = json.loads(data)
        self.assertEqual(data, PACKAGE.canonical(manifest))
        self.assertEqual(manifest["schema"], "agent-router-release-v1")
        self.assertEqual(manifest["source"]["commit"], self.source_commit)
        self.assertEqual(manifest["artifacts"][0]["archive_sha256"], self.candidate_sha)
        self.assertEqual(self.signing_message.read_bytes(), b"agent-router-release-v1\0" + data)
        self.assertNotIn(str(self.root).encode(), data)
        self.assertNotIn(b"/Users/", data)
        self.assertNotIn(b"raw_log", data)

    def test_valid_signature_pins_and_template_render_exact_selected_artifact(self):
        self.signed_preparation()
        result = self.verify()
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode(errors="replace"))
        rendered = self.formula.read_text()
        self.assertIn(f'sha256 "{self.candidate_sha}"', rendered)
        self.assertIn('# target: "aarch64-apple-darwin"', rendered)
        self.assertIn("# features: []", rendered)
        self.assertIn('bin.install "bin/agent-router"', rendered)
        self.assertFalse(self.formula.is_symlink())

    def test_candidate_manifest_signature_and_key_substitution_fail_without_formula(self):
        self.signed_preparation()
        wrong_candidate = self.verify(expected_candidate_sha="0" * 64, formula=self.root / "wrong-candidate.rb")
        self.assert_fails(wrong_candidate, "release.candidate_pin_mismatch", self.root / "wrong-candidate.rb")
        wrong_source = self.verify(source_commit="b" * 40, formula=self.root / "wrong-source.rb")
        self.assert_fails(wrong_source, "release.source_commit_mismatch", self.root / "wrong-source.rb")
        tampered = self.root / "tampered.json"
        tampered.write_bytes(self.manifest.read_bytes().replace(b"release-v1", b"release-v0", 1))
        result = self.verify(manifest=tampered, formula=self.root / "tampered.rb",
                             expected_manifest_sha=sha(self.manifest.read_bytes()))
        self.assert_fails(result, "release.manifest_pin_mismatch", self.root / "tampered.rb")

    def test_invalid_signature_and_wrong_public_key_pin_are_rejected(self):
        self.signed_preparation()
        self.signature.write_bytes(b"x" * 64)
        invalid = self.verify(formula=self.root / "bad-signature.rb")
        self.assert_fails(invalid, "release.signature_invalid", self.root / "bad-signature.rb")
        self.sign()
        bad_pin_args = ["verify", "--source-archive", self.source_archive, "--source-sha256", self.source_sha,
                        "--expected-source-commit", self.source_commit, "--manifest", self.manifest,
                        "--signature", self.signature, "--public-key", self.public_key,
                        "--public-key-sha256", "0" * 64,
                        "--expected-manifest-sha256", sha(self.manifest.read_bytes()),
                        "--expected-candidate-sha256", self.candidate_sha, "--candidate", self.candidate,
                        "--target", "aarch64-apple-darwin", "--features-json", "[]",
                        "--formula-output", self.root / "bad-key.rb"]
        pinned = self.cli(bad_pin_args)
        self.assert_fails(pinned, "release.public_key_pin_mismatch", self.root / "bad-key.rb")

    def test_duplicate_pairs_candidate_limit_and_existing_outputs_fail_closed(self):
        duplicate = self.prepare(candidates=[self.candidate, self.candidate], output=self.root / "duplicate")
        self.assert_fails(duplicate, "release.archive_name_duplicate", self.root / "duplicate")
        too_many = self.prepare(candidates=[self.candidate] * 9, output=self.root / "too-many")
        self.assert_fails(too_many, "release.artifact_count_invalid", self.root / "too-many")
        self.assertEqual(self.prepare().returncode, 0)
        overwrite = self.prepare(output=self.out)
        self.assert_fails(overwrite, "release.output_exists_or_parent_invalid")

    def test_changed_formula_template_is_rejected_after_valid_signature(self):
        self.signed_preparation()
        mirror = self.make_mirror()
        template = mirror / "distribution/homebrew/agent-router.rb.in"
        template.write_bytes(template.read_bytes() + b"# edited after approval\n")
        result = self.verify(formula=self.root / "tampered-template.rb", script=mirror / "scripts/release-local.py")
        self.assert_fails(result, "release.formula_template_hash_mismatch", self.root / "tampered-template.rb")

    def test_manifest_altered_with_recomputed_pin_fails_signature(self):
        self.signed_preparation()
        data = self.manifest.read_bytes()
        marker = b'"formula_sha256":"'
        index = data.index(marker) + len(marker)
        flipped = b"0" if data[index:index + 1] != b"0" else b"1"
        altered = self.root / "altered-manifest.json"
        altered.write_bytes(data[:index] + flipped + data[index + 1:])
        result = self.verify(manifest=altered, formula=self.root / "altered.rb")
        self.assert_fails(result, "release.signature_invalid", self.root / "altered.rb")

    def test_altered_archive_fails_against_signed_hash(self):
        self.signed_preparation()
        altered = self.root / "altered" / self.candidate.name
        altered.parent.mkdir()
        altered.write_bytes(self.candidate.read_bytes() + b"\0")
        result = self.verify(candidates=[altered], formula=self.root / "archive.rb",
                             expected_candidate_sha=sha(altered.read_bytes()))
        self.assert_fails(result, "release.candidate_sha256_mismatch", self.root / "archive.rb")

    def test_substituted_public_key_fails_signature_even_with_matching_pin(self):
        self.signed_preparation()
        other_private, other_public = self.root / "other.pem", self.root / "other.der"
        self.openssl(["genpkey", "-algorithm", "ED25519", "-out", other_private])
        self.openssl(["pkey", "-in", other_private, "-pubout", "-outform", "DER", "-out", other_public])
        result = self.verify(public_key=other_public, formula=self.root / "other-key.rb")
        self.assert_fails(result, "release.signature_invalid", self.root / "other-key.rb")

    def test_signature_over_other_message_is_rejected(self):
        self.signed_preparation()
        other = self.root / "other-message.bin"
        other.write_bytes(self.signing_message.read_bytes() + b"x")
        self.sign(message=other)
        result = self.verify(formula=self.root / "other-message.rb")
        self.assert_fails(result, "release.signature_invalid", self.root / "other-message.rb")

    def test_formula_output_is_never_overwritten(self):
        self.signed_preparation()
        self.formula.write_text("existing")
        result = self.verify()
        self.assert_fails(result, "release.output_exists_or_parent_invalid")
        self.assertEqual(self.formula.read_text(), "existing")

    def test_formula_hash_does_not_depend_on_candidate_location(self):
        self.signed_preparation()
        moved = self.root / "moved"
        moved.mkdir()
        target = moved / self.candidate.name
        shutil.move(self.candidate, target)
        result = self.verify(candidates=[target])
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode(errors="replace"))
        rendered = self.formula.read_text()
        self.assertIn(target.resolve().as_uri(), rendered)
        self.assertNotIn("agent-router-release-archive", rendered)

    def test_truncated_candidate_archive_reports_json_error_without_path(self):
        data = self.candidate.read_bytes()
        broken = self.root / "agent-router-broken-core.tar.gz"
        broken.write_bytes(data[: len(data) // 2])
        result = self.prepare(candidates=[broken], output=self.root / "truncated")
        self.assert_fails(result, "release.input_invalid", self.root / "truncated")
        self.assertNotIn(b"Traceback", result.stderr)
        self.assertNotIn(str(self.root).encode(), result.stdout + result.stderr)

    def test_candidate_with_non_object_inventory_json_reports_json_error(self):
        members = {}
        with tarfile.open(self.candidate, "r:gz") as archive:
            for member in archive:
                members[member] = archive.extractfile(member).read()
        replaced = {}
        for member, data in members.items():
            if member.name.endswith("/LICENSE-INVENTORY.json"):
                data = b"[]"
            replaced[member] = data
        integrity = {member.name.partition("/")[2]: sha(data) for member, data in replaced.items()
                     if not member.name.endswith("/INTEGRITY.json")}
        odd = self.root / "agent-router-odd-core.tar.gz"
        with tarfile.open(odd, "w:gz") as archive:
            for member, data in replaced.items():
                if member.name.endswith("/INTEGRITY.json"):
                    data = json.dumps(integrity).encode()
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        result = self.prepare(candidates=[odd], output=self.root / "odd")
        self.assert_fails(result, "release.input_invalid", self.root / "odd")
        self.assertNotIn(b"Traceback", result.stderr)

    def test_openssl_version_must_match_exactly(self):
        self.signed_preparation()
        mirror = self.make_mirror()
        script = mirror / "scripts/release-local.py"
        text = script.read_text()
        self.assertIn('("OpenSSL", "3.6.4")', text)
        script.write_text(text.replace('("OpenSSL", "3.6.4")', '("OpenSSL", "3.6")'))
        result = self.verify(formula=self.root / "version.rb", script=script)
        self.assert_fails(result, "release.openssl_version_mismatch", self.root / "version.rb")

    def test_unrecognised_openssl_verdict_is_an_openssl_failure_not_a_signature_verdict(self):
        self.signed_preparation()
        mirror = self.make_mirror()
        stub = mirror / "openssl-stub"
        stub.write_text("#!/bin/sh\nif [ \"$1\" = version ]; then echo 'OpenSSL 3.6.4 stub'; exit 0; fi\n"
                        "echo 'Error loading key' >&2\nexit 1\n")
        stub.chmod(0o755)
        script = mirror / "scripts/release-local.py"
        text = script.read_text()
        self.assertIn('OPENSSL = Path("/opt/homebrew/bin/openssl")', text)
        script.write_text(text.replace('OPENSSL = Path("/opt/homebrew/bin/openssl")', f'OPENSSL = Path("{stub}")'))
        result = self.verify(formula=self.root / "stub.rb", script=script)
        self.assert_fails(result, "release.openssl_failed", self.root / "stub.rb")

    def test_missing_candidate_for_manifest_entry_is_rejected(self):
        self.signed_preparation()
        second, _ = self.make_candidate(name="agent-router-second-core.tar.gz")
        result = self.verify(candidates=[second], formula=self.root / "missing.rb")
        self.assert_fails(result, "release.candidate_set_mismatch", self.root / "missing.rb")


if __name__ == "__main__":
    unittest.main(verbosity=2)
