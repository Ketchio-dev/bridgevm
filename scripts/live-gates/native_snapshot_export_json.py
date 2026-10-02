"""Stable, bounded JSON bytes for native snapshot export evidence.

Metadata hashes must describe the same bytes whose fields were checked. Reject
duplicate fields and nonfinite constants before interpreting a success result.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from native_snapshot_restore_public import _reject_constant, _unique_fields
from native_snapshot_restore_seal import read_bounded_regular

LIMIT = 65_536


def load_json(path: Path) -> tuple[object, str]:
    data = read_bounded_regular(path, LIMIT)
    value = json.loads(data.decode("utf-8"), object_pairs_hook=_unique_fields,
                       parse_constant=_reject_constant)
    return value, hashlib.sha256(data).hexdigest()
