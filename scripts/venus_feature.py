"""Which files the venus feature gates, and whether it can link here.

Split out of check-tests-are-reachable.py: the feature is an external
virglrenderer build, and deciding what it covers is its own question.
"""

from __future__ import annotations

import os
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent


def venus_links() -> bool:
    """Whether the venus feature can link here: build.rs wants
    -lvirglrenderer from BRIDGEVM_VENUS_PREFIX or ~/BridgeVM/3d/prefix, and CI
    only `cargo check`s that feature, so the library is absent there."""
    prefix = os.environ.get("BRIDGEVM_VENUS_PREFIX")
    root = pathlib.Path(prefix) if prefix else pathlib.Path.home() / "BridgeVM/3d/prefix"
    return (root / "lib/libvirglrenderer.dylib").exists()


def venus_gated_paths() -> set[str]:
    """Files that only exist with the venus feature, by module declaration."""
    gated = set()
    for module in ROOT.glob("crates/**/*.rs"):
        text = module.read_text(errors="ignore")
        for match in re.finditer(
            r'#\[cfg\(feature\s*=\s*"venus"\)\]\s*\n\s*(?:pub\s+)?mod\s+(\w+)\s*;', text
        ):
            name = match.group(1)
            single = module.parent / f"{name}.rs"
            if single.exists():
                gated.add(str(single.relative_to(ROOT)))
            directory = module.parent / name
            if directory.is_dir():
                for nested in directory.rglob("*.rs"):
                    gated.add(str(nested.relative_to(ROOT)))
    return gated
