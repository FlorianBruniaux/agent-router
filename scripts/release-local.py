#!/usr/bin/env python3
"""Prepare and verify a locally signed release attestation; never signs or publishes."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import tarfile
import tempfile
import tomllib
import zlib

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent
PACKAGE_SPEC = importlib.util.spec_from_file_location("package_binary", SCRIPT_DIR / "package-binary.py")
PACKAGE = importlib.util.module_from_spec(PACKAGE_SPEC)
PACKAGE_SPEC.loader.exec_module(PACKAGE)

SCHEMA = "agent-router-release-v2"
DOMAIN = b"agent-router-release-v2\0"
MAX_MANIFEST = 1024 * 1024
MAX_ARTIFACTS = 8
MAX_SIGNATURE = 64
MAX_PUBLIC_KEY = 44
ED25519_SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")
OPENSSL = Path("/opt/homebrew/bin/openssl")
OPENSSL_VERSION = ("OpenSSL", "3.6.5")
NEUTRAL_ARCHIVE_URL = "file:///agent-router-release-archive/"
TEMPLATE = ROOT / "distribution/homebrew/agent-router.rb.in"
ARCHIVE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._+-]{0,120}\.tar\.gz$")
TARGET_NAME = re.compile(r"^[a-zA-Z0-9_]+(?:-[a-zA-Z0-9_]+){2,3}$")
FEATURE_NAME = re.compile(r"^[a-z][a-z0-9_-]{0,63}$")
COMMIT = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")
HEX_SHA = re.compile(r"^[0-9a-f]{64}$")
RELEASE_FEATURES = ["claude", "codex"]
SUPPORTED_TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin",
                     "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"}
BINARY_RECIPES = [
    {"package": "agent-router-cli", "binary": "agent-router", "path": "bin/agent-router", "features": RELEASE_FEATURES},
    {"package": "router-sandbox-helper", "binary": "router-sandbox-helper", "path": "bin/router-sandbox-helper", "features": []},
]
BINARY_NAMES = {item["binary"] for item in BINARY_RECIPES}
TEMPLATE_FIELDS = ("ARCHIVE_URL_JSON", "ARCHIVE_SHA256", "VERSION_JSON", "TARGET_JSON", "FEATURES_JSON", "TARGET_GUARD_JSON")


class ReleaseError(Exception):
    pass


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return PACKAGE.canonical(value)


def fail(code: str) -> None:
    raise ReleaseError(code)


def read_regular(path: Path, limit: int, code: str) -> bytes:
    try:
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(path, flags)
        try:
            info = os.fstat(descriptor)
            if not stat.S_ISREG(info.st_mode) or info.st_size > limit or info.st_size < 0:
                fail(code)
            chunks, remaining = [], limit + 1
            while remaining:
                chunk = os.read(descriptor, min(65536, remaining))
                if not chunk:
                    break
                chunks.append(chunk)
                remaining -= len(chunk)
            data = b"".join(chunks)
            if len(data) > limit or len(data) != info.st_size:
                fail(code)
            return data
        finally:
            os.close(descriptor)
    except ReleaseError:
        raise
    except OSError as error:
        raise ReleaseError(code) from error


def strict_json(data: bytes, code: str, limit: int = MAX_MANIFEST, canonical_required: bool = True) -> dict:
    if len(data) > limit:
        fail(code)

    def object_without_duplicates(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                fail(code)
            result[key] = value
        return result

    try:
        value = json.loads(data, object_pairs_hook=object_without_duplicates)
    except ReleaseError:
        raise
    except (ValueError, UnicodeError, TypeError) as error:
        raise ReleaseError(code) from error
    if not isinstance(value, dict) or (canonical_required and canonical(value) != data):
        fail(code)
    return value


def require_sha(value: object, code: str) -> str:
    if not isinstance(value, str) or not HEX_SHA.fullmatch(value):
        fail(code)
    return value


def require_commit(value: object, code: str) -> str:
    if not isinstance(value, str) or not COMMIT.fullmatch(value):
        fail(code)
    return value


def safe_archive_name(value: object) -> str:
    if not isinstance(value, str) or not ARCHIVE_NAME.fullmatch(value) or Path(value).name != value:
        fail("release.archive_name_invalid")
    return value


def validate_features(value: object) -> list[str]:
    if (not isinstance(value, list) or len(value) > 64
            or any(not isinstance(item, str) or not FEATURE_NAME.fullmatch(item) for item in value)
            or value != sorted(set(value))):
        fail("release.features_invalid")
    return value


def load_source(path: Path, expected_sha: str, expected_commit: str) -> tuple[dict[str, bytes], dict]:
    require_sha(expected_sha, "release.source_sha256_invalid")
    require_commit(expected_commit, "release.source_commit_invalid")
    try:
        files = PACKAGE.checked_archive(path, expected_sha)
    except PACKAGE.PackageError as error:
        raise ReleaseError(str(error)) from error
    distribution = strict_json(files.get("DISTRIBUTION.json", b""), "release.source_manifest_invalid",
                               canonical_required=False)
    if distribution.get("kind") != "core-source" or distribution.get("source_commit") != expected_commit:
        fail("release.source_commit_mismatch")
    for name in ("Cargo.toml", "Cargo.lock", "LICENSE-INVENTORY.json", "THIRD_PARTY_NOTICES.md", "SBOM.spdx.json"):
        if name not in files:
            fail("release.source_evidence_missing")
    inventory = strict_json(files["LICENSE-INVENTORY.json"], "release.license_inventory_invalid",
                            canonical_required=False)
    if not isinstance(inventory.get("packages"), list):
        fail("release.license_inventory_invalid")
    try:
        cargo = tomllib.loads(files["Cargo.toml"].decode("utf-8"))
        version = cargo.get("workspace", {}).get("package", {}).get("version")
    except (ValueError, UnicodeError) as error:
        raise ReleaseError("release.source_manifest_invalid") from error
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        fail("release.source_version_invalid")
    return files, {"distribution": distribution, "version": version}


def read_template() -> bytes:
    try:
        if TEMPLATE.is_symlink() or not TEMPLATE.is_file() or TEMPLATE.stat().st_size > 65536:
            fail("release.formula_template_invalid")
        return TEMPLATE.read_bytes()
    except ReleaseError:
        raise
    except OSError as error:
        raise ReleaseError("release.formula_template_invalid") from error


def archive_uri(archive_path: Path) -> str:
    try:
        return archive_path.resolve(strict=True).as_uri()
    except (OSError, ValueError) as error:
        raise ReleaseError("release.candidate_invalid") from error


def selected_archive_uri(path: Path, version: str, requested_url: str | None) -> str:
    if requested_url is None:
        return archive_uri(path)
    # The signature binds content and the neutral formula, not transport origin
    # or remote availability. Only this canonical public candidate URL is allowed.
    expected = f"https://github.com/FlorianBruniaux/agent-router/releases/download/v{version}/{safe_archive_name(path.name)}"
    if requested_url != expected:
        fail("release.archive_url_invalid")
    return requested_url


def render_formula(template: bytes, archive_url: str, archive_sha: str, version: str,
                   target: str, features: list[str]) -> bytes:
    try:
        text = template.decode("utf-8")
    except UnicodeError as error:
        raise ReleaseError("release.formula_template_invalid") from error
    placeholders = re.findall(r"\{\{([^{}]+)\}\}", text)
    if (len(placeholders) != len(TEMPLATE_FIELDS) or set(placeholders) != set(TEMPLATE_FIELDS)
            or any(placeholders.count(name) != 1 for name in TEMPLATE_FIELDS)):
        fail("release.formula_template_invalid")
    replacements = {
        "{{ARCHIVE_URL_JSON}}": json.dumps(archive_url, ensure_ascii=True),
        "{{ARCHIVE_SHA256}}": archive_sha,
        "{{VERSION_JSON}}": json.dumps(version),
        "{{TARGET_JSON}}": json.dumps(target),
        "{{TARGET_GUARD_JSON}}": json.dumps(target),
        "{{FEATURES_JSON}}": json.dumps(features, separators=(",", ":")),
    }
    for name, replacement in replacements.items():
        text = text.replace(name, replacement)
    return text.encode("utf-8")


def validate_candidate(path: Path, source: dict[str, bytes], source_meta: dict,
                       source_sha: str, archive_payload: bytes) -> dict:
    try:
        archive_files = PACKAGE.verify_binary_archive(archive_payload)
    except (PACKAGE.PackageError, ValueError, KeyError, TypeError) as error:
        raise ReleaseError("release.candidate_archive_invalid") from error
    recipe = strict_json(archive_files["BUILD-RECIPE.json"], "release.build_recipe_invalid", 65536)
    observations = strict_json(archive_files["BUILD-OBSERVATIONS.json"], "release.build_observations_invalid", 65536)
    build_sbom = strict_json(archive_files["BUILD-SBOM.json"], "release.build_sbom_invalid", 1024 * 1024)
    if (recipe.get("schema") != "agent-router-build-recipe-v2"
            or recipe.get("binaries") != BINARY_RECIPES or recipe.get("profile") != "release"
            or observations.get("schema") != "agent-router-build-observations-v2"
            or build_sbom.get("schema") != "agent-router-build-sbom-v1"):
        fail("release.candidate_metadata_invalid")
    target = recipe.get("target")
    if not isinstance(target, str) or target not in SUPPORTED_TARGETS:
        fail("release.target_invalid")
    features = validate_features(recipe.get("features"))
    if features != RELEASE_FEATURES:
        fail("release.features_invalid")
    binary_hashes = {item["binary"]: sha(archive_files[item["path"]]) for item in BINARY_RECIPES}
    expected_binaries = [dict(item, sha256=binary_hashes[item["binary"]]) for item in BINARY_RECIPES]
    if observations.get("binaries") != expected_binaries:
        fail("release.binary_evidence_mismatch")
    if observations.get("builds") != [{"binaries_sha256": binary_hashes}] * 2:
        fail("release.two_build_evidence_missing")
    if (observations.get("target") != target or observations.get("profile") != "release"
            or validate_features(observations.get("features")) != features):
        fail("release.build_configuration_mismatch")
    if (observations.get("independent_builds") != 2 or observations.get("binary_bytes_equal") is not True
            or observations.get("normalized_archive_bytes_equal") is not True):
        fail("release.two_build_evidence_missing")
    distribution_sha = sha(source["DISTRIBUTION.json"])
    lock_sha = sha(source["Cargo.lock"])
    if (observations.get("source_archive_sha256") != source_sha
            or observations.get("source_manifest_sha256") != distribution_sha
            or observations.get("cargo_lock_sha256") != lock_sha):
        fail("release.source_evidence_mismatch")
    if (archive_files["LICENSE-INVENTORY.json"] != source["LICENSE-INVENTORY.json"]
            or archive_files["THIRD_PARTY_NOTICES.md"] != source["THIRD_PARTY_NOTICES.md"]
            or archive_files.get("SOURCE-SBOM.spdx.json") != source["SBOM.spdx.json"]):
        fail("release.license_or_sbom_mismatch")
    for field in ("cargo_binary_sha256", "rustc_binary_sha256"):
        require_sha(observations.get(field), "release.toolchain_hash_invalid")
    toolchain_versions = {}
    for field in ("cargo_version", "rustc_version"):
        value = observations.get(field)
        if (not isinstance(value, str) or not value.partition("\n")[0] or len(value.encode()) > 4096
                or any(c in value for c in "\0\r")
                or any(prefix in value for prefix in ("/Users/", "/private/", "/tmp/", "/home/", "/opt/homebrew/"))):
            fail("release.toolchain_version_invalid")
        # The packager records -Vv output. Keep the manifest label on one line;
        # build_observations_sha256 still binds the complete validated output.
        toolchain_versions[field] = value.partition("\n")[0]
    return {
        "archive": safe_archive_name(path.name),
        "archive_sha256": sha(archive_payload),
        "binaries_sha256": binary_hashes,
        "target": target,
        "features": features,
        "profile": "release",
        "source_commit": source_meta["distribution"]["source_commit"],
        "source_archive_sha256": source_sha,
        "source_manifest_sha256": distribution_sha,
        "cargo_lock_sha256": lock_sha,
        "license_inventory_sha256": sha(source["LICENSE-INVENTORY.json"]),
        "notices_sha256": sha(source["THIRD_PARTY_NOTICES.md"]),
        "source_sbom_sha256": sha(source["SBOM.spdx.json"]),
        "build_sbom_sha256": sha(archive_files["BUILD-SBOM.json"]),
        "build_observations_sha256": sha(archive_files["BUILD-OBSERVATIONS.json"]),
        "build_evidence": {"independent_builds": 2, "binary_bytes_equal": True,
                           "normalized_archive_bytes_equal": True},
        "toolchain": {"cargo_version": toolchain_versions["cargo_version"],
                      "cargo_binary_sha256": observations["cargo_binary_sha256"],
                      "rustc_version": toolchain_versions["rustc_version"],
                      "rustc_binary_sha256": observations["rustc_binary_sha256"]},
    }


def check_manifest_shape(manifest: dict) -> None:
    if set(manifest) != {"schema", "source", "formula_template_sha256", "artifacts"} or manifest.get("schema") != SCHEMA:
        fail("release.manifest_schema_invalid")
    source = manifest["source"]
    source_fields = {"commit", "archive_sha256", "distribution_manifest_sha256", "cargo_lock_sha256",
                     "license_inventory_sha256", "notices_sha256", "source_sbom_sha256"}
    if not isinstance(source, dict) or set(source) != source_fields:
        fail("release.source_record_invalid")
    require_commit(source["commit"], "release.source_record_invalid")
    for field in source_fields - {"commit"}:
        require_sha(source[field], "release.source_record_invalid")
    require_sha(manifest["formula_template_sha256"], "release.formula_template_hash_invalid")
    artifacts = manifest["artifacts"]
    if not isinstance(artifacts, list) or not 1 <= len(artifacts) <= MAX_ARTIFACTS:
        fail("release.artifact_count_invalid")
    names, pairs = set(), set()
    fields = {"archive", "archive_sha256", "binaries_sha256", "target", "features", "profile",
              "source_commit", "source_archive_sha256", "source_manifest_sha256", "cargo_lock_sha256",
              "license_inventory_sha256", "notices_sha256", "source_sbom_sha256", "build_sbom_sha256",
              "build_observations_sha256", "build_evidence", "toolchain", "formula_sha256"}
    for item in artifacts:
        if not isinstance(item, dict) or set(item) != fields:
            fail("release.artifact_record_invalid")
        name = safe_archive_name(item["archive"])
        if name in names:
            fail("release.archive_name_duplicate")
        names.add(name)
        target = item["target"]
        if not isinstance(target, str) or target not in SUPPORTED_TARGETS:
            fail("release.target_invalid")
        features = validate_features(item["features"])
        if features != RELEASE_FEATURES:
            fail("release.features_invalid")
        binary_hashes = item["binaries_sha256"]
        if not isinstance(binary_hashes, dict) or set(binary_hashes) != BINARY_NAMES:
            fail("release.binary_evidence_mismatch")
        for digest in binary_hashes.values():
            require_sha(digest, "release.binary_evidence_mismatch")
        pair = target, tuple(features)
        if pair in pairs:
            fail("release.artifact_pair_duplicate")
        pairs.add(pair)
        if item["profile"] != "release":
            fail("release.profile_invalid")
        require_commit(item["source_commit"], "release.artifact_source_invalid")
        for field in fields - {"archive", "target", "features", "profile", "source_commit", "build_evidence", "toolchain", "binaries_sha256"}:
            require_sha(item[field], "release.artifact_record_invalid")
        if item["build_evidence"] != {"independent_builds": 2, "binary_bytes_equal": True,
                                      "normalized_archive_bytes_equal": True}:
            fail("release.two_build_evidence_missing")
        toolchain = item["toolchain"]
        if not isinstance(toolchain, dict) or set(toolchain) != {"cargo_version", "cargo_binary_sha256", "rustc_version", "rustc_binary_sha256"}:
            fail("release.toolchain_record_invalid")
        for field in ("cargo_version", "rustc_version"):
            if (not isinstance(toolchain[field], str) or not toolchain[field] or len(toolchain[field].encode()) > 4096
                    or any(c in toolchain[field] for c in "\0\r\n")
                    or any(prefix in toolchain[field] for prefix in ("/Users/", "/private/", "/tmp/", "/home/", "/opt/homebrew/"))):
                fail("release.toolchain_record_invalid")
        for field in ("cargo_binary_sha256", "rustc_binary_sha256"):
            require_sha(toolchain[field], "release.toolchain_record_invalid")
        if item["source_commit"] != source["commit"] or item["source_archive_sha256"] != source["archive_sha256"]:
            fail("release.artifact_source_mismatch")
    canonical_order = sorted(artifacts, key=lambda item: (item["target"], item["features"], item["archive"]))
    if artifacts != canonical_order:
        fail("release.artifact_order_invalid")


def signature_message(manifest_bytes: bytes) -> bytes:
    return DOMAIN + manifest_bytes


def run_openssl(args: list[str], cwd: Path, env: dict[str, str], timeout: float = 5.0,
                allow_failure: bool = False) -> tuple[int, bytes, bytes]:
    if not OPENSSL.is_file():
        fail("release.openssl_unavailable")
    try:
        version_code, stdout, stderr = PACKAGE.run_bounded_process(
            [str(OPENSSL), *args], cwd, env, timeout=timeout, max_log=65536)
    except PACKAGE.PackageError as error:
        if str(error) == "build.timeout":
            fail("release.openssl_timeout")
        fail("release.openssl_failed")
    if version_code and not allow_failure:
        fail("release.openssl_failed")
    return version_code, stdout, stderr


def verify_signature(message: bytes, signature: bytes, public_key: bytes) -> None:
    if len(signature) != MAX_SIGNATURE:
        fail("release.signature_invalid")
    if len(public_key) != MAX_PUBLIC_KEY or not public_key.startswith(ED25519_SPKI_PREFIX):
        fail("release.public_key_format_invalid")
    with tempfile.TemporaryDirectory(prefix="agent-router-release-verify-", dir=tempfile.gettempdir()) as directory:
        private = Path(directory)
        message_path, signature_path, key_path = private / "message", private / "signature", private / "public.der"
        for path, data in ((message_path, message), (signature_path, signature), (key_path, public_key)):
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            with os.fdopen(descriptor, "wb") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
        env = {"HOME": str(private), "TMPDIR": str(private), "PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
        _, version, _ = run_openssl(["version"], private, env)
        if tuple(version.decode("ascii", errors="replace").split()[:2]) != OPENSSL_VERSION:
            fail("release.openssl_version_mismatch")
        code, verdict, _ = run_openssl(["pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                                  "-inkey", str(key_path), "-sigfile", str(signature_path), "-in", str(message_path)],
                                 private, env, allow_failure=True)
        text = verdict.decode("ascii", errors="replace").strip()
        if code == 0 and text == "Signature Verified Successfully":
            return
        if code == 1 and text == "Signature Verification Failure":
            fail("release.signature_invalid")
        fail("release.openssl_failed")


def write_new_file(path: Path, data: bytes, mode: int = 0o644) -> None:
    if path.exists() or path.is_symlink() or not path.parent.is_dir() or path.parent.is_symlink():
        fail("release.output_exists_or_parent_invalid")
    temporary = None
    try:
        descriptor, name = tempfile.mkstemp(prefix=".agent-router-release-", dir=path.parent)
        temporary = Path(name)
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.link(temporary, path)
    except FileExistsError as error:
        raise ReleaseError("release.output_exists_or_parent_invalid") from error
    except OSError as error:
        raise ReleaseError("release.output_write_failed") from error
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def prepare(args: argparse.Namespace) -> dict:
    if not HEX_SHA.fullmatch(args.source_sha256) or not COMMIT.fullmatch(args.source_commit):
        fail("release.source_pin_invalid")
    source_files, source_meta = load_source(args.source_archive, args.source_sha256, args.source_commit)
    if not 1 <= len(args.candidate) <= MAX_ARTIFACTS:
        fail("release.artifact_count_invalid")
    template = read_template()
    artifacts = []
    used_names, used_pairs = set(), set()
    for candidate_path in args.candidate:
        if candidate_path.is_symlink() or not candidate_path.is_file():
            fail("release.candidate_invalid")
        safe_archive_name(candidate_path.name)
        payload = read_regular(candidate_path, PACKAGE.MAX_ARCHIVE, "release.candidate_invalid")
        candidate = validate_candidate(candidate_path, source_files, source_meta, args.source_sha256, payload)
        if candidate["archive"] in used_names:
            fail("release.archive_name_duplicate")
        pair = candidate["target"], tuple(candidate["features"])
        if pair in used_pairs:
            fail("release.artifact_pair_duplicate")
        used_names.add(candidate["archive"])
        used_pairs.add(pair)
        formula = render_formula(template, NEUTRAL_ARCHIVE_URL + candidate["archive"], candidate["archive_sha256"],
                                 source_meta["version"], candidate["target"], candidate["features"])
        candidate["formula_sha256"] = sha(formula)
        artifacts.append(candidate)
    artifacts.sort(key=lambda item: (item["target"], item["features"], item["archive"]))
    distribution = source_meta["distribution"]
    manifest = {
        "schema": SCHEMA,
        "source": {
            "commit": distribution["source_commit"],
            "archive_sha256": args.source_sha256,
            "distribution_manifest_sha256": sha(source_files["DISTRIBUTION.json"]),
            "cargo_lock_sha256": sha(source_files["Cargo.lock"]),
            "license_inventory_sha256": sha(source_files["LICENSE-INVENTORY.json"]),
            "notices_sha256": sha(source_files["THIRD_PARTY_NOTICES.md"]),
            "source_sbom_sha256": sha(source_files["SBOM.spdx.json"]),
        },
        "formula_template_sha256": sha(template),
        "artifacts": artifacts,
    }
    manifest_bytes = canonical(manifest)
    if len(manifest_bytes) > MAX_MANIFEST:
        fail("release.manifest_size_limit")
    check_manifest_shape(manifest)
    output = args.output_dir
    if output.exists() or output.is_symlink() or not output.parent.is_dir() or output.parent.is_symlink():
        fail("release.output_exists_or_parent_invalid")
    try:
        output.mkdir(mode=0o700)
    except OSError as error:
        raise ReleaseError("release.output_exists_or_parent_invalid") from error
    try:
        (output / "MANIFEST.json").write_bytes(manifest_bytes)
        (output / "MANIFEST.json").chmod(0o600)
        (output / "SIGNING-MESSAGE.bin").write_bytes(signature_message(manifest_bytes))
        (output / "SIGNING-MESSAGE.bin").chmod(0o600)
    except OSError as error:
        for path in (output / "MANIFEST.json", output / "SIGNING-MESSAGE.bin"):
            path.unlink(missing_ok=True)
        output.rmdir()
        raise ReleaseError("release.output_write_failed") from error
    return {"ok": True, "schema": SCHEMA, "manifest_sha256": sha(manifest_bytes),
            "files": ["MANIFEST.json", "SIGNING-MESSAGE.bin"]}


def verify(args: argparse.Namespace) -> dict:
    for value in (args.expected_manifest_sha256, args.expected_candidate_sha256,
                  args.source_sha256, args.public_key_sha256):
        if not HEX_SHA.fullmatch(value):
            fail("release.pin_invalid")
    if not COMMIT.fullmatch(args.expected_source_commit):
        fail("release.source_commit_invalid")
    manifest_bytes = read_regular(args.manifest, MAX_MANIFEST, "release.manifest_size_or_type_invalid")
    if sha(manifest_bytes) != args.expected_manifest_sha256:
        fail("release.manifest_pin_mismatch")
    manifest = strict_json(manifest_bytes, "release.manifest_json_invalid")
    check_manifest_shape(manifest)
    signature = read_regular(args.signature, MAX_SIGNATURE, "release.signature_invalid")
    public_key = read_regular(args.public_key, MAX_PUBLIC_KEY, "release.public_key_format_invalid")
    if sha(public_key) != args.public_key_sha256:
        fail("release.public_key_pin_mismatch")
    verify_signature(signature_message(manifest_bytes), signature, public_key)
    source_files, source_meta = load_source(args.source_archive, args.source_sha256, args.expected_source_commit)
    source_record = manifest["source"]
    if (source_record["commit"] != args.expected_source_commit
            or source_record["archive_sha256"] != args.source_sha256
            or source_record["distribution_manifest_sha256"] != sha(source_files["DISTRIBUTION.json"])
            or source_record["cargo_lock_sha256"] != sha(source_files["Cargo.lock"])
            or source_record["license_inventory_sha256"] != sha(source_files["LICENSE-INVENTORY.json"])
            or source_record["notices_sha256"] != sha(source_files["THIRD_PARTY_NOTICES.md"])
            or source_record["source_sbom_sha256"] != sha(source_files["SBOM.spdx.json"])):
        fail("release.source_evidence_mismatch")
    template = read_template()
    if sha(template) != manifest["formula_template_sha256"]:
        fail("release.formula_template_hash_mismatch")
    candidates = {}
    for path in args.candidate:
        if path.is_symlink() or not path.is_file():
            fail("release.candidate_invalid")
        safe_archive_name(path.name)
        if path.name in candidates:
            fail("release.archive_name_duplicate")
        candidates[path.name] = path
    records = {item["archive"]: item for item in manifest["artifacts"]}
    if set(candidates) != set(records):
        fail("release.candidate_set_mismatch")
    checked = {}
    for name, path in candidates.items():
        record = records[name]
        payload = read_regular(path, PACKAGE.MAX_ARCHIVE, "release.candidate_invalid")
        if sha(payload) != record["archive_sha256"]:
            fail("release.candidate_sha256_mismatch")
        actual = validate_candidate(path, source_files, source_meta, args.source_sha256, payload)
        for key in ("archive_sha256", "binaries_sha256", "target", "features", "profile", "source_commit",
                    "source_archive_sha256", "source_manifest_sha256", "cargo_lock_sha256",
                    "license_inventory_sha256", "notices_sha256", "source_sbom_sha256", "build_sbom_sha256",
                    "build_observations_sha256", "build_evidence", "toolchain"):
            if actual[key] != record[key]:
                fail("release.candidate_metadata_mismatch")
        checked[name] = (record, path)
    features = parse_features_argument(args.features_json)
    selected = [record for record in manifest["artifacts"]
                if record["target"] == args.target and record["features"] == features]
    if len(selected) != 1:
        fail("release.artifact_selection_missing_or_ambiguous")
    record = selected[0]
    if record["archive_sha256"] != args.expected_candidate_sha256:
        fail("release.candidate_pin_mismatch")
    candidate_path = checked[record["archive"]][1]
    neutral = render_formula(template, NEUTRAL_ARCHIVE_URL + record["archive"], record["archive_sha256"],
                             source_meta["version"], record["target"], record["features"])
    if sha(neutral) != record["formula_sha256"]:
        fail("release.formula_hash_mismatch")
    formula = render_formula(template, selected_archive_uri(candidate_path, source_meta["version"], args.archive_url), record["archive_sha256"],
                             source_meta["version"], record["target"], record["features"])
    write_new_file(args.formula_output, formula)
    return {"ok": True, "schema": SCHEMA, "selected_archive_sha256": record["archive_sha256"],
            "manifest_sha256": sha(manifest_bytes), "formula_sha256": sha(formula),
            "neutral_formula_sha256": record["formula_sha256"],
            "target": record["target"], "features": record["features"], "profile": record["profile"]}


def parse_features_argument(value: str) -> list[str]:
    try:
        data = value.encode("utf-8")

        def no_duplicates(pairs):
            result = {}
            for key, item in pairs:
                if key in result:
                    fail("release.features_invalid")
                result[key] = item
            return result

        parsed = json.loads(data, object_pairs_hook=no_duplicates)
    except (ReleaseError, ValueError, TypeError, UnicodeError) as error:
        if isinstance(error, ReleaseError):
            raise
        raise ReleaseError("release.features_invalid") from error
    if not isinstance(parsed, list) or canonical(parsed) != data + b"\n":
        fail("release.features_invalid")
    return validate_features(parsed)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare", help="create canonical manifest and signing bytes")
    prepare_parser.add_argument("--source-archive", type=Path, required=True)
    prepare_parser.add_argument("--source-sha256", required=True)
    prepare_parser.add_argument("--source-commit", required=True)
    prepare_parser.add_argument("--candidate", type=Path, action="append", required=True)
    prepare_parser.add_argument("--output-dir", type=Path, required=True)
    verify_parser = commands.add_parser("verify", help="verify signature and external pins, then render one formula")
    verify_parser.add_argument("--source-archive", type=Path, required=True)
    verify_parser.add_argument("--source-sha256", required=True)
    verify_parser.add_argument("--expected-source-commit", required=True)
    verify_parser.add_argument("--manifest", type=Path, required=True)
    verify_parser.add_argument("--signature", type=Path, required=True)
    verify_parser.add_argument("--public-key", type=Path, required=True)
    verify_parser.add_argument("--public-key-sha256", required=True)
    verify_parser.add_argument("--expected-manifest-sha256", required=True)
    verify_parser.add_argument("--expected-candidate-sha256", required=True)
    verify_parser.add_argument("--candidate", type=Path, action="append", required=True)
    verify_parser.add_argument("--target", required=True)
    verify_parser.add_argument("--features-json", required=True)
    verify_parser.add_argument("--formula-output", type=Path, required=True)
    verify_parser.add_argument("--archive-url", help="opt-in canonical GitHub release URL; does not check publication")
    args = parser.parse_args()
    try:
        result = prepare(args) if args.command == "prepare" else verify(args)
    except (ReleaseError, OSError, ValueError, TypeError, KeyError, AttributeError, EOFError,
            tarfile.TarError, zlib.error, PACKAGE.PackageError) as error:
        reason = str(error) if isinstance(error, ReleaseError) else "release.input_invalid"
        print(json.dumps({"ok": False, "error": reason}, sort_keys=True))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
