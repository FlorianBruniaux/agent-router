#!/usr/bin/env python3
"""Export auditable Rust sources without installing or invoking an integration."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib


MAX_FILE = 8 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
FORBIDDEN_PARTS = {
    ".git", ".codex", ".claude", ".agents", ".agent-router", ".harness-state",
    "target", "node_modules", "private", "telemetry", "policy-packs", "credentials",
}
SUFFIXES = {".rs", ".toml", ".json", ".jsonl", ".md", ".txt", ".yaml", ".yml"}
CONTENT_RULES = (
    ("private_home_path", re.compile(rb"(?:/Users/|/home/)[A-Za-z0-9_.-]+[/\\]")),
    ("private_windows_path", re.compile(rb"[A-Za-z]:[\\/]+Users[\\/]+[^\s\"']+")),
    ("private_key", re.compile(rb"-----BEGIN (?:[A-Z0-9]+ )?PRIVATE KEY-----")),
    ("credential_token", re.compile(rb"\b(?:gh[pousr]_[A-Za-z0-9]{25,}|github_pat_[A-Za-z0-9_]{40,}|sk-(?:proj-)?[A-Za-z0-9_-]{24,}|AKIA[A-Z0-9]{16})\b")),
    ("credential_assignment", re.compile(rb"(?i)(?:api[_-]?key|access[_-]?token|client[_-]?secret|password)\s*[=:]\s*[\"'][A-Za-z0-9_+/=.-]{12,}[\"']")),
)


class PackageError(Exception):
    """A packaging boundary failed. Messages never include matched secret data."""


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode()


def run(argv: list[str], cwd: Path) -> bytes:
    result = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=120, check=False)
    if result.returncode:
        # Cargo and scanner diagnostics can contain absolute paths or secret values.
        raise PackageError(f"{Path(argv[0]).name}.failed (exit {result.returncode})")
    return result.stdout


def safe_path(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        bool(path.parts)
        and path.as_posix() == name
        and not path.is_absolute()
        and all(part not in {"", ".", ".."} for part in path.parts)
        and all(part.lower() not in FORBIDDEN_PARTS for part in path.parts)
        and not any(part.lower().startswith(".env") for part in path.parts)
        and not name.lower().endswith((".pem", ".key", ".db", ".sqlite", ".sqlite3", ".local.toml"))
        and "\\" not in name
    )


def check_content(name: str, data: bytes) -> None:
    for code, pattern in CONTENT_RULES:
        if pattern.search(data):
            raise PackageError(f"content.{code}: {name}")


def read_source(root: Path, relative: str) -> bytes:
    if not safe_path(relative):
        raise PackageError(f"path.excluded: {relative}")
    path = root
    for component in PurePosixPath(relative).parts:
        path /= component
        if path.is_symlink():
            raise PackageError(f"path.symlink: {relative}")
    if not path.is_file() or path.stat().st_size > MAX_FILE:
        raise PackageError(f"source.missing_or_oversized: {relative}")
    data = path.read_bytes()
    if len(data) > MAX_FILE:
        raise PackageError(f"source.oversized: {relative}")
    check_content(relative, data)
    return data


def discover(root: Path, distribution: str = "core") -> tuple[dict, list[dict], list[dict]]:
    if distribution not in {"core", "integrations"}:
        raise PackageError("workspace.invalid_distribution_mode")
    metadata = json.loads(run([
        "cargo", "metadata", "--no-deps", "--locked", "--offline", "--format-version", "1",
    ], root))
    if Path(metadata["workspace_root"]).resolve() != root:
        raise PackageError("workspace.root_mismatch")
    core, excluded, integration_roots = [], [], []
    for package in metadata["packages"]:
        directory = Path(package["manifest_path"]).parent.resolve()
        try:
            relative = directory.relative_to(root).as_posix()
        except ValueError as error:
            raise PackageError("workspace.external_member") from error
        declared = ((package.get("metadata") or {}).get("agent-router") or {}).get("distribution")
        if declared not in {None, "core", "integration", "development", "private"}:
            raise PackageError(f"workspace.invalid_distribution: {package['name']}")
        record = dict(package, relative=relative)
        if declared == "integration" or (declared not in {"development", "private"} and relative.startswith("crates/adapter-")):
            integration_roots.append(record)
        if declared in {"integration", "development", "private"} or relative == "xtask":
            excluded.append(record)
        elif relative.startswith("crates/") and declared in {None, "core"}:
            core.append(record)
        else:
            raise PackageError(f"workspace.unclassified_member: {package['name']}")
    if distribution == "integrations":
        if not integration_roots:
            raise PackageError("workspace.no_integration_packages")
        allowed = {str((root / p["relative"]).resolve()): p for p in core + integration_roots}
        selected = {}
        todo = list(integration_roots)
        while todo:
            package = todo.pop()
            if package["name"] in selected:
                continue
            selected[package["name"]] = package
            for dependency in package["dependencies"]:
                if not dependency.get("path"):
                    continue
                path = str(Path(dependency["path"]).resolve())
                if path not in allowed:
                    raise PackageError(f"workspace.excluded_path_dependency: {package['name']}/{dependency['name']}")
                todo.append(allowed[path])
        all_packages = {p["name"]: p for p in core + excluded}
        core = list(selected.values())
        excluded = [p for name, p in all_packages.items() if name not in selected]
    if not core:
        raise PackageError("workspace.no_core_packages")
    included_paths = {str((root / package["relative"]).resolve()) for package in core}
    for package in core:
        for dependency in package["dependencies"]:
            if dependency.get("path") and str(Path(dependency["path"]).resolve()) not in included_paths:
                raise PackageError(f"workspace.excluded_path_dependency: {package['name']}/{dependency['name']}")
    return metadata, sorted(core, key=lambda p: p["name"]), sorted(excluded, key=lambda p: p["name"])


def projected_manifest(data: bytes, packages: list[dict]) -> bytes:
    text = data.decode()
    parsed = tomllib.loads(text)
    if "package" in parsed:
        raise PackageError("workspace.root_package_unsupported")
    match = re.search(r"(?ms)^\[workspace\]\s*\n(.*?)(?=^\[|\Z)", text)
    if not match:
        raise PackageError("workspace.section_missing")
    section = match.group(1)
    members = json.dumps(sorted(package["relative"] for package in packages))
    for key in ("members", "default-members", "exclude"):
        pattern = rf"(?ms)^{key}\s*=\s*\[.*?\]\s*(?:\n|$)"
        replacement = f"members = {members}\n" if key == "members" else ""
        section, count = re.subn(pattern, replacement, section)
        if key == "members" and count != 1:
            raise PackageError("workspace.members_projection_failed")
    result = (text[:match.start(1)] + section + text[match.end(1):]).encode()
    if tomllib.loads(result.decode())["workspace"]["members"] != json.loads(members):
        raise PackageError("workspace.projection_invalid")
    return result


def collect_sources(root: Path, packages: list[dict], distribution: str = "core", excluded: list[dict] | None = None) -> dict[str, bytes]:
    names = {"Cargo.toml", "Cargo.lock", "LICENSE.md", "NOTICE"}
    for package in packages:
        base = root / package["relative"]
        names.add(f"{package['relative']}/Cargo.toml")
        if (base / "build.rs").exists():
            names.add(f"{package['relative']}/build.rs")
        for directory in (base / "src", base / "tests", base / "benches", base / "examples"):
            if not directory.exists():
                continue
            for path in sorted(directory.rglob("*")):
                if path.is_symlink():
                    raise PackageError(f"path.symlink: {path.relative_to(root).as_posix()}")
                if path.is_file():
                    relative = path.relative_to(root).as_posix()
                    if path.suffix not in SUFFIXES:
                        raise PackageError(f"source.unclassified_file: {relative}")
                    names.add(relative)
        for key in ("readme", "license_file"):
            if package.get(key):
                path = Path(package[key])
                if not path.is_absolute():
                    path = base / path
                try:
                    names.add(path.relative_to(root).as_posix())
                except ValueError as error:
                    raise PackageError(f"workspace.external_{key}: {package['name']}") from error
    for directory in (root / "assets" / "catalogs", root / "tests" / "fixtures", root / "fixtures" / "examples"):
        if directory.exists():
            for path in sorted(directory.rglob("*")):
                relative = path.relative_to(root).as_posix()
                if path.is_symlink():
                    raise PackageError(f"path.symlink: {relative}")
                if path.is_file():
                    if path.suffix not in SUFFIXES:
                        raise PackageError(f"source.unclassified_file: {relative}")
                    names.add(relative)
    if distribution == "integrations":
        excluded_paths = [PurePosixPath(p["relative"]) for p in excluded or []]
        for path in sorted((root / "integrations").rglob("*")):
            relative = path.relative_to(root).as_posix()
            if any(PurePosixPath(relative).is_relative_to(base) for base in excluded_paths):
                continue
            if path.is_symlink():
                raise PackageError(f"path.symlink: {relative}")
            if path.is_file():
                if path.suffix not in SUFFIXES:
                    raise PackageError(f"source.unclassified_file: {relative}")
                names.add(relative)
    files = {name: read_source(root, name) for name in sorted(names)}
    files["Cargo.toml"] = projected_manifest(files["Cargo.toml"], packages)
    if sum(map(len, files.values())) > MAX_TOTAL:
        raise PackageError("source.total_size_exceeded")
    return files


def validate_projected_lock(original: bytes, projected: bytes, packages: list[dict]) -> None:
    before = tomllib.loads(original.decode())
    after = tomllib.loads(projected.decode())
    if before.get("version") != after.get("version"):
        raise PackageError("workspace.lock_version_changed")
    previous = {(p["name"], p["version"], p.get("source")): p for p in before["package"]}
    selected = {(p["name"], p["version"]) for p in packages}
    actual = {(p["name"], p["version"]) for p in after["package"] if not p.get("source")}
    if selected != actual:
        raise PackageError("workspace.lock_member_mismatch")

    def dependencies(package: dict, lock: dict) -> set[tuple]:
        result = set()
        for dependency in package.get("dependencies", []):
            parts = dependency.split(" ", 2)
            matches = [p for p in lock["package"] if p["name"] == parts[0]
                       and (len(parts) < 2 or p["version"] == parts[1])
                       and (len(parts) < 3 or f"({p.get('source')})" == parts[2])]
            if len(matches) != 1:
                raise PackageError("workspace.lock_dependency_ambiguous")
            match = matches[0]
            result.add((match["name"], match["version"], match.get("source")))
        return result

    for package in after["package"]:
        key = (package["name"], package["version"], package.get("source"))
        prior = previous.get(key)
        if prior is None or prior.get("checksum") != package.get("checksum"):
            raise PackageError("workspace.lock_dependency_changed")
        if not dependencies(package, after) <= dependencies(prior, before):
            raise PackageError("workspace.lock_dependency_graph_changed")
        if not package.get("source") and key[:2] not in selected:
            raise PackageError("workspace.lock_excluded_member")


def projected_lock(files: dict[str, bytes], packages: list[dict]) -> bytes:
    # Cargo may prune inactive workspace packages even when every version stays
    # pinned. Resolve only the copied workspace, offline, without building it.
    with tempfile.TemporaryDirectory(prefix="agent-router-package-lock-") as directory:
        staging = Path(directory)
        for name, data in files.items():
            destination = staging / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        run(["cargo", "metadata", "--offline", "--all-features", "--format-version", "1"], staging)
        lock = (staging / "Cargo.lock").read_bytes()
    validate_projected_lock(files["Cargo.lock"], lock, packages)
    return lock


def crate_evidence(package: dict, cargo_home: Path) -> tuple[dict, dict[str, bytes]]:
    record = {
        "name": package["name"], "version": package["version"],
        "source": package.get("source", "workspace"), "checksum": package.get("checksum"),
        "license_declared": "NOASSERTION", "evidence": "missing_cached_archive", "license_files": [],
    }
    if not package.get("source", "").startswith("registry+") or not package.get("checksum"):
        record["evidence"] = "unsupported_source"
        return record, {}
    filename = f"{package['name']}-{package['version']}.crate"
    for cache in sorted((cargo_home / "registry" / "cache").glob(f"*/{filename}")):
        if cache.is_symlink() or cache.stat().st_size > MAX_TOTAL:
            continue
        data = cache.read_bytes()
        if digest(data) != package["checksum"]:
            record["evidence"] = "archive_checksum_mismatch"
            continue
        prefix = f"{package['name']}-{package['version']}/"
        texts = {}
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            members = archive.getmembers()
            manifest_member = archive.getmember(prefix + "Cargo.toml")
            if not manifest_member.isfile() or manifest_member.size > MAX_FILE:
                raise PackageError(f"license.invalid_manifest: {filename}")
            manifest_data = archive.extractfile(manifest_member).read(MAX_FILE + 1)
            manifest = tomllib.loads(manifest_data.decode())["package"]
            if (manifest["name"], manifest["version"]) != (package["name"], package["version"]):
                raise PackageError(f"license.identity_mismatch: {filename}")
            record["license_declared"] = manifest.get("license", "NOASSERTION")
            record["manifest_sha256"] = digest(manifest_data)
            named_license = manifest.get("license-file")
            for member in members:
                if not member.name.startswith(prefix):
                    raise PackageError(f"license.invalid_member: {filename}")
                relative = member.name[len(prefix):]
                basename = PurePosixPath(relative).name.upper()
                if not (basename.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))
                        or basename.endswith(("-LICENSE", "-LICENCE"))
                        or basename == "AUTHORS" or relative == named_license):
                    continue
                if not member.isfile() or not safe_path(relative) or member.size > MAX_FILE:
                    raise PackageError(f"license.unsafe_file: {filename}")
                content = archive.extractfile(member).read(MAX_FILE + 1)
                check_content(relative, content)
                destination = f"licenses/dependencies/{package['name']}-{package['version']}/{relative}"
                texts[destination] = content
                record["license_files"].append({"path": destination, "sha256": digest(content)})
        record["license_files"].sort(key=lambda item: item["path"])
        record["evidence"] = "checksum_verified" if texts else "license_text_missing"
        return record, texts
    return record, {}


def supplemental_licenses(root: Path, record: dict) -> dict[str, bytes]:
    evidence_path = root / "licenses/third-party/evidence.json"
    if not evidence_path.exists():
        return {}
    evidence = json.loads(read_source(root, "licenses/third-party/evidence.json"))
    matches = [item for item in evidence["packages"]
               if (item["name"], item["version"]) == (record["name"], record["version"])]
    if len(matches) > 1:
        raise PackageError("license.duplicate_supplement")
    if not matches:
        return {}
    item = matches[0]
    if item["crate_sha256"] != record.get("checksum"):
        raise PackageError("license.supplement_archive_mismatch")
    files = {}
    for entry in item["files"]:
        if not entry["path"].startswith("licenses/third-party/") or not entry["url"].startswith("https://"):
            raise PackageError("license.invalid_supplement")
        content = read_source(root, entry["path"])
        if digest(content) != entry["sha256"]:
            raise PackageError("license.supplement_hash_mismatch")
        files[entry["path"]] = content
        record["license_files"].append(entry)
    record["supplemental_provenance"] = item
    record["evidence"] = "checksum_and_upstream_verified"
    return files


def license_inventory(lock: dict, packages: list[dict], cargo_home: Path, root: Path | None = None) -> tuple[list[dict], dict[str, bytes]]:
    inventory, texts = [], {}
    names = {package["name"]: package for package in packages}
    for package in sorted(lock["package"], key=lambda p: (p["name"], p["version"], p.get("source", ""))):
        if not package.get("source"):
            if package["name"] in names:
                declared = names[package["name"]].get("license") or "NOASSERTION"
                inventory.append({"name": package["name"], "version": package["version"],
                                  "source": "workspace", "license_declared": declared,
                                  "evidence": "workspace_declaration", "license_files": []})
            continue
        record, evidence = crate_evidence(package, cargo_home)
        if root and record["evidence"] == "license_text_missing":
            evidence.update(supplemental_licenses(root, record))
        inventory.append(record)
        texts.update(evidence)
    return inventory, texts


def dependency_scopes(metadata: dict, core: list[dict]) -> dict[tuple[str, str], list[str]]:
    """Classify all-target/all-feature reachability, without claiming binary inclusion."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    todo = [(package["id"], "normal") for package in core]
    seen = set()
    result = {}
    while todo:
        identifier, scope = todo.pop()
        if (identifier, scope) in seen:
            continue
        seen.add((identifier, scope))
        package = packages[identifier]
        result.setdefault((package["name"], package["version"]), set()).add(scope)
        for dependency in nodes[identifier]["deps"]:
            for kind in dependency["dep_kinds"]:
                declared = kind["kind"] or "normal"
                child_scope = "dev" if "dev" in {scope, declared} else "build" if "build" in {scope, declared} else "normal"
                child = packages[dependency["pkg"]]
                if child_scope == "normal" and any("proc-macro" in target["kind"] for target in child["targets"]):
                    child_scope = "build"
                todo.append((dependency["pkg"], child_scope))
    return {key: sorted(value) for key, value in result.items()}


