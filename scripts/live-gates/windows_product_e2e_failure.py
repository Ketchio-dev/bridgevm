#!/usr/bin/env python3
"""Map authenticated private T17 lane failures into the stable public taxonomy."""
from __future__ import annotations

LANE_TO_PUBLIC = {
    "invalid-request": "product-model-failed",
    "accessibility-untrusted": "product-model-failed",
    "app-launch-failed": "product-model-failed",
    "ui-element-missing": "product-model-failed",
    "input-selection-failed": "product-model-failed",
    "vm-creation-failed": "product-model-failed",
    "snapshot-unavailable": "snapshot-failed",
}


def public_code(code: str, lanes: list[dict], allowed: set[str]) -> str:
    if code in allowed:
        return code
    if code == "guest-evidence-missing":
        return "integration-failed" if lanes and lanes[-1].get("first_ready") is True else "first-boot-failed"
    mapped = LANE_TO_PUBLIC.get(code, "internal-error")
    return mapped if mapped in allowed else "internal-error"
