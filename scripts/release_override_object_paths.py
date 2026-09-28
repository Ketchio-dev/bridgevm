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

def declared_targets() -> tuple[set[str], set[str]]:
    try:
        package = json.loads(subprocess.check_output(
            ["swift", "package", "--package-path", str(ROOT / "apps/macos"), "dump-package"],
            cwd=ROOT, stderr=subprocess.DEVNULL, text=True, timeout=60,
        ))
        targets = package["targets"]
        if not isinstance(targets, list) or not targets:
            raise SelectionError("SwiftPM target inventory is empty")
        kinds: dict[str, str] = {}
        tests: set[str] = set()
        for target in targets:
            name, kind = target["name"], target["type"]
            if not isinstance(name, str) or not name or not isinstance(kind, str) or name in kinds:
                raise SelectionError("SwiftPM target inventory is ambiguous")
            kinds[name] = kind
            if kind == "test":
                tests.add(name)
        products: set[str] = set()
        for product in package["products"]:
            if "executable" in product["type"]:
                products.update(product["targets"] or [""])
        if not tests or not products or any(kinds.get(name) != "executable" for name in products):
            raise SelectionError("SwiftPM test or executable product inventory is incomplete")
        return tests, products
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
        raise SelectionError("SwiftPM target inventory could not be parsed") from exc

def object_paths(config: str, tests: set[str], products: set[str], build: Path = BUILD) -> list[Path]:
    if config not in {"debug", "release"}:
        raise SelectionError("build configuration must be debug or release")
    title = config.title()
    test_dirs = {f"{name}{suffix}.build" for name in tests for suffix in ("", "-p")}
    selected: list[Path] = []
    present: set[str] = set()
    legacy = build / "arm64-apple-macosx" / config
    if legacy.exists():
        for path in legacy.rglob("*.o"):
            target_dir = path.relative_to(legacy).parts[0]
            present.add(target_dir)
            if target_dir not in test_dirs:
                selected.append(path)
    modern = build / "out/Intermediates.noindex"
    if modern.exists():
        for path in modern.rglob("*.o"):
            parts = path.relative_to(modern).parts
            if title not in parts:
                continue
            index = parts.index(title)
            target_dir = parts[index + 1] if index + 1 < len(parts) else ""
            present.add(target_dir)
            if target_dir not in test_dirs:
                selected.append(path)
    if present and not selected:
        raise SelectionError("build contains XCTest objects but no product objects")
    missing = {name for name in products if not {f"{name}.build", f"{name}-p.build"} & present}
    if present and missing:
        raise SelectionError(f"missing executable product objects: {', '.join(sorted(missing))}")
    return sorted(selected)

def main() -> int:
    try:
        if len(sys.argv) != 2:
            raise SelectionError("expected one build configuration")
        tests, products = declared_targets()
        for path in object_paths(sys.argv[1], tests, products):
            print(path)
    except SelectionError as exc:
        print(f"release overrides: FAIL ({exc})", file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