def sbom(inventory: list[dict], identity: str, created: str, distribution: str = "core") -> dict:
    packages = []
    for index, record in enumerate(inventory):
        package = {
            "SPDXID": f"SPDXRef-Package-{index}", "name": record["name"],
            "versionInfo": record["version"], "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False, "licenseConcluded": "NOASSERTION",
            "licenseDeclared": record["license_declared"], "copyrightText": "NOASSERTION",
            "comment": f"Evidence: {record['evidence']}; scope: Cargo.lock conservative inventory, not an active-target dependency graph.",
        }
        if record.get("checksum"):
            package["checksums"] = [{"algorithm": "SHA256", "checksumValue": record["checksum"]}]
        packages.append(package)
    return {
        "spdxVersion": "SPDX-2.3", "dataLicense": "CC0-1.0", "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"agent-router-{distribution}", "documentNamespace": f"https://agent-router.invalid/spdx/{identity}",
        "creationInfo": {"creators": ["Tool: agent-router-package-core"], "created": created},
        "packages": packages,
        "relationships": [{"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES",
                           "relatedSpdxElement": package["SPDXID"]} for package in packages],
    }


def notices(inventory: list[dict]) -> bytes:
    lines = ["# Third-party dependency notices", "",
             "This inventory covers all external Cargo.lock entries, including optional, development and other-platform dependencies.",
             "Declarations come from manifests inside cached crate archives verified against Cargo.lock. NOASSERTION means no established declaration.",
             "License texts retain upstream attribution. A checksum validates bytes, not legal compatibility or security.", "",
             "| Package | Declared license | Evidence |", "| --- | --- | --- |"]
    for record in inventory:
        if record["source"] != "workspace":
            lines.append(f"| {record['name']} {record['version']} | {record['license_declared']} | {record['evidence']} |")
    return ("\n".join(lines) + "\n").encode()


