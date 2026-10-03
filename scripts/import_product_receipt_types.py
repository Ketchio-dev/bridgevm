"""Strict primitive types used by the installed-disk import receipt validator."""
from __future__ import annotations

import re
from datetime import datetime, timezone

TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
SIGNING_CLASSES = ("unverified", "development-ad-hoc", "development-signed", "developer-id-notarized")

class ReceiptError(ValueError): pass

def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value: raise ReceiptError(f"duplicate field: {key}")
        value[key] = item
    return value

def integer(value, name, minimum=0):
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ReceiptError(f"{name} must be an integer >= {minimum}")
    return value

def timestamp(value, name):
    if not isinstance(value, str) or not TIMESTAMP.fullmatch(value):
        raise ReceiptError(f"{name} is not a UTC timestamp")
    return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)

def validate_signing_class(value):
    if not isinstance(value, str) or value not in SIGNING_CLASSES:
        raise ReceiptError("artifact_signing_class is not recognized")
