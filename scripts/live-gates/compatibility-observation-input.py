#!/usr/bin/env python3
"""Seal and verify the private inputs for the 20-AppX observation tier."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import os
import re
import stat
from pathlib import Path

KEYS = ("image", "vars", "candidates", "b4_manifest")
COLUMNS = (
    "id", "package", "app_id", "executable", "executable_sha256",
    "blockmap_sha256", "version", "static_graphics_imports",
)
HEX = re.compile(r"[0-9a-f]{64}")
IDENT = re.compile(r"[a-z0-9][a-z0-9-]{2,63}")
TOKEN = re.compile(r"[A-Za-z0-9._-]+")
MAX_SMALL = 1_000_000


class Refusal(ValueError):
    pass


def read_regular(path: Path, limit: int | None = None) -> bytes:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags)
    except OSError as error:
        raise Refusal(f"cannot open regular input {path.name}: {error}") from error
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise Refusal(f"input is not regular: {path.name}")
        if limit is not None and info.st_size > limit:
            raise Refusal(f"input is oversized: {path.name}")
        data = bytearray()
        while block := os.read(fd, 1024 * 1024):
            data.extend(block)
            if limit is not None and len(data) > limit:
                raise Refusal(f"input is oversized: {path.name}")
        return bytes(data)
    finally:
        os.close(fd)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags)
    except OSError as error:
        raise Refusal(f"cannot hash regular input {path.name}: {error}") from error
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise Refusal(f"input is not regular: {path.name}")
        while block := os.read(fd, 1024 * 1024):
            value.update(block)
    finally:
        os.close(fd)
    return value.hexdigest()


def read_manifest(path: Path) -> dict[str, tuple[Path, str]]:
    raw = read_regular(path, 65536)
    if not raw.endswith(b"\n") or b"\r" in raw or b"\0" in raw:
        raise Refusal("observation manifest framing is invalid")
    lines = raw.decode("utf-8").splitlines()
    if len(lines) != len(KEYS):
        raise Refusal("observation manifest must have exactly four rows")
    result: dict[str, tuple[Path, str]] = {}
    for expected, line in zip(KEYS, lines, strict=True):
        fields = line.split("\t")
        if len(fields) != 3 or fields[0] != expected:
            raise Refusal("observation manifest keys are missing, reordered, or duplicated")
        source, expected_hash = Path(fields[1]), fields[2]
        if not source.is_absolute() or not HEX.fullmatch(expected_hash):
            raise Refusal(f"invalid path or SHA-256 for {expected}")
        result[expected] = source, expected_hash
    return result


def validate_candidates(path: Path) -> list[dict[str, str]]:
    raw = read_regular(path, MAX_SMALL)
    if not raw.endswith(b"\n") or b"\r" in raw or b"\0" in raw:
        raise Refusal("candidate TSV framing is invalid")
    reader = csv.DictReader(io.StringIO(raw.decode("utf-8")), delimiter="\t")
    if tuple(reader.fieldnames or ()) != COLUMNS:
        raise Refusal("candidate TSV columns differ from the fixed schema")
    rows = list(reader)
    if len(rows) != 20 or len({row["id"] for row in rows}) != 20:
        raise Refusal("candidate TSV must contain exactly 20 unique rows")
    for row in rows:
        if None in row or not IDENT.fullmatch(row["id"]) or any(word in row["id"] for word in ("smoke", "synthetic", "triangle", "demo", "benchmark")):
            raise Refusal("candidate id is invalid")
        if not TOKEN.fullmatch(row["package"]) or not TOKEN.fullmatch(row["app_id"]):
            raise Refusal(f"package identity is invalid for {row['id']}")
        executable = row["executable"].replace("\\", "/")
        parts = executable.split("/")
        if executable.startswith("/") or any(part in ("", ".", "..") for part in parts):
            raise Refusal(f"candidate executable path is unsafe for {row['id']}")
        if not all(HEX.fullmatch(row[key]) for key in ("executable_sha256", "blockmap_sha256")):
            raise Refusal(f"candidate hashes are invalid for {row['id']}")
        if not row["version"] or not row["static_graphics_imports"]:
            raise Refusal(f"candidate provenance is incomplete for {row['id']}")
    return rows


def copy_small(source: Path, destination: Path, expected: str) -> None:
    raw = read_regular(source, MAX_SMALL)
    if hashlib.sha256(raw).hexdigest() != expected:
        raise Refusal(f"small input hash mismatch: {source.name}")
    fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    try:
        view = memoryview(raw)
        while view:
            view = view[os.write(fd, view):]
    finally:
        os.close(fd)


def copy_inputs(manifest: Path, out: Path) -> str:
    entries = read_manifest(manifest)
    for key in ("image", "vars"):
        source, _ = entries[key]
        info = source.lstat()
        if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode) or info.st_mode & 0o222:
            raise Refusal(f"{key} must be an immutable regular file")
    validate_candidates(entries["candidates"][0])
    out.mkdir(mode=0o700)
    try:
        source, expected = entries["candidates"]
        copy_small(source, out / "sealed-candidates.tsv", expected)
        source, expected = entries["b4_manifest"]
        copy_small(source, out / "b4-input-manifest.tsv", expected)
    except Exception:
        for name in ("sealed-candidates.tsv", "b4-input-manifest.tsv"):
            (out / name).unlink(missing_ok=True)
        out.rmdir()
        raise
    return entries["candidates"][1]


def write_manifest(image: Path, vars_file: Path, candidates: Path, b4_manifest: Path, out: Path) -> str:
    for source, label in ((image, "image"), (vars_file, "vars")):
        info = source.lstat()
        if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode) or info.st_mode & 0o222:
            raise Refusal(f"{label} must be an immutable regular file")
    if image.parent != vars_file.parent or image.name != "disk.raw" or vars_file.name != "vars.fd":
        raise Refusal("image and vars do not form one prepared source")
    identity = image.parent.name
    if len(identity) != 129 or identity[64] != "-" or not HEX.fullmatch(identity[:64]) or not HEX.fullmatch(identity[65:]):
        raise Refusal("prepared source directory does not seal image/vars hashes")
    validate_candidates(candidates)
    entries = (("image", image, identity[:64]), ("vars", vars_file, identity[65:]),
               ("candidates", candidates, digest(candidates)), ("b4_manifest", b4_manifest, digest(b4_manifest)))
    if out.exists() or out.is_symlink():
        raise Refusal("manifest output already exists")
    with out.open("x", encoding="utf-8") as output:
        for key, source, expected in entries:
            output.write(f"{key}\t{source.absolute()}\t{expected}\n")
    os.chmod(out, 0o600)
    return digest(out)


def verify_inputs(manifest: Path, directory: Path, large: bool) -> str:
    entries = read_manifest(manifest)
    copied_candidates = directory / "sealed-candidates.tsv"
    copied_b4 = directory / "b4-input-manifest.tsv"
    validate_candidates(copied_candidates)
    for copied, key in ((copied_candidates, "candidates"), (copied_b4, "b4_manifest")):
        if digest(copied) != entries[key][1]:
            raise Refusal(f"sealed {key} changed")
    if large:
        for key in ("image", "vars"):
            if digest(entries[key][0]) != entries[key][1]:
                raise Refusal(f"sealed {key} changed or mismatched")
    return entries["candidates"][1]


def self_test() -> None:
    import tempfile

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        image, vars_file = root / "image.bin", root / "vars.bin"
        image.write_bytes(b"image"); vars_file.write_bytes(b"vars")
        os.chmod(image, 0o400); os.chmod(vars_file, 0o400)
        candidates = root / "candidates.tsv"
        lines = ["\t".join(COLUMNS)]
        for number in range(20):
            lines.append(f"real-app-{number:02d}\tPublisher.App_{number}_arm64__pub\tApp\tApp.exe\t{'a'*64}\t{'b'*64}\t1.0\tnone-static")
        candidates.write_text("\n".join(lines) + "\n")
        b4 = root / "b4.tsv"; b4.write_text("package\n")
        manifest = root / "manifest.tsv"
        with manifest.open("w") as output:
            for key, path in (("image", image), ("vars", vars_file), ("candidates", candidates), ("b4_manifest", b4)):
                output.write(f"{key}\t{path}\t{digest(path)}\n")
        sealed = root / "sealed"
        copy_inputs(manifest, sealed); verify_inputs(manifest, sealed, True)
        saved = candidates.read_text(); candidates.write_text(saved + lines[-1] + "\n")
        try: copy_inputs(manifest, root / "bad")
        except Refusal: pass
        else: raise AssertionError("post-manifest candidate mutation survived")
        candidates.write_text(saved)
        os.chmod(sealed / "sealed-candidates.tsv", 0o600)
        (sealed / "sealed-candidates.tsv").write_text(saved.replace("real-app-00", "real-app-01", 1))
        try: verify_inputs(manifest, sealed, False)
        except Refusal: pass
        else: raise AssertionError("sealed candidate mutation survived")
    print("PASS: compatibility observation inputs seal exactly 20 real AppX rows")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("write-manifest", "copy", "verify", "self-test"))
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--dir", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--vars", type=Path)
    parser.add_argument("--candidates", type=Path)
    parser.add_argument("--b4-manifest", type=Path)
    parser.add_argument("--verify-large", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "self-test": self_test()
        elif args.command == "write-manifest": print(write_manifest(args.image, args.vars, args.candidates, args.b4_manifest, args.out))
        elif args.command == "copy": print(copy_inputs(args.manifest, args.out))
        else: print(verify_inputs(args.manifest, args.dir, args.verify_large))
    except (OSError, UnicodeError, Refusal, ValueError) as error:
        print(f"REFUSED: {error}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