def archive_bytes(files: dict[str, bytes], distribution: str = "core") -> bytes:
    if distribution not in {"core", "integrations"}:
        raise PackageError("archive.invalid_distribution")
    buffer = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buffer, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for name, content in sorted(files.items()):
                if not safe_path(name):
                    raise PackageError(f"archive.unsafe_path: {name}")
                info = tarfile.TarInfo(f"agent-router-{distribution}/" + name)
                info.size, info.mode, info.mtime = len(content), 0o644, 0
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                archive.addfile(info, io.BytesIO(content))
    return buffer.getvalue()


def verify_archive(path: Path) -> dict:
    if path.stat().st_size > MAX_TOTAL:
        raise PackageError("archive.size_exceeded")
    files = {}
    total = 0
    prefix = None
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            member_prefix, separator, name = member.name.partition("/")
            if not separator or member_prefix not in {"agent-router-core", "agent-router-integrations"} or prefix not in {None, member_prefix}:
                raise PackageError("archive.invalid_prefix")
            prefix = member_prefix
            if not member.isfile() or not safe_path(name) or name in files or member.size > MAX_FILE:
                raise PackageError("archive.invalid_member")
            total += member.size
            if total > MAX_TOTAL:
                raise PackageError("archive.expanded_size_exceeded")
            data = archive.extractfile(member).read(MAX_FILE + 1)
            check_content(name, data)
            files[name] = data
    if "DISTRIBUTION.json" not in files:
        raise PackageError("archive.manifest_missing")
    manifest = json.loads(files.pop("DISTRIBUTION.json"))
    distribution = prefix.removeprefix("agent-router-")
    if manifest.get("kind", "core-source") != f"{distribution}-source":
        raise PackageError("archive.distribution_mismatch")
    expected = {entry["path"]: entry["sha256"] for entry in manifest["files"]}
    actual = {name: digest(data) for name, data in files.items()}
    if expected != actual or len(expected) != len(manifest["files"]):
        raise PackageError("archive.inventory_mismatch")
    if manifest["content_sha256"] != digest(json_bytes(actual)):
        raise PackageError("archive.content_digest_mismatch")
    return {"schema_version": 1, "kind": f"{distribution}-source", "verified": True, "sha256": digest(path.read_bytes()),
            "files": len(files) + 1, "content_sha256": manifest["content_sha256"]}


