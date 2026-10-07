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
            "LICENSE.md": b"Project license text\n",
            "NOTICE": b"Project notice text\n",
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

    def make_candidate(self, name="agent-router-aarch64-apple-darwin-core.tar.gz", features=None, versions=None):
        source = PACKAGE.checked_archive(self.source_archive, self.source_sha)
        features = ["claude", "codex"] if features is None else features
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
            "dynamic_libraries_declared": {"agent-router": [], "router-sandbox-helper": []},
            "builds": [{"binaries_sha256": {"agent-router": sha(b"fixture binary bytes"),
                       "router-sandbox-helper": sha(b"fixture helper bytes")}}] * 2,
        }
        if versions is not None:
            proof.update(versions)
        path = self.root / name
        path.write_bytes(PACKAGE.archive_payload({"agent-router": b"fixture binary bytes", "router-sandbox-helper": b"fixture helper bytes"}, proof, source))
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
               expected_candidate_sha=None, source_commit=None, formula=None, candidates=None, archive_url=None, script=SCRIPT):
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
                "--target", "aarch64-apple-darwin", "--features-json", '["claude","codex"]', "--formula-output", formula]
        if archive_url is not None:
            args.extend(["--archive-url", archive_url])
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
        self.assertEqual(manifest["schema"], "agent-router-release-v2")
        self.assertEqual(manifest["source"]["commit"], self.source_commit)
        self.assertEqual(manifest["artifacts"][0]["archive_sha256"], self.candidate_sha)
        self.assertEqual(self.signing_message.read_bytes(), b"agent-router-release-v2\0" + data)
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
        self.assertIn('# features: ["claude","codex"]', rendered)
        self.assertIn('bin.install "bin/agent-router", "bin/router-sandbox-helper"', rendered)
        self.assertFalse(self.formula.is_symlink())

    def test_verbose_toolchain_observations_prepare_sign_and_verify(self):
        # Synthetic harness fixture matches the real package-binary -Vv format.
        versions = {
            "cargo_version": "cargo 1.85.1 (d73d2caf9 2024-12-31)\nrelease: 1.85.1\nhost: aarch64-apple-darwin\n",
            "rustc_version": "rustc 1.85.1 (4eb161250 2025-03-15)\nbinary: rustc\nrelease: 1.85.1\nLLVM version: 19.1.7\n",
        }
        self.candidate, self.candidate_sha = self.make_candidate(versions=versions)
        self.signed_preparation()
        manifest = json.loads(self.manifest.read_bytes())
        record = manifest["artifacts"][0]
        for name, value in versions.items():
            self.assertEqual(record["toolchain"][name], value.partition("\n")[0])
        archive = PACKAGE.verify_binary_archive(self.candidate.read_bytes())
        self.assertEqual(record["build_observations_sha256"], sha(archive["BUILD-OBSERVATIONS.json"]))
        self.assertEqual(self.verify().returncode, 0)

    def test_verbose_toolchain_rejects_private_tail_controls_and_oversized_values(self):
        invalid = [
            "", "\ncargo 1.85.1", "cargo 1.85.1\r\nrelease: 1.85.1",
            "cargo 1.85.1\nsecret\0value", "cargo 1.85.1\n" + "x" * 4096,
            "cargo 1.85.1\npath: /Users/tester/private",
            "cargo 1.85.1\npath: /private/tmp/build",
        ]
        for value in invalid:
            with self.subTest(value=value):
                self.candidate, self.candidate_sha = self.make_candidate(versions={"cargo_version": value})
                self.assert_fails(self.prepare(), "release.toolchain_version_invalid", self.out)

    def test_candidate_manifest_signature_and_key_substitution_fail_without_formula(self):
        self.signed_preparation()
        wrong_candidate = self.verify(expected_candidate_sha="0" * 64, formula=self.root / "wrong-candidate.rb")
        self.assert_fails(wrong_candidate, "release.candidate_pin_mismatch", self.root / "wrong-candidate.rb")
        wrong_source = self.verify(source_commit="b" * 40, formula=self.root / "wrong-source.rb")
        self.assert_fails(wrong_source, "release.source_commit_mismatch", self.root / "wrong-source.rb")
        tampered = self.root / "tampered.json"
        tampered.write_bytes(self.manifest.read_bytes().replace(b"release-v2", b"release-v0", 1))
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
                        "--target", "aarch64-apple-darwin", "--features-json", '["claude","codex"]',
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
        self.assertIn('OPENSSL = Path("/opt/homebrew/bin/openssl")', text)
        for version in ("3.6.4", "3.6", "3.6.50", "3.6.6"):
            with self.subTest(version=version):
                stub = mirror / f"openssl-{version}"
                # A permissive verifier must never be reached through an unpinned version.
                stub.write_text(f"#!/bin/sh\nif [ \"$1\" = version ]; then echo 'OpenSSL {version} stub'; exit 0; fi\n"
                                "echo 'Signature Verified Successfully'\nexit 0\n")
                stub.chmod(0o755)
                script.write_text(text.replace('OPENSSL = Path("/opt/homebrew/bin/openssl")', f'OPENSSL = Path("{stub}")'))
                formula = self.root / f"version-{version}.rb"
                result = self.verify(formula=formula, script=script)
                self.assert_fails(result, "release.openssl_version_mismatch", formula)

    def test_unrecognised_openssl_verdict_is_an_openssl_failure_not_a_signature_verdict(self):
        self.signed_preparation()
        mirror = self.make_mirror()
        stub = mirror / "openssl-stub"
        stub.write_text("#!/bin/sh\nif [ \"$1\" = version ]; then echo 'OpenSSL 3.6.5 stub'; exit 0; fi\n"
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

    def mutate_candidate(self, transform):
        files = PACKAGE.verify_binary_archive(self.candidate.read_bytes())
        transform(files)
        files["INTEGRITY.json"] = PACKAGE.canonical({name: sha(data) for name, data in sorted(files.items())
                                                    if name != "INTEGRITY.json"})
        with tarfile.open(self.candidate, "w:gz") as archive:
            for name, data in sorted(files.items()):
                member = tarfile.TarInfo("agent-router-binary/" + name)
                member.size = len(data)
                member.mode = 0o755 if name.startswith("bin/") else 0o644
                archive.addfile(member, io.BytesIO(data))
        self.candidate_sha = sha(self.candidate.read_bytes())

    def test_both_binary_hashes_are_signed_and_revalidated(self):
        self.signed_preparation()
        record = json.loads(self.manifest.read_bytes())["artifacts"][0]
        self.assertEqual(record["binaries_sha256"], {"agent-router": sha(b"fixture binary bytes"),
                                                   "router-sandbox-helper": sha(b"fixture helper bytes")})
        for omit in (False, True):
            with self.subTest(omit=omit):
                manifest = json.loads(self.manifest.read_bytes())
                hashes = manifest["artifacts"][0]["binaries_sha256"]
                if omit:
                    del hashes["router-sandbox-helper"]
                else:
                    hashes["router-sandbox-helper"] = "0" * 64
                altered = self.root / f"helper-{omit}.json"
                altered.write_bytes(PACKAGE.canonical(manifest))
                message = self.root / f"helper-{omit}.bin"
                message.write_bytes(b"agent-router-release-v2\0" + altered.read_bytes())
                self.sign(message=message)
                result = self.verify(manifest=altered)
                self.assert_fails(result, "release.binary_evidence_mismatch" if omit else "release.candidate_metadata_mismatch", self.formula)

    def test_missing_changed_or_incomplete_helper_evidence_is_rejected(self):
        original = self.candidate.read_bytes()
        def observation_mutation(files, mode):
            observations = json.loads(files["BUILD-OBSERVATIONS.json"])
            if mode == "omitted":
                del observations["builds"][1]["binaries_sha256"]["router-sandbox-helper"]
            elif mode == "changed":
                observations["builds"][1]["binaries_sha256"]["router-sandbox-helper"] = "0" * 64
            elif mode == "one-build":
                observations["builds"].pop()
            elif mode == "v1":
                observations["schema"] = "agent-router-build-observations-v1"
            files["BUILD-OBSERVATIONS.json"] = PACKAGE.canonical(observations)
        for mode in ("omitted", "changed", "one-build", "v1", "helper-bytes", "helper-missing"):
            with self.subTest(mode=mode):
                self.candidate.write_bytes(original)
                def mutate(files):
                    if mode == "helper-bytes":
                        files["bin/router-sandbox-helper"] = b"substituted helper"
                    elif mode == "helper-missing":
                        del files["bin/router-sandbox-helper"]
                    else:
                        observation_mutation(files, mode)
                self.mutate_candidate(mutate)
                self.assert_fails(self.prepare(), "release.candidate_archive_invalid", self.out)

    def test_v1_manifest_and_v1_domain_cannot_be_used_as_v2(self):
        self.signed_preparation()
        v1 = json.loads(self.manifest.read_bytes())
        v1["schema"] = "agent-router-release-v1"
        old = self.root / "v1.json"
        old.write_bytes(PACKAGE.canonical(v1))
        self.assert_fails(self.verify(manifest=old), "release.manifest_schema_invalid", self.formula)
        old_message = self.root / "old-domain.bin"
        old_message.write_bytes(b"agent-router-release-v1\0" + self.manifest.read_bytes())
        self.sign(message=old_message)
        self.assert_fails(self.verify(), "release.signature_invalid", self.formula)

    def test_formula_os_and_cpu_guards_run_before_any_install(self):
        if shutil.which("ruby") is None:
            self.skipTest("Ruby unavailable for Homebrew DSL fixture")
        self.signed_preparation()
        self.assertEqual(self.verify().returncode, 0)
        stub = self.root / "formula-harness.rb"
        stub.write_text('''require "json"
module OS
  def self.mac?; ENV.fetch("TEST_OS") == "macos"; end
  def self.linux?; ENV.fetch("TEST_OS") == "linux"; end
end
module Hardware
  module CPU
    def self.arm?; ENV.fetch("TEST_CPU") == "arm"; end
    def self.intel?; ENV.fetch("TEST_CPU") == "intel"; end
    def self.is_64_bit?; ENV.fetch("TEST_BITS") == "64"; end
  end
end
$installs = []
class Destination
  def install(*paths); $installs.concat(paths); end
end
class Formula
  def self.desc(*); end
  def self.homepage(*); end
  def self.url(*); end
  def self.sha256(*); end
  def self.version(*); end
  def self.test(&block); end
  def bin; Destination.new; end
  def doc; Destination.new; end
  def odie(message); raise message; end
end
load ARGV.fetch(0)
begin
  AgentRouter.new.install
  puts JSON.generate({installed: $installs})
rescue => error
  puts JSON.generate({error: error.message, installed: $installs})
  exit 1
end
''')
        source = self.formula.read_text()
        targets = {"aarch64-apple-darwin": ("macos", "arm"), "x86_64-apple-darwin": ("macos", "intel"),
                   "aarch64-unknown-linux-gnu": ("linux", "arm"), "x86_64-unknown-linux-gnu": ("linux", "intel")}
        for target, (expected_os, expected_cpu) in targets.items():
            rendered = self.root / f"formula-{target}.rb"
            rendered.write_text(source.replace("aarch64-apple-darwin", target))
            for os_name, cpu, bits in ((expected_os, expected_cpu, "64"),
                                       ("linux" if expected_os == "macos" else "macos", expected_cpu, "64"),
                                       (expected_os, "intel" if expected_cpu == "arm" else "arm", "64"),
                                       (expected_os, expected_cpu, "32")):
                with self.subTest(target=target, os=os_name, cpu=cpu, bits=bits):
                    result = subprocess.run(["ruby", str(stub), str(rendered)], cwd=self.root, capture_output=True,
                                            timeout=10, env={**os.environ, "TEST_OS": os_name, "TEST_CPU": cpu, "TEST_BITS": bits})
                    observed = json.loads(result.stdout)
                    compatible = (os_name, cpu, bits) == (expected_os, expected_cpu, "64")
                    self.assertEqual(result.returncode, 0 if compatible else 1, result.stderr)
                    if compatible:
                        self.assertIn("bin/agent-router", observed["installed"])
                        self.assertIn("bin/router-sandbox-helper", observed["installed"])
                    else:
                        self.assertEqual(observed["installed"], [])
                        self.assertIn(target, observed["error"])


    def test_unsupported_target_and_incomplete_feature_set_are_refused(self):
        original = self.candidate.read_bytes()
        for target in ("x86_64-pc-windows-msvc", "x86_64-unknown-freebsd", "aarch64-unknown-linux-musl"):
            with self.subTest(target=target):
                self.candidate.write_bytes(original)
                def mutate(files):
                    for name in ("BUILD-RECIPE.json", "BUILD-OBSERVATIONS.json"):
                        document = json.loads(files[name])
                        document["target"] = target
                        files[name] = PACKAGE.canonical(document)
                self.mutate_candidate(mutate)
                self.assert_fails(self.prepare(), "release.target_invalid", self.out)
        self.candidate, self.candidate_sha = self.make_candidate(features=[])
        self.assert_fails(self.prepare(), "release.candidate_metadata_invalid", self.out)

    def test_mono_binary_manifest_cannot_claim_v2(self):
        self.signed_preparation()
        manifest = json.loads(self.manifest.read_bytes())
        record = manifest["artifacts"][0]
        record["binary_sha256"] = record.pop("binaries_sha256")["agent-router"]
        altered = self.root / "mono-v2.json"
        altered.write_bytes(PACKAGE.canonical(manifest))
        self.assert_fails(self.verify(manifest=altered), "release.artifact_record_invalid", self.formula)


    def test_public_formula_is_explicit_and_keeps_signed_content_pins(self):
        self.signed_preparation()
        local = self.verify()
        self.assertEqual(local.returncode, 0, local.stderr)
        remote_formula = self.root / "public.rb"
        url = f"https://github.com/FlorianBruniaux/agent-router/releases/download/v0.1.0/{self.candidate.name}"
        remote = self.verify(formula=remote_formula, archive_url=url)
        self.assertEqual(remote.returncode, 0, remote.stderr)
        self.assertIn(f'url "{url}"', remote_formula.read_text())
        self.assertEqual(remote_formula.read_text(), self.formula.read_text().replace(self.candidate.resolve().as_uri(), url))
        local_result, remote_result = json.loads(local.stdout), json.loads(remote.stdout)
        self.assertEqual(local_result["neutral_formula_sha256"], remote_result["neutral_formula_sha256"])
        self.assertEqual(local_result["selected_archive_sha256"], remote_result["selected_archive_sha256"])
        self.assertNotEqual(local_result["formula_sha256"], remote_result["formula_sha256"])
        self.signature.write_bytes(b"x" * 64)
        self.assert_fails(self.verify(formula=self.root / "bad-public.rb", archive_url=url),
                          "release.signature_invalid", self.root / "bad-public.rb")

    def test_public_formula_refuses_ambiguous_or_unapproved_urls(self):
        self.signed_preparation()
        url = f"https://github.com/FlorianBruniaux/agent-router/releases/download/v0.1.0/{self.candidate.name}"
        invalid = [url.replace("https://", "http://"), url.replace("github.com/", "github.com.evil.test/"),
                   url.replace("github.com/", "github.com:443/"), url.replace("github.com/", "user@github.com/"),
                   url.replace("FlorianBruniaux", "another-owner"), url.replace("agent-router/releases", "other-repo/releases"),
                   url.replace("v0.1.0", "v0.1.1"), url + "?download=1", url + "#fragment", url + "/suffix",
                   url.replace("/v0.1.0/", "/v0.1.0/../v0.1.0/"), url.replace("/v0.1.0/", "/%760.1.0/"),
                   url.replace(self.candidate.name, "other.tar.gz"), self.candidate.resolve().as_uri()]
        for value in invalid:
            with self.subTest(url=value):
                self.assert_fails(self.verify(archive_url=value), "release.archive_url_invalid", self.formula)


if __name__ == "__main__":
    unittest.main(verbosity=2)
