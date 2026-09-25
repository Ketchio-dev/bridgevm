"""Bounded public field and JSON parsing boundary for T22 receipts."""
from __future__ import annotations

from datetime import datetime
import json
from pathlib import Path
import re

from a19_interrupted_restore_seal import read_bounded_regular

HOST_MODEL = re.compile(r"(?:Mac|iMac)[A-Za-z0-9]{0,31}\d+,\d{1,2}\Z")
MACOS_VERSION = re.compile(r"\d{1,2}\.\d{1,2}(?:\.\d{1,2})?\Z")
UTC_TIME = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?(?:Z|\+00:00)\Z")


def _timestamp(value: object, field: str) -> datetime:
    if type(value) is not str or len(value) > 40 or not UTC_TIME.fullmatch(value):
        raise ValueError(f"T22 receipt {field} is not a bounded UTC timestamp")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError(f"T22 receipt {field} is not a valid UTC time") from error


def validate_public_fields(value: dict) -> None:
    started = _timestamp(value["started_at"], "started_at")
    finished = _timestamp(value["finished_at"], "finished_at")
    if finished < started:
        raise ValueError("T22 receipt finishes before it starts")
    for field, pattern in (("host_model", HOST_MODEL), ("macos_version", MACOS_VERSION)):
        item = value[field]
        if item == "absent" and value["pass"] is False:
            continue
        if type(item) is not str or len(item) > 64 or not pattern.fullmatch(item):
            raise ValueError(f"T22 receipt {field} is not bounded public host data")


def _unique_fields(pairs: list[tuple[str, object]]) -> dict:
    value: dict = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("T22 receipt has a repeated JSON field")
        value[key] = item
    return value


def _reject_constant(value: str) -> None:
    raise ValueError(f"T22 receipt has a nonfinite JSON constant: {value}")


def load_receipt(path: Path) -> dict:
    return json.loads(read_bounded_regular(path, 65_536).decode("utf-8"),
                      object_pairs_hook=_unique_fields, parse_constant=_reject_constant)
