#!/usr/bin/env python3
"""Bundle top-level license/notice files for every locked third-party crate."""

import argparse
import json
import subprocess
from pathlib import Path

NAMES = ("LICENSE*", "COPYING*", "NOTICE*", "license*", "copying*", "notice*")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    metadata = json.loads(
        subprocess.check_output(["scripts/rust-distribution-packages.py"], text=True)
    )
    sections = []
    for package in metadata:
        root = Path(package["manifest_path"]).parent
        notices = sorted(
            {path for pattern in NAMES for path in root.glob(pattern) if path.is_file()}
        )
        if not notices:
            raise SystemExit(f"no license/notice file found for {package['id']}")
        texts = []
        for notice in notices:
            try:
                text = notice.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                text = notice.read_text(encoding="latin-1")
            texts.append(f"--- {notice.name} ---\n{text.rstrip()}\n")
        sections.append(
            f"=== {package['name']} {package['version']} ===\n"
            f"license-expression: {package.get('license')}\n" + "\n".join(texts)
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n\n".join(sections) + "\n", encoding="utf-8")
    print(f"rust_license_package_count={len(sections)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
