#!/usr/bin/env python3
"""Create a deterministic synthetic upstream notice set for parser smokes."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "graphics_notices", ROOT / "scripts/package-windows-graphics-notices.py"
)
if SPEC is None or SPEC.loader is None:
    raise SystemExit("cannot load Windows graphics notice packager")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} OUTPUT_DIRECTORY")
    MODULE.create_self_test_fixture(Path(sys.argv[1]).resolve())
    return 0


if __name__ == "__main__":
    sys.exit(main())
