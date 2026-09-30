#!/usr/bin/env python3
"""Build and locally export one provider-neutral Agent Router binary."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import selectors
import signal
import time

MAX_ARCHIVE = 256 * 1024 * 1024
MAX_VENDOR = 1024 * 1024 * 1024
MAX_BINARY = 128 * 1024 * 1024
MAX_CRATE_FILE = 32 * 1024 * 1024
MAX_LOG = 64 * 1024 * 1024
MAX_LINE = 1024 * 1024
MAX_EVENTS = 100_000
PYTHON = Path(__file__).with_name("package-core.py")
SPEC = importlib.util.spec_from_file_location("package_core", PYTHON)
CORE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CORE)


class PackageError(Exception):
    pass


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def checked_archive(path: Path, expected: str) -> dict[str, bytes]:
    if not path.is_file() or path.is_symlink() or path.stat().st_size > MAX_ARCHIVE:
        raise PackageError("source.archive_invalid")
    payload = path.read_bytes()
    if sha(payload) != expected:
        raise PackageError("source.sha256_mismatch")
    with tempfile.TemporaryDirectory(prefix="agent-router-source-verify-") as directory:
        verified_path = Path(directory) / "source.tar.gz"
        verified_path.write_bytes(payload)
        summary = CORE.verify_archive(verified_path)
    if summary["kind"] != "core-source":
        raise PackageError("source.kind_mismatch")
    output: dict[str, bytes] = {}
    total = 0
    with tarfile.open(fileobj=__import__("io").BytesIO(payload), mode="r:gz") as archive:
        for member in archive:
            prefix, separator, name = member.name.partition("/")
            if prefix != "agent-router-core" or not separator or not member.isfile() or not CORE.safe_path(name):
                raise PackageError("source.member_invalid")
            data = archive.extractfile(member).read(CORE.MAX_FILE + 1)
            if len(data) != member.size or name in output:
                raise PackageError("source.member_invalid")
            total += len(data)
            if total > MAX_ARCHIVE:
                raise PackageError("source.expanded_limit")
            output[name] = data
    if "Cargo.lock" not in output or "Cargo.toml" not in output or "LICENSE-INVENTORY.json" not in output:
        raise PackageError("source.required_file_missing")
    manifest = json.loads(output["DISTRIBUTION.json"])
    if manifest.get("evidence", {}).get("license_texts") != "complete":
        raise PackageError("source.license_evidence_incomplete")
    return output


def safe_extract_crate(payload: bytes, destination: Path, name: str, version: str,
                       expanded_remaining: int) -> dict[str, bytes]:
    prefix = f"{name}-{version}/"
    extracted: dict[str, bytes] = {}
    expanded = 0
    with tarfile.open(fileobj=__import__("io").BytesIO(payload), mode="r:gz") as archive:
        for item in archive:
            if not item.name.startswith(prefix):
                raise PackageError("vendor.crate_prefix_invalid")
            relative = item.name[len(prefix):]
            if not relative and item.isdir():
                continue
            crate_path = PurePosixPath(relative)
            if (not relative or crate_path.is_absolute() or crate_path.as_posix() != relative
                    or ".." in crate_path.parts or "\\" in relative or relative in extracted):
                raise PackageError("vendor.crate_member_invalid")
            if item.isdir():
                continue
            if not item.isfile():
                raise PackageError("vendor.crate_member_invalid")
            if item.size > MAX_CRATE_FILE:
                raise PackageError("vendor.crate_member_oversized")
            stream = archive.extractfile(item)
            data = stream.read(MAX_CRATE_FILE + 1)
            if len(data) != item.size:
                raise PackageError("vendor.crate_member_invalid")
            expanded += len(data)
            if expanded > expanded_remaining:
                raise PackageError("vendor.expanded_size_limit")
            extracted[relative] = data
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    if "Cargo.toml" not in extracted:
        raise PackageError("vendor.crate_manifest_missing")
    manifest = tomllib.loads(extracted["Cargo.toml"].decode())["package"]
    if (manifest.get("name"), manifest.get("version")) != (name, version):
        raise PackageError("vendor.crate_identity_mismatch")
    checksum = {"files": {path: sha(data) for path, data in sorted(extracted.items())}, "package": sha(payload)}
    (destination / ".cargo-checksum.json").write_bytes(canonical(checksum))
    return extracted


def vendor(lock_bytes: bytes, cargo_cache: Path, destination: Path) -> tuple[dict, str]:
    lock = tomllib.loads(lock_bytes.decode())
    packages = [item for item in lock["package"] if item.get("source", "").startswith("registry+")]
    total = 0
    expanded_total = 0
    inventory = {}
    for package in packages:
        name, version, checksum = package["name"], package["version"], package.get("checksum")
        if not checksum or not re_hex(checksum):
            raise PackageError("vendor.lock_checksum_missing")
        candidate = cargo_cache / "registry" / "cache"
        paths = [p for p in candidate.glob(f"*/{name}-{version}.crate") if p.is_file() and not p.is_symlink()]
        matches = []
        for path in paths:
            contents = path.read_bytes()
            if sha(contents) == checksum:
                matches.append((path, contents))
        if len(matches) != 1:
            raise PackageError("vendor.crate_missing_or_ambiguous")
        payload = matches[0][1]
        total += len(payload)
        if total > MAX_VENDOR:
            raise PackageError("vendor.size_limit")
        record = {"name": name, "version": version, "source": package["source"], "checksum": checksum}
        folder = destination / f"{name}-{version}"
        files = safe_extract_crate(payload, folder, name, version, MAX_VENDOR - expanded_total)
        expanded_total += sum(map(len, files.values()))
        record["observed_license"] = tomllib.loads(files["Cargo.toml"].decode())["package"].get("license", "NOASSERTION")
        inventory[(name, version, package["source"])] = record
    digest = sha(canonical({"packages": sorted((key, value["checksum"]) for key, value in inventory.items())}))
    return inventory, digest


def re_hex(value: str) -> bool:
    return len(value) == 64 and all(char in "0123456789abcdef" for char in value)


def check_cargo_config_ancestors(path: Path) -> None:
    for directory in (path, *path.parents):
        cargo_directory = directory / ".cargo"
        if cargo_directory.is_symlink():
            for name in ("config", "config.toml"):
                candidate = cargo_directory / name
                if candidate.exists() or candidate.is_symlink():
                    raise PackageError("cargo.inherited_config_refused")
        for name in ("config", "config.toml"):
            candidate = cargo_directory / name
            if candidate.exists() or candidate.is_symlink():
                raise PackageError("cargo.inherited_config_refused")


def run_bounded_process(argv: list[str], cwd: Path, env: dict[str, str], timeout: int,
                        max_log: int = MAX_LOG) -> tuple[int, bytes, bytes]:
    process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True)
    assert process.stdout is not None and process.stderr is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    line_tails = {"stdout": 0, "stderr": 0}
    line_counts = {"stdout": 0, "stderr": 0}
    started = time.monotonic()

    def abort(code: str) -> None:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        raise PackageError(code)

    try:
        while selector.get_map():
            if time.monotonic() - started > timeout:
                abort("build.timeout")
            for key, _ in selector.select(timeout=0.1):
                stream = key.data
                chunk = os.read(key.fd, 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    key.fileobj.close()
                    continue
                total = len(buffers["stdout"]) + len(buffers["stderr"])
                if total + len(chunk) > max_log:
                    abort("build.log_limit")
                tail = line_tails[stream]
                pieces = chunk.split(b"\n")
                if len(pieces) == 1:
                    line_tails[stream] = tail + len(pieces[0])
                    if line_tails[stream] > MAX_LINE:
                        abort("build.line_limit")
                else:
                    if tail + len(pieces[0]) > MAX_LINE or any(len(piece) > MAX_LINE for piece in pieces[1:-1]):
                        abort("build.line_limit")
                    line_counts[stream] += len(pieces) - 1
                    line_tails[stream] = len(pieces[-1])
                    if line_tails[stream] > MAX_LINE:
                        abort("build.line_limit")
                if stream == "stdout" and line_counts[stream] > MAX_EVENTS:
                    abort("build.event_limit")
                buffers[stream].extend(chunk)
        try:
            return_code = process.wait(timeout=max(0.0, timeout - (time.monotonic() - started)))
        except subprocess.TimeoutExpired:
            abort("build.timeout")
        return return_code, bytes(buffers["stdout"]), bytes(buffers["stderr"])
    finally:
        selector.close()
        process.stdout.close()
        process.stderr.close()
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()


def resolve_tool(path: Path, tool: str) -> Path:
    candidate = path.expanduser()
    try:
        is_proxy = candidate.is_symlink() or candidate.resolve().name in {"rustup", "rustup-init"}
    except OSError:
        is_proxy = False
    if is_proxy:
        rustup = shutil.which("rustup")
        if not rustup:
            raise PackageError("toolchain.rustup_unavailable")
        result = subprocess.run([rustup, "which", tool], capture_output=True, text=True, timeout=10, check=False)
        if result.returncode:
            raise PackageError("toolchain.binary_unavailable")
        candidate = Path(result.stdout.strip())
    candidate = candidate.resolve()
    if not candidate.is_file() or candidate.is_symlink():
        raise PackageError("toolchain.binary_invalid")
    return candidate


def cargo_events(stdout: bytes) -> tuple[list[tuple[str, str]], list[dict], list[bool]]:
    lines = stdout.splitlines()
    if len(lines) > MAX_EVENTS or any(len(line) > MAX_LINE for line in lines):
        raise PackageError("build.log_limit")
    packages, binaries, finished = [], [], []
    for line in lines:
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            raise PackageError("build.log_truncated_or_invalid") from error
        if not isinstance(event, dict):
            raise PackageError("build.log_invalid")
        if event.get("reason") == "compiler-artifact":
            raw_package_id = event.get("package_id", "")
            package_source, separator, package_version = raw_package_id.rpartition("#")
            identity = package_version.rsplit("@", 1)
            if len(identity) == 2:
                packages.append((identity[0], identity[1]))
            elif separator and package_version:
                packages.append((Path(package_source).name, package_version))
            target_info = event.get("target", {})
            if target_info.get("name") == "agent-router" and "bin" in target_info.get("kind", []):
                binaries.append({"package_id": event.get("package_id"), "target": target_info["name"],
                                 "kind": target_info["kind"], "features": sorted(event.get("features", [])),
                                 "filename_paths": event.get("filenames", [])})
        if event.get("reason") == "build-finished":
            finished.append(event.get("success"))
    return sorted(set(packages)), binaries, finished


def check_build_event_contract(exit_code: int, finished: list[bool], binaries: list[dict],
                               target_dir: Path, executable: Path, features: list[str]) -> dict:
    if exit_code or finished != [True]:
        raise PackageError(f"build.failed.exit_{exit_code}")
    package_id = binaries[0].get("package_id", "") if len(binaries) == 1 else ""
    if (len(binaries) != 1 or "#agent-router-cli@" not in package_id
            and not package_id.rsplit("#", 1)[0].endswith("/crates/agent-router-cli")):
        raise PackageError("build.expected_single_binary_artifact")
    artifact = binaries[0]
    paths = [Path(path).resolve() for path in artifact.get("filename_paths", [])]
    if executable.resolve() not in paths or any(not path.is_relative_to(target_dir.resolve()) for path in paths):
        raise PackageError("build.executable_not_in_cargo_event")
    if sorted(artifact.get("features", [])) != sorted(set(features)):
        raise PackageError("build.feature_set_mismatch")
    artifact["filenames"] = ["agent-router"]
    artifact.pop("filename_paths", None)
    artifact["package_id"] = "path#agent-router-cli@0.0.1"
    return artifact


def run_build(source: dict[str, bytes], vendor_root: Path, run_root: Path, cargo: Path, rustc: Path,
              rustup_home: Path, target: str, features: list[str], inventory: dict, log_destination: Path) -> dict:
    source_root = run_root / "source"
    source_root.mkdir()
    check_cargo_config_ancestors(source_root)
    for name, data in source.items():
        destination = source_root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    cargo_home, target_dir = run_root / "cargo-home", run_root / "target"
    cargo_home.mkdir()
    config = ('[source.crates-io]\nreplace-with = "vendored-sources"\n'
              '[source.vendored-sources]\ndirectory = "' + str(vendor_root) + '"\n').encode()
    config_path = cargo_home / "config.toml"
    config_path.write_bytes(config)
    cmd = [str(cargo), "build", "--release", "--locked", "--offline", "--message-format=json",
           "-p", "agent-router-cli", "--bin", "agent-router", "--no-default-features", "--target", target]
    if features:
        cmd.extend(["--features", ",".join(sorted(set(features)))])
    remap_flags = "\x1f".join([f"--remap-path-prefix={source_root}=/agent-router/source",
                                 f"--remap-path-prefix={vendor_root}=/agent-router/vendor"])
    env = {"PATH": os.pathsep.join([str(cargo.parent), "/usr/bin", "/bin"]), "HOME": str(run_root),
           "CARGO_HOME": str(cargo_home), "CARGO_TARGET_DIR": str(target_dir), "RUSTUP_HOME": str(rustup_home),
           "RUSTC": str(rustc), "RUSTFLAGS": "", "CARGO_ENCODED_RUSTFLAGS": remap_flags, "CARGO_BUILD_TARGET": target,
           "CARGO_NET_OFFLINE": "true", "CARGO_TERM_COLOR": "never", "LC_ALL": "C", "LANG": "C",
           "SOURCE_DATE_EPOCH": "0"}
    return_code, stdout, stderr = run_bounded_process(cmd, source_root, env, timeout=1800)
    raw = b"CARGO-STDOUT\n" + stdout + b"\nCARGO-STDERR\n" + stderr
    all_artifacts, binaries, seen_build_finished = cargo_events(stdout)
    log_path = log_destination
    log_path.write_bytes(raw)
    log_path.chmod(0o600)
    executable = target_dir / target / "release" / "agent-router"
    if not executable.is_file() or executable.is_symlink() or executable.stat().st_size > MAX_BINARY:
        raise PackageError("build.binary_missing_or_oversized")
    binary = executable.read_bytes()
    if not binary or not os.access(executable, os.X_OK):
        raise PackageError("build.binary_not_executable")
    executables = [p for p in (target_dir / target / "release").iterdir() if p.is_file() and os.access(p, os.X_OK)]
    if executables != [executable] and set(executables) != {executable}:
        raise PackageError("build.extra_executable")
    observed = check_build_event_contract(return_code, seen_build_finished, binaries, target_dir, executable, features)
    feature_set = sorted(observed["features"])
    if feature_set != sorted(set(features)):
        raise PackageError("build.feature_set_mismatch")
    rustc_version = subprocess.run([str(rustc), "-Vv"], capture_output=True, env=env, timeout=10, check=True).stdout.decode()
    cargo_version = subprocess.run([str(cargo), "-Vv"], capture_output=True, env=env, timeout=10, check=True).stdout.decode()
    rustc_host = next((line.split(": ", 1)[1] for line in rustc_version.splitlines() if line.startswith("host: ")), None)
    if "release: 1.85.1" not in rustc_version or not cargo_version.startswith("cargo 1.85.1 "):
        raise PackageError("toolchain.version_mismatch")
    if target != rustc_host:
        raise PackageError("target.must_match_native_host")
    def tool_observation(argv: list[str]) -> str:
        executable_path = shutil.which(argv[0], path=env["PATH"])
        if not executable_path:
            return "not_available"
        observed_tool = subprocess.run([executable_path, *argv[1:]], capture_output=True, env=env, timeout=10, check=False)
        data = (observed_tool.stdout + observed_tool.stderr)[:65537]
        if observed_tool.returncode or len(data) > 65536:
            return "unavailable_or_over_limit"
        return data.decode(errors="replace").strip()
    c_compiler = tool_observation(["clang", "--version"])
    sdk = tool_observation(["xcrun", "--sdk", "macosx", "--show-sdk-version"])
    dynamic = "not_inspected"
    if target.endswith("apple-darwin") and Path("/usr/bin/otool").is_file():
        linked = subprocess.run(["/usr/bin/otool", "-L", str(executable)], capture_output=True, env=env, timeout=10, check=False)
        lines = linked.stdout.decode(errors="replace").splitlines()
        dynamic = lines[1:] if linked.returncode == 0 and len(lines) <= 4096 else "unavailable_or_over_limit"
    return {"binary": binary, "features": feature_set, "event": observed, "rustc": rustc_version,
            "cargo": cargo_version, "rustc_binary_sha256": sha(rustc.read_bytes()),
            "cargo_binary_sha256": sha(cargo.read_bytes()), "c_compiler": c_compiler,
            "sdk": sdk, "dynamic_libraries_declared": dynamic, "target": target, "lock_sha256": sha(source["Cargo.lock"]),
            "source_manifest_sha256": sha(source["DISTRIBUTION.json"]), "vendor_sha256": "",
            "config_sha256": sha(config), "raw_log_sha256": sha(raw), "raw_log_size": len(raw),
            "build_packages": sorted(set(all_artifacts)), "exit_code": return_code}


def archive_payload(binary: bytes, proof: dict, source: dict[str, bytes]) -> bytes:
    files = {"bin/agent-router": binary, "LICENSE.md": source["LICENSE.md"],
             "NOTICE": source["NOTICE"], "THIRD_PARTY_NOTICES.md": source["THIRD_PARTY_NOTICES.md"],
             "LICENSE-INVENTORY.json": source["LICENSE-INVENTORY.json"]}
    if "SBOM.spdx.json" in source:
        files["SOURCE-SBOM.spdx.json"] = source["SBOM.spdx.json"]
    lock = tomllib.loads(source["Cargo.lock"].decode())
    inventory_document = json.loads(source["LICENSE-INVENTORY.json"])
    inventory = inventory_document["packages"]
    declared = {(item["name"], item["version"]): item for item in inventory}
    for item in inventory:
        for license_file in item.get("license_files", []):
            path, digest = license_file["path"], license_file["sha256"]
            if (path not in source or sha(source[path]) != digest
                    or not path.startswith(("licenses/dependencies/", "licenses/third-party/"))
                    or not CORE.safe_path(path)):
                raise PackageError("archive.source_license_text_mismatch")
            files[path] = source[path]
    observed = {(p["name"], p["version"], p["source"]): p for p in lock["package"]
                if p.get("source", "").startswith("registry+")}
    packages = []
    observed_ids = {item[0] for item in proof.get("package_ids", [])}
    observed_build = set(map(tuple, proof["build_packages"]))
    for key, record in sorted(proof["vendor_inventory"].items()):
        name, version, registry_source = key
        if (name, version) not in observed_build:
            continue
        source_id = next((p for p in observed.values() if (p["name"], p["version"], p["source"]) == key), None)
        if source_id is None:
            raise PackageError("sbom.lock_record_missing")
        lic = declared.get((name, version))
        if (not lic or lic.get("checksum") != source_id.get("checksum")
                or record.get("checksum") != source_id.get("checksum")
                or record.get("observed_license") != lic.get("license_declared")):
            raise PackageError("sbom.license_record_mismatch")
        packages.append({"name": name, "version": version, "source": registry_source,
                         "checksum": source_id["checksum"], "license_declared": lic.get("license_declared", "NOASSERTION"),
                         "license_concluded": "NOASSERTION", "build_observed": True,
                         "license_files": lic.get("license_files", [])})
    files["BUILD-SBOM.json"] = canonical({"schema": "agent-router-build-sbom-v1", "scope": "Cargo compiler-artifact events and checksum-verified vendor set; not a complete unit graph or static-link proof", "packages": packages})
    files["BUILD-RECIPE.json"] = canonical({"schema": "agent-router-build-recipe-v1", "package": "agent-router-cli", "binary": "agent-router", "profile": "release", "features": proof["features"], "target": proof["target"], "options": ["--locked", "--offline", "--no-default-features", "--remap-path-prefix=<private-source>=/agent-router/source", "--remap-path-prefix=<private-vendor>=/agent-router/vendor"]})
    files["BUILD-OBSERVATIONS.json"] = canonical({"schema": "agent-router-build-observations-v1", "source_archive_sha256": proof["source_sha256"], "source_manifest_sha256": proof["source_manifest_sha256"], "cargo_lock_sha256": proof["lock_sha256"], "vendor_sha256": proof["vendor_sha256"], "cargo_version": proof["cargo_version"], "cargo_binary_sha256": proof["cargo_binary_sha256"], "rustc_version": proof["rustc_version"], "rustc_binary_sha256": proof["rustc_binary_sha256"], "c_compiler": proof["c_compiler"], "sdk": proof["sdk"], "target": proof["target"], "profile": "release", "features": proof["features"], "build_observed_packages": [list(p) for p in proof["build_packages"]], "independent_builds": 2, "binary_bytes_equal": True, "normalized_archive_bytes_equal": True, "dependency_graph": "Cargo metadata and compiler-artifact observations are not an exact compilation-unit or static-link graph", "dynamic_libraries_declared_by_otool": proof["dynamic_libraries_declared"], "security_review": "UNKNOWN", "security_note": "No general security clearance is asserted. Review findings and dependency status are tied to the source archive identified above."})
    files["INTEGRITY.json"] = canonical({name: sha(data) for name, data in sorted(files.items())})
    import io
    buffer = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buffer, mtime=0, compresslevel=9) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as archive:
            for name, data in sorted(files.items()):
                info = tarfile.TarInfo("agent-router-binary/" + name)
                info.size, info.mtime = len(data), 0
                info.mode = 0o755 if name == "bin/agent-router" else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                archive.addfile(info, io.BytesIO(data))
    payload = buffer.getvalue()
    if len(payload) > MAX_ARCHIVE:
        raise PackageError("archive.size_limit")
    verify_binary_archive(payload)
    return payload


def verify_binary_archive(payload: bytes) -> dict[str, bytes]:
    import io
    if len(payload) > MAX_ARCHIVE:
        raise PackageError("archive.size_limit")
    files, modes, total = {}, {}, 0
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        for member in archive:
            prefix, separator, name = member.name.partition("/")
            if prefix != "agent-router-binary" or not separator or not member.isfile() or not CORE.safe_path(name):
                raise PackageError("archive.member_invalid")
            if name in files or member.size > MAX_BINARY:
                raise PackageError("archive.member_invalid")
            total += member.size
            if total > MAX_ARCHIVE:
                raise PackageError("archive.expanded_limit")
            data = archive.extractfile(member).read(MAX_BINARY + 1)
            if len(data) != member.size:
                raise PackageError("archive.member_invalid")
            files[name], modes[name] = data, member.mode
    fixed_names = {"bin/agent-router", "LICENSE.md", "NOTICE", "THIRD_PARTY_NOTICES.md",
                   "LICENSE-INVENTORY.json", "BUILD-SBOM.json", "BUILD-RECIPE.json",
                   "BUILD-OBSERVATIONS.json", "INTEGRITY.json"}
    if "SOURCE-SBOM.spdx.json" in files:
        fixed_names.add("SOURCE-SBOM.spdx.json")
    license_names = {name for name in files if name.startswith(("licenses/dependencies/", "licenses/third-party/"))}
    if set(files) != fixed_names | license_names or modes.get("bin/agent-router") != 0o755:
        raise PackageError("archive.inventory_mismatch")
    if any(mode != 0o644 for name, mode in modes.items() if name != "bin/agent-router"):
        raise PackageError("archive.mode_mismatch")
    integrity = json.loads(files["INTEGRITY.json"])
    actual = {name: sha(data) for name, data in files.items() if name != "INTEGRITY.json"}
    if integrity != actual:
        raise PackageError("archive.integrity_mismatch")
    license_inventory = json.loads(files["LICENSE-INVENTORY.json"])
    expected_licenses = {}
    for package in license_inventory.get("packages", []):
        for record in package.get("license_files", []):
            expected_licenses[record["path"]] = record["sha256"]
    if set(license_names) != set(expected_licenses):
        raise PackageError("archive.license_inventory_mismatch")
    if any(sha(files[path]) != digest for path, digest in expected_licenses.items()):
        raise PackageError("archive.license_digest_mismatch")
    sbom = json.loads(files["BUILD-SBOM.json"])
    for package in sbom.get("packages", []):
        for record in package.get("license_files", []):
            if record["path"] not in files or sha(files[record["path"]]) != record["sha256"]:
                raise PackageError("archive.sbom_license_link_invalid")
    if not files["bin/agent-router"]:
        raise PackageError("archive.binary_empty")
    return files


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--diagnostics-dir", type=Path, required=True)
    parser.add_argument("--cargo-cache", type=Path, default=Path.home() / ".cargo")
    parser.add_argument("--cargo", type=Path, default=Path(shutil.which("cargo") or "cargo"))
    parser.add_argument("--rustc", type=Path, default=Path(shutil.which("rustc") or "rustc"))
    parser.add_argument("--rustup-home", type=Path, default=Path.home() / ".rustup")
    parser.add_argument("--target", required=True)
    parser.add_argument("--features", default="")
    args = parser.parse_args()
    try:
        if not re_hex(args.source_sha256):
            raise PackageError("source.sha256_invalid")
        if args.output.exists() or args.output.is_symlink() or args.diagnostics_dir.exists() or args.diagnostics_dir.is_symlink():
            raise PackageError("output.already_exists")
        source = checked_archive(args.source_archive, args.source_sha256)
        features = [item for item in args.features.split(",") if item]
        if features != sorted(set(features)):
            raise PackageError("features.must_be_sorted_and_unique")
        check_cargo_config_ancestors(Path.cwd().resolve())
        check_cargo_config_ancestors(args.output.parent.resolve())
        args.diagnostics_dir.mkdir(mode=0o700, parents=True)
        args.diagnostics_dir.chmod(0o700)
        cargo_binary = resolve_tool(args.cargo, "cargo")
        rustc_binary = resolve_tool(args.rustc, "rustc")
        rustc_details = subprocess.run([str(rustc_binary), "-Vv"], capture_output=True, timeout=10, check=True).stdout.decode()
        cargo_details = subprocess.run([str(cargo_binary), "-Vv"], capture_output=True, timeout=10, check=True).stdout.decode()
        host_target = next((line.split(": ", 1)[1] for line in rustc_details.splitlines() if line.startswith("host: ")), None)
        if "release: 1.85.1" not in rustc_details or not cargo_details.startswith("cargo 1.85.1 "):
            raise PackageError("toolchain.version_mismatch")
        if args.target != host_target:
            raise PackageError("target.must_match_native_host")
        with tempfile.TemporaryDirectory(prefix="agent-router-binary-", dir=args.output.parent) as temporary:
            root = Path(temporary)
            vendor_root = root / "vendor"
            vendor_root.mkdir()
            vendor_inventory, vendor_digest = vendor(source["Cargo.lock"], args.cargo_cache, vendor_root)
            outputs, reports = [], []
            for index in (1, 2):
                run_root = root / f"run-{index}"
                run_root.mkdir()
                result = run_build(source, vendor_root, run_root, cargo_binary, rustc_binary, args.rustup_home.resolve(), args.target, features, vendor_inventory, args.diagnostics_dir / f"run-{index}.log")
                result["vendor_sha256"] = vendor_digest
                outputs.append(result.pop("binary"))
                reports.append(result)
            if outputs[0] != outputs[1]:
                raise PackageError("build.reproducibility_mismatch")
            if reports[0]["build_packages"] != reports[1]["build_packages"]:
                raise PackageError("build.observed_package_set_mismatch")
            proof = {"features": features, "target": args.target, "vendor_inventory": vendor_inventory,
                     "build_packages": reports[0]["build_packages"], "source_sha256": args.source_sha256,
                     "source_manifest_sha256": reports[0]["source_manifest_sha256"],
                     "lock_sha256": reports[0]["lock_sha256"], "vendor_sha256": vendor_digest,
                     "cargo_version": reports[0]["cargo"], "rustc_version": reports[0]["rustc"],
                     "cargo_binary_sha256": reports[0]["cargo_binary_sha256"], "rustc_binary_sha256": reports[0]["rustc_binary_sha256"],
                     "c_compiler": reports[0]["c_compiler"], "sdk": reports[0]["sdk"],
                     "dynamic_libraries_declared": reports[0]["dynamic_libraries_declared"]}
            proof_a = dict(proof, cargo_version=reports[0]["cargo"], rustc_version=reports[0]["rustc"])
            proof_b = dict(proof, cargo_version=reports[1]["cargo"], rustc_version=reports[1]["rustc"])
            for target_proof, report in ((proof_a, reports[0]), (proof_b, reports[1])):
                for key in ("cargo_binary_sha256", "rustc_binary_sha256", "c_compiler", "sdk", "dynamic_libraries_declared"):
                    target_proof[key] = report[key]
            for target_proof, report in ((proof_a, reports[0]), (proof_b, reports[1])):
                for key in ("cargo_binary_sha256", "rustc_binary_sha256", "c_compiler", "sdk", "dynamic_libraries_declared"):
                    target_proof[key] = report[key]
            payload = archive_payload(outputs[0], proof_a, source)
            second_payload = archive_payload(outputs[1], proof_b, source)
            if payload != second_payload:
                raise PackageError("build.normalized_archive_mismatch")
            with tempfile.NamedTemporaryFile(dir=args.output.parent, prefix=".agent-router-binary-", delete=False) as file:
                staged = Path(file.name)
                file.write(payload)
            try:
                os.link(staged, args.output)
            finally:
                staged.unlink()
            report = {"schema": "agent-router-binary-run-v1", "source_sha256": args.source_sha256,
                      "binary_sha256": sha(outputs[0]), "archive_sha256": sha(payload), "reproducible": True,
                      "diagnostics": str(args.diagnostics_dir), "builds": reports}
            print(json.dumps(report, sort_keys=True))
    except (PackageError, OSError, ValueError, KeyError, TypeError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(json.dumps({"schema": "agent-router-binary-run-v1", "error": str(error) if isinstance(error, PackageError) else type(error).__name__}, sort_keys=True))
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
