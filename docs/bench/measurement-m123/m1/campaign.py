"""Bounded offline preparation. This module never invokes a model or grants authority."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import time

MAX_BYTES = 262144
MAX_MODELS = 64
EFFORTS = ("low", "medium", "high", "xhigh", "max", "ultra")
STAGES = ("plan", "execute", "verify")


def digest(value):
    """SHA-256 of UTF-8 compact sorted-key JSON, without a trailing newline."""
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def load_json(path):
    with Path(path).open("rb") as stream:
        raw = stream.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError("JSON byte bound exceeded")
    try:
        return json.loads(raw, object_pairs_hook=_pairs, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON number")))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("invalid JSON") from error


def _keys(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError("invalid object fields")


def _text(value, maximum=4096):
    if not isinstance(value, str) or not value.strip() or len(value) > maximum:
        raise ValueError("invalid bounded string")


def _model(value):
    _text(value, 128)
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", value):
        raise ValueError("invalid model identifier")


def _choices(value, allowed):
    if not isinstance(value, list) or not value or len(value) > len(allowed) or any(not isinstance(item, str) or item not in allowed for item in value) or len(set(value)) != len(value):
        raise ValueError("invalid bounded choices")


def prepare(catalog, host, provenance, now=None, max_age_seconds=86400):
    """Return declared model/effort eligibility, distinct from inference evidence."""
    _keys(catalog, ("schema_version", "version", "models", "skills"))
    _keys(host, ("id", "harness", "supported_models", "supported_efforts", "capabilities"))
    _keys(provenance, ("schema_version", "observed_at_unix_seconds", "observed_at_utc", "source", "source_kind", "host_sha256", "evidence_scope", "provider_identity", "provider_inference", "capability_authentication", "native_runtime_qualification"))
    if type(catalog["schema_version"]) is not int or catalog["schema_version"] != 1 or type(provenance["schema_version"]) is not int or provenance["schema_version"] != 1:
        raise ValueError("unsupported schema")
    _text(catalog["version"], 128)
    _text(host["id"], 128)
    if host["harness"] != "codex" or host["capabilities"] != [] or catalog["skills"] != []:
        raise ValueError("M1 supports only capability-free Codex metadata preparation")
    models = catalog["models"]
    available = host["supported_models"]
    supported_efforts = host["supported_efforts"]
    if not isinstance(models, list) or not 1 <= len(models) <= MAX_MODELS or not isinstance(available, list) or not 1 <= len(available) <= MAX_MODELS or not isinstance(supported_efforts, dict):
        raise ValueError("model bound exceeded")
    for model in available:
        _model(model)
    if len(set(available)) != len(available) or set(supported_efforts) != set(available):
        raise ValueError("host model/effort mismatch")
    for efforts in supported_efforts.values():
        _choices(efforts, EFFORTS)
    observed_at = provenance["observed_at_unix_seconds"]
    now = int(time.time()) if now is None else now
    if type(observed_at) is not int or observed_at < 0 or type(now) is not int or type(max_age_seconds) is not int or not 0 <= max_age_seconds <= 86400:
        raise ValueError("invalid observation clock")
    if observed_at > now or now - observed_at > max_age_seconds:
        raise ValueError("future or stale metadata")
    if provenance["observed_at_utc"] != datetime.fromtimestamp(observed_at, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"):
        raise ValueError("observation dates differ")
    for field in ("source", "evidence_scope"):
        _text(provenance[field])
    if provenance["source_kind"] != "platform_metadata" or provenance["host_sha256"] != digest(host):
        raise ValueError("metadata provenance mismatch")
    for field in ("provider_identity", "provider_inference", "capability_authentication", "native_runtime_qualification"):
        if provenance[field] != "UNKNOWN":
            raise ValueError("platform metadata cannot establish runtime evidence")
    eligible, excluded, seen = [], [], set()
    for model in models:
        _keys(model, ("id", "stages", "efforts", "capabilities", "quality_micros", "cost_micros", "latency_micros", "source"))
        _model(model["id"])
    for model in sorted(models, key=lambda item: item["id"]):
        identifier = model["id"]
        if identifier in seen:
            raise ValueError("duplicate catalog model")
        seen.add(identifier)
        _choices(model["stages"], STAGES)
        _choices(model["efforts"], EFFORTS)
        _text(model["source"])
        if model["capabilities"] != []:
            raise ValueError("M1 metadata grants no capabilities")
        for field in ("quality_micros", "cost_micros", "latency_micros"):
            if type(model[field]) is not int or not 0 <= model[field] <= 1000000:
                raise ValueError("invalid policy prior")
        if identifier not in available:
            excluded.append({"model": identifier, "effort": None, "reason": "model_absent"})
            continue
        for effort in EFFORTS:
            if effort not in model["efforts"]:
                continue
            if effort not in supported_efforts[identifier]:
                excluded.append({"model": identifier, "effort": effort, "reason": "effort_incompatible"})
                continue
            eligible.append({"model": identifier, "effort": effort, "stages": [stage for stage in STAGES if stage in model["stages"]]})
    return {"schema_version": 1, "status": "READY_FOR_PREPARATION" if len({c["model"] for c in eligible}) >= 2 else "INSUFFICIENT_MODEL_CHOICE", "eligible": eligible, "excluded": excluded, "catalog_sha256": digest(catalog), "host_sha256": digest(host), "provenance_sha256": digest(provenance), "hash_encoding": "sha256:utf8-json-sort-keys-compact-no-newline", "observed_at_unix_seconds": observed_at, "policy_scores": "UNCALIBRATED_PRIORS_NOT_MONEY", "provider_inference": "UNKNOWN", "provider_identity": "UNKNOWN", "execution_authorized": False}


def _identity(value, allow_unknown=False):
    if value is None and allow_unknown:
        return
    _keys(value, ("model", "effort"))
    if value["model"] is None and allow_unknown:
        pass
    else:
        _model(value["model"])
    if value["effort"] is None and allow_unknown:
        pass
    elif value["effort"] not in EFFORTS:
        raise ValueError("invalid identity effort")


def assess_identity(requested, observed, declaration=None):
    """Compare caller-supplied identities; do not authenticate the provider."""
    _identity(requested)
    _identity(observed, allow_unknown=True)
    if declaration is not None:
        _keys(declaration, ("requested", "observed", "reason"))
        _text(declaration["reason"])
        if declaration["requested"] != requested or declaration["observed"] != observed or observed == requested:
            raise ValueError("substitution declaration mismatch")
    if observed is None or None in observed.values():
        status, matches = "UNKNOWN", None
    elif requested == observed:
        status, matches = "MATCHED_SUPPLIED", True
    else:
        status, matches = ("SUBSTITUTION_DECLARED" if declaration else "SUBSTITUTION_UNDECLARED"), False
    return {"schema_version": 1, "status": status, "requested": requested, "observed": observed, "declaration": declaration, "matches_requested": matches, "provider_identity_verified": False, "evidence_scope": "comparison_of_supplied_identity_only"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    base = Path(__file__).resolve().parent
    for name, default in (("catalog", "catalog.json"), ("host", "host-snapshot.json"), ("provenance", "provenance.json")):
        prep.add_argument("--" + name, type=Path, default=base / default)
    prep.add_argument("--now", type=int, help="Replay clock only; omit for live freshness validation")
    identity = commands.add_parser("identity")
    identity.add_argument("--record", type=Path, required=True, help="JSON with requested, observed, declaration")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            result = prepare(load_json(args.catalog), load_json(args.host), load_json(args.provenance), args.now)
            exit_code = 0 if result["status"] == "READY_FOR_PREPARATION" else 3
        else:
            record = load_json(args.record)
            _keys(record, ("requested", "observed", "declaration"))
            result = assess_identity(record["requested"], record["observed"], record["declaration"])
            exit_code = 0 if result["status"] == "MATCHED_SUPPLIED" else 3
    except (ValueError, OSError, TypeError, OverflowError) as error:
        result, exit_code = {"schema_version": 1, "status": "INVALID", "reason": str(error)}, 2
    print(json.dumps(result, sort_keys=True, indent=2))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