def package(root: Path, output: Path, cargo_home: Path, strict_licenses: bool, require_gitleaks: bool, distribution: str = "core") -> dict:
    root = root.resolve()
    original_manifest = read_source(root, "Cargo.toml")
    _, packages, excluded = discover(root, distribution)
    if read_source(root, "Cargo.toml") != original_manifest:
        raise PackageError("source.changed_during_discovery")
    files = collect_sources(root, packages, distribution, excluded)
    collected_hashes = {name: digest(data) for name, data in files.items()}
    original_hashes = dict(collected_hashes, **{"Cargo.toml": digest(original_manifest)})
    lock = tomllib.loads(files["Cargo.lock"].decode())
    files["Cargo.lock"] = projected_lock(files, packages)
    source_hashes = {name: digest(data) for name, data in files.items()}
    inventory, texts = license_inventory(lock, packages, cargo_home, root)
    try:
        resolved = json.loads(run(["cargo", "metadata", "--locked", "--offline", "--all-features", "--format-version", "1"], root))
        scopes = dependency_scopes(resolved, packages)
    except PackageError:
        scopes = {}
    for item in inventory:
        item["dependency_scopes"] = scopes.get((item["name"], item["version"]),
                                               ["outside_selected_workspace"] if scopes else ["UNKNOWN"])
    missing = [f"{p['name']}@{p['version']}" for p in inventory
               if p["source"] != "workspace" and (p["evidence"] not in {"checksum_verified", "checksum_and_upstream_verified"}
                                                    or p["license_declared"] == "NOASSERTION")]
    if strict_licenses and missing:
        raise PackageError(f"license.evidence_incomplete: {len(missing)} dependencies")
    commit = run(["git", "rev-parse", "HEAD"], root).decode().strip()
    created = run(["git", "show", "-s", "--format=%cI", "HEAD"], root).decode().strip()
    from datetime import datetime, timezone
    created = datetime.fromisoformat(created).astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    identity = digest(json_bytes(source_hashes))
    files.update(texts)
    files["README.md"] = (
        "# Agent Router core source archive\n\n"
        "Build the provider-neutral CLI without host features:\n\n"
        "```sh\ncargo build -p agent-router-cli --no-default-features --locked --offline\n"
        "cargo test -p agent-router-cli --no-default-features --locked --offline\n```\n\n"
        "Offline builds require the locked Rust dependencies in the local Cargo cache. "
        "This source archive does not vendor them. Python is only used by repository release tooling, "
        "not by the compiled Rust core. Optional Rust adapter packages remain separate workspace members; "
        "host executables and integration configuration are not bundled.\n\n"
        "DISTRIBUTION.json records the exact contents and evidence boundaries. "
        "LICENSE-INVENTORY.json, SBOM.spdx.json and THIRD_PARTY_NOTICES.md record dependency provenance. "
        "A verified archive is not evidence of host execution or authorization to publish.\n"
    ).encode()
    if distribution == "integrations":
        files["README.md"] = (
            "# Agent Router integrations source archive\n\n"
            "This independent workspace contains public adapters, provider request translators, "
            "their transitive public Rust dependencies, protocol provenance, synthetic fixtures "
            "and conformance tests. It includes no credentials or activation script.\n\n"
            "```sh\ncargo test --workspace --locked --offline\n```\n\n"
            "Offline builds require the locked Rust dependencies already in the local Cargo cache. "
            "Integration descriptors are inert. Native executables, authenticated HTTP transport, "
            "operator permissions and live canaries are deployment responsibilities. "
            "A passing conformance fixture does not prove live host invocation, model identity, "
            "sandboxing, authentication or task success. See integrations/README.md and "
            "integrations/additional-hosts.md for protocol boundaries.\n\n"
            "DISTRIBUTION.json records exact contents; LICENSE-INVENTORY.json, SBOM.spdx.json "
            "and THIRD_PARTY_NOTICES.md record dependency evidence. No publication is performed.\n"
        ).encode()
    files["LICENSE-INVENTORY.json"] = json_bytes({"schema_version": 1, "scope": "Cargo.lock", "packages": inventory})
    files["SBOM.spdx.json"] = json_bytes(sbom(inventory, identity, created, distribution))
    files["THIRD_PARTY_NOTICES.md"] = notices(inventory)
    scanner = "not_available"
    if shutil.which("gitleaks"):
        with tempfile.TemporaryDirectory(prefix="agent-router-package-scan-") as directory:
            staging = Path(directory)
            for name, data in files.items():
                destination = staging / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
            run(["gitleaks", "dir", "--no-banner", "--redact", "--exit-code", "1", str(staging)], root)
        scanner = "passed"
    elif require_gitleaks:
        raise PackageError("gitleaks.not_available")
    hashes = {name: digest(data) for name, data in sorted(files.items())}
    manifest = {
        "schema_version": 1, "kind": f"{distribution}-source", "source_commit": commit,
        "source_content_sha256": identity, "original_source_sha256": digest(json_bytes(original_hashes)),
        "content_sha256": digest(json_bytes(hashes)),
        "workspace_packages": [{"name": p["name"], "version": p["version"], "path": p["relative"],
                                "features": p["features"]} for p in packages],
        "excluded_packages": [{"name": p["name"], "path": p["relative"]} for p in excluded],
        "source_transformations": ["Cargo.toml workspace membership selects " +
                                   ("core packages, excluding development, integration and private packages" if distribution == "core" else
                                    "public integrations and their public path dependency closure, excluding development and private packages"),
                                   "Cargo.lock projected offline to included workspace; original dependency identities and checksums preserved"],
        "evidence": {"license_inventory": "complete", "license_texts": "complete" if not missing else "incomplete", "missing_licenses": missing,
                     "dependency_scope": f"all_{distribution}_members_all_features_all_targets" if scopes else "UNKNOWN",
                     "gitleaks": scanner, "legal_compatibility_review": "UNKNOWN", "security_review": "UNKNOWN",
                     "host_runtime": "NOT_TESTED", "publication": "NOT_REQUESTED"},
        "files": [{"path": name, "sha256": value} for name, value in hashes.items()],
    }
    files["DISTRIBUTION.json"] = json_bytes(manifest)
    for name, data in files.items():
        check_content(name, data)
    payload = archive_bytes(files, distribution)
    for name, expected in original_hashes.items():
        if digest(read_source(root, name)) != expected:
            raise PackageError(f"source.changed_during_export: {name}")
    if {name: digest(data) for name, data in collect_sources(root, packages, distribution, excluded).items()} != collected_hashes:
        raise PackageError("source.file_set_changed_during_export")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Publish only a verified, complete file; link is atomic and refuses an existing destination.
    with tempfile.NamedTemporaryFile(prefix=".agent-router-package-", dir=output.parent, delete=False) as handle:
        temporary = Path(handle.name)
        handle.write(payload)
    try:
        result = verify_archive(temporary)
        os.link(temporary, output)
    finally:
        temporary.unlink()
    result["evidence"] = manifest["evidence"]
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--distribution", choices=("core", "integrations"), default="core")
    parser.add_argument("--cargo-home", type=Path, default=Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
    parser.add_argument("--strict-licenses", action="store_true")
    parser.add_argument("--require-gitleaks", action="store_true")
    args = parser.parse_args()
    if bool(args.output) == bool(args.verify):
        parser.error("choose exactly one of --output or --verify")
    try:
        result = verify_archive(args.verify) if args.verify else package(
            args.root, args.output, args.cargo_home, args.strict_licenses, args.require_gitleaks, args.distribution)
    except (PackageError, OSError, ValueError, KeyError, TypeError, tarfile.TarError, subprocess.TimeoutExpired) as error:
        message = str(error) if isinstance(error, PackageError) else type(error).__name__
        print(json.dumps({"schema_version": 1, "error": message}))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
