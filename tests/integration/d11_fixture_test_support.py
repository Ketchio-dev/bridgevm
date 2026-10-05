"""Owned, tiny synthetic inputs; never a guest or live receipt."""
import hashlib
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
sys.path.insert(0, str(ROOT / "scripts"))
from d11_fixture_inputs import FILES, TREES, SCHEMA, FIRMWARE

COMMIT = "a" * 40
HASH = "b" * 64


def rows(base):
    value = {key: [str(base / key), HASH] for key in FILES | TREES}
    value.update({"schema": [SCHEMA], "classification": ["DEVELOPMENT_ONLY"], "source_commit": [COMMIT],
                  "binary_source_commit": ["c" * 40], "binary_profile": ["release"], "binary_features": ["venus"],
                  "rust_toolchain": ["1.93.0"], "container_gib": ["24"]})
    value["firmware"][1] = FIRMWARE
    return value


def manifest(value):
    return "".join("\t".join((k, *v)) + "\n" for k, v in sorted(value.items())).encode()


def binding():
    return {"job_id": "d11-synthetic", "tier": "d11-native-fixture-preparation", "commit": COMMIT,
            "input_manifest_sha256": HASH, "sealed_binary_sha256": HASH}


def guest(nonce="d" * 64):
    return {"schema": "bridgevm.d11-fixture-ready.v1", "nonce": nonce, "script_sha256": HASH,
            "account_bound": True, "password_never_expires": True, "agent_task_registered": True,
            "desktop_observed": True, "autologon_remaining": 3}
