#!/usr/bin/env python3
"""Hash the media pair the product selects now: `snapshot_pair_cli digest` under the pair lease.

After a restore the original disk and vars paths keep their old bytes and the selected
generation lives under the managed root (crates/bridgevm-hvf/src/managed_pair.rs). Studio
T17 r80 completed every journey stage and was refused because its final media were hashed
at the original paths. The helper is the sealed app bundle's own binary.
"""
from __future__ import annotations
from windows_selected_media_cli import selected

def selected_digests(request: dict) -> tuple[str, str]:
    values = selected(request)
    return values["disk_sha256"], values["vars_sha256"]
