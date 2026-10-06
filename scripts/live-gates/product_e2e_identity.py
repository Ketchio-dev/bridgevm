"""Exact JSON identities and the request bytes authorized before a helper launch."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

from native_snapshot_restore_seal import read_bounded_regular

SHA256 = re.compile(r"[0-9a-f]{64}\Z")


def fixed_fields_match(value: dict, expected: dict) -> bool:
    return all(type(value.get(key)) is type(item) and value.get(key) == item
               for key, item in expected.items())


def is_sha256(value: object) -> bool:
    return type(value) is str and SHA256.fullmatch(value) is not None


def unique(pairs: list[tuple[str, object]]) -> dict:
    value: dict = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate field: {key}")
        value[key] = item
    return value


def sealed_request(path: Path, expected_sha256: str) -> object:
    if not is_sha256(expected_sha256):
        raise ValueError("prelaunch request SHA-256 is invalid")
    data = read_bounded_regular(path, 1024 * 1024)
    if hashlib.sha256(data).hexdigest() != expected_sha256:
        raise ValueError("request differs from its prelaunch SHA-256")
    return json.loads(data.decode("utf-8"), object_pairs_hook=unique)
