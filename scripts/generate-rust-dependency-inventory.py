#!/usr/bin/env python3
"""Write a deterministic, locked inventory of all third-party Rust packages."""

import argparse
import json
import subprocess
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    metadata = json.loads(
        subprocess.check_output(["scripts/rust-distribution-packages.py"], text=True)
    )
    packages = []
    for package in metadata:
        license_expression = package.get("license")
        source = package.get("source")
        if not license_expression or not source:
            raise SystemExit(
                f"third-party package lacks license/source metadata: {package['id']}"
            )
        packages.append(
            (package["name"], package["version"], license_expression, source)
        )

    packages.sort()
    lines = [
        "format=bridgevm-rust-dependencies-v1",
        "scope=normal/build dependency closure of shipped Rust roots",
        "name\tversion\tlicense\tsource",
    ]
    lines.extend("\t".join(package) for package in packages)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"rust_dependency_count={len(packages)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
