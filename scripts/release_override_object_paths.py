#!/usr/bin/env python3
"""List release-gate objects, omitting only declared SwiftPM XCTest targets."""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / "apps/macos/.build"


class SelectionError(Exception):
    pass


def test_targets() -> set[str]:
    try:
        result = subprocess.run(
            ["swift", "package", "--package-path", str(ROOT / "apps/macos"), "dump-package"],
            cwd=ROOT, capture_output=True, text=True, check=False,
        )
        if result.returncode != 0:
            raise SelectionError("SwiftPM target inventory is unavailable")
        targets = json.loads(result.stdout)["targets"]
        if not isinstance(targets, list) or not targets:
            raise SelectionError("SwiftPM target inventory is empty")
        names: set[str] = set()
        tests: set[str] = set()
        for target in targets:
            name, kind = target["name"], target["type"]
            if not isinstance(name, str) or not name or not isinstance(kind, str) or name in names:
                raise SelectionError("SwiftPM target inventory is ambiguous")
            names.add(name)
            if kind == "test":
                tests.add(name)
        if not tests:
            raise SelectionError("SwiftPM declares no XCTest targets")
        return tests
    except (OSError, ValueError, KeyError, TypeError) as exc:
        raise SelectionError("SwiftPM target inventory could not be parsed") from exc


def object_paths(config: str, tests: set[str], build: Path = BUILD) -> list[Path]:
    if config not in {"debug", "release"}:
        raise SelectionError("build configuration must be debug or release")
    title = config.title()
    test_dirs = {f"{name}{suffix}.build" for name in tests for suffix in ("", "-p")}
    selected: list[Path] = []
    found = 0
    legacy = build / "arm64-apple-macosx" / config
    if legacy.exists():
        for path in legacy.rglob("*.o"):
            found += 1
            if path.relative_to(legacy).parts[0] not in test_dirs:
                selected.append(path)
    modern = build / "out/Intermediates.noindex"
    if modern.exists():
        for path in modern.rglob("*.o"):
            parts = path.relative_to(modern).parts
            if title not in parts:
                continue
            found += 1
            index = parts.index(title)
            target_dir = parts[index + 1] if index + 1 < len(parts) else ""
            if target_dir not in test_dirs:
                selected.append(path)
    if found and not selected:
        raise SelectionError("build contains XCTest objects but no product objects")
    return sorted(selected)


def main() -> int:
    try:
        if len(sys.argv) != 2:
            raise SelectionError("expected one build configuration")
        for path in object_paths(sys.argv[1], test_targets()):
            print(path)
    except SelectionError as exc:
        print(f"release overrides: FAIL ({exc})", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
