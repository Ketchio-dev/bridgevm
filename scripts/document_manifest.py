#!/usr/bin/env python3
"""Read the document catalog and its explicitly included, single-level shards."""
from __future__ import annotations

from pathlib import Path
import re
import sys

HEADER = "path\tclass\ttopic\tsuperseded_by"
SHARD = re.compile(r"docs/document-manifests/[A-Za-z0-9][A-Za-z0-9_.-]*\.tsv\Z")


def rows(path: Path) -> list[tuple[str, ...]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != HEADER:
        raise ValueError(f"invalid documentation manifest header: {path}")
    result = []
    for number, line in enumerate(lines[1:], 2):
        if not line:
            continue
        fields = tuple(line.split("\t"))
        if len(fields) != 4 or any(not field for field in fields):
            raise ValueError(f"invalid documentation manifest columns: {path}:{number}")
        result.append(fields)
    return result


def load_manifest(manifest: Path) -> list[tuple[str, ...]]:
    root = manifest.parent.parent
    documents: list[tuple[str, ...]] = []
    seen_paths: set[str] = set()
    seen_shards: set[str] = set()

    def append(record: tuple[str, ...]) -> None:
        path = record[0]
        if path == "@include":
            raise ValueError("nested documentation manifest includes are forbidden")
        if path in seen_paths:
            raise ValueError(f"duplicate manifest path: {path}")
        seen_paths.add(path)
        documents.append(record)

    for record in rows(manifest):
        if record[0] != "@include":
            append(record)
            continue
        _, shard, topic, superseded_by = record
        if not SHARD.fullmatch(shard) or (topic, superseded_by) != ("-", "-"):
            raise ValueError(f"invalid documentation manifest include: {shard}")
        if shard in seen_shards:
            raise ValueError(f"duplicate documentation manifest include: {shard}")
        seen_shards.add(shard)
        path = root / shard
        try:
            path.resolve().relative_to(root.resolve())
        except ValueError:
            raise ValueError(f"documentation manifest shard escapes the repository: {shard}")
        if path.is_symlink() or path.parent.is_symlink() or not path.is_file():
            raise ValueError(f"documentation manifest shard is missing or unsafe: {shard}")
        for included in rows(path):
            append(included)
    return documents


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: document_manifest.py MANIFEST", file=sys.stderr)
        return 2
    try:
        records = load_manifest(Path(sys.argv[1]))
    except (OSError, UnicodeError, ValueError) as error:
        print(f"documentation manifest: {error}", file=sys.stderr)
        return 1
    print(HEADER)
    for record in records:
        print("\t".join(record))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
