"""Inspect the pinned native schema without starting an agent or provider session."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

NATIVE_SHA256 = "16593cc2f422d5f398a8e40f550ebbaf1245392528957be342c295920a300704"
VERSION = "codex-cli 0.159.2"
EXPECTED_METHODS = {
    "item/commandExecution/requestApproval", "item/fileChange/requestApproval",
    "item/tool/requestUserInput", "mcpServer/elicitation/request",
    "item/permissions/requestApproval", "item/tool/call",
    "account/chatgptAuthTokens/refresh", "attestation/generate",
    "currentTime/read", "applyPatchApproval", "execCommandApproval",
}


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def inspect(binary):
    binary = binary.resolve(strict=True)
    if not binary.is_file() or digest(binary) != NATIVE_SHA256:
        raise ValueError("native binary pin mismatch; no execution")
    with tempfile.TemporaryDirectory(prefix="ar-entry-schema-") as directory:
        root = Path(directory)
        home = root / "home"
        home.mkdir(mode=0o700)
        env = {"HOME": str(home), "CODEX_HOME": str(home / "codex"),
               "PATH": "/usr/bin:/bin", "TMPDIR": str(root)}
        commands = [
            ("version", ["--version"]),
            ("help", ["app-server", "--help"]),
            ("schema", ["app-server", "generate-json-schema", "--experimental",
                        "--out", str(root / "schema")]),
        ]
        exits = {}
        for name, args in commands:
            result = subprocess.run([str(binary), *args], cwd=root, env=env,
                                    capture_output=True, timeout=30, check=True)
            exits[name] = result.returncode
            if name == "version" and result.stdout.decode().strip() != VERSION:
                raise ValueError("native version mismatch")
        projections = {}
        for name in ["ServerRequest.json", "v2/ThreadStartParams.json", "v2/TurnStartParams.json"]:
            path = root / "schema" / name
            if path.stat().st_size > 2 * 1024 * 1024:
                raise ValueError("schema size exceeded")
            data = json.loads(path.read_bytes())
            entry = {"sha256": digest(path)}
            if name == "ServerRequest.json":
                methods = [item["properties"]["method"]["enum"][0]
                           for item in data["oneOf"]]
                if len(methods) != len(EXPECTED_METHODS) or set(methods) != EXPECTED_METHODS:
                    raise ValueError("server request method drift; re-audit required")
                entry["methods"] = sorted(methods)
            else:
                fields = data["properties"]
                required = ({"model", "effort", "approvalPolicy", "approvalsReviewer", "sandboxPolicy"}
                            if name.endswith("TurnStartParams.json") else
                            {"model", "modelProvider", "approvalPolicy", "approvalsReviewer", "sandbox", "config"})
                if not required <= fields.keys():
                    raise ValueError("native parameter drift")
                entry["selected_parameter_fields"] = sorted(required)
            projections[name] = entry
        if digest(binary) != NATIVE_SHA256:
            raise ValueError("native binary changed during inspection")
    return {"schema_version": 1, "evidence_kind": "native_schema_discovery",
            "native_version": "0.159.2", "native_sha256": NATIVE_SHA256,
            "command_exit_codes": exits, "schemas": projections,
            "generated_provider_send_admission_callback": "ABSENT_IN_ENUMERATED_SERVER_REQUESTS",
            "provider_call_executed": False, "native_transport_compatibility": "UNKNOWN",
            "physical_send_budget_enforcement": "UNKNOWN"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(inspect(args.binary), indent=2, sort_keys=True))
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"inspection failed: {type(error).__name__}\n")
