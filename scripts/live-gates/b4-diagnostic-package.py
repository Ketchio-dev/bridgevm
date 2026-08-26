#!/usr/bin/env python3
"""Seal and stage the fixed B4 diagnostic driver package inventory."""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import stat
import tempfile
from pathlib import Path

FILES = (
    "BridgeVM-viogpu3d-Test.cer",
    "bridgevm-package-provenance.env",
    "viogpu3d.cat",
    "viogpu3d.inf",
    "viogpu3d.sys",
    "viogpu_d3d10.dll",
    "virtio_icd.arm64.json",
    "vulkan_virtio.dll",
)
MAX_FILE_BYTES = 64 * 1024 * 1024
CHUNK_BYTES = 1 * 1024 * 1024
EXPECTED_DRIVER_VERSION = b"120.50.0.0"
EXPECTED_PROVENANCE = b"VIOGPU3D_SOURCE_REF=d780b2b7f76301ef50282be973e95dbe6bba783f + mesa@cb531c440ff34a9c6334859dda0848132be49ec3 + builder@2f74d3332e50a71cf64bc25ee428fc0803334f81:submit-trace+resident-kmd"
REQUIRED_UMD_MARKERS = (
    b"BV-VIRGL-ALLOC-LIST-GROW-FAIL",
    b"BV-VIRGL-SUBMIT stage=",
)


class Refusal(ValueError):
    pass

def digest_file(path: Path) -> tuple[str, int]:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise Refusal(f"cannot open regular package file {path.name}: {error}") from error
    digest = hashlib.sha256()
    size = 0
    try:
        status = os.fstat(descriptor)
        if not stat.S_ISREG(status.st_mode):
            raise Refusal(f"package entry is not regular: {path.name}")
        while block := os.read(descriptor, 1024 * 1024):
            size += len(block)
            if size > MAX_FILE_BYTES:
                raise Refusal(f"package entry exceeds {MAX_FILE_BYTES} bytes: {path.name}")
            digest.update(block)
    finally:
        os.close(descriptor)
    return digest.hexdigest(), size

def bounded_bytes(path: Path, limit: int = MAX_FILE_BYTES) -> bytes:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise Refusal(f"package entry is not regular: {path.name}")
        data = bytearray()
        while block := os.read(descriptor, 1024 * 1024):
            data.extend(block)
            if len(data) > limit:
                raise Refusal(f"package entry exceeds {limit} bytes: {path.name}")
        return bytes(data)
    finally:
        os.close(descriptor)

def contains_all(path: Path, markers: tuple[bytes, ...]) -> bool:
    data = bounded_bytes(path)
    return all(marker in data for marker in markers)

def exact_directory(directory: Path) -> dict[str, tuple[str, int]]:
    try:
        status = directory.lstat()
    except OSError as error:
        raise Refusal(f"cannot inspect package directory: {error}") from error
    if not stat.S_ISDIR(status.st_mode) or directory.is_symlink():
        raise Refusal("package directory must be a real directory")
    try:
        entries = sorted(entry.name for entry in directory.iterdir())
    except OSError as error:
        raise Refusal(f"cannot read package directory: {error}") from error
    if entries != sorted(FILES):
        raise Refusal("package directory does not contain the exact fixed inventory")
    inf = bounded_bytes(directory / "viogpu3d.inf").replace(b" ", b"")
    if b"DriverVer=" not in inf or EXPECTED_DRIVER_VERSION not in inf:
        raise Refusal("diagnostic package does not carry the required 120.50 driver version")
    provenance = bounded_bytes(directory / "bridgevm-package-provenance.env").splitlines()
    if [line for line in provenance if line.startswith(b"VIOGPU3D_SOURCE_REF=")] != [EXPECTED_PROVENANCE]:
        raise Refusal("diagnostic package provenance does not pin the required sources")
    if not contains_all(directory / "viogpu_d3d10.dll", REQUIRED_UMD_MARKERS):
        raise Refusal("diagnostic UMD does not contain the required trace markers")
    return {name: digest_file(directory / name) for name in FILES}

def write_manifest(directory: Path, output: Path) -> str:
    inventory = exact_directory(directory)
    if output.exists() or output.is_symlink():
        raise Refusal("manifest output already exists")
    lines = [f"{name}\t{(directory.absolute() / name)}\t{inventory[name][0]}\n" for name in FILES]
    with output.open("x", encoding="utf-8") as manifest:
        manifest.write("".join(lines))
    os.chmod(output, 0o600)
    return tree_hash({name: inventory[name][0] for name in FILES})

def read_manifest(path: Path) -> dict[str, tuple[Path, str]]:
    try:
        data = bounded_bytes(path, 65536)
    except (OSError, Refusal) as error:
        raise Refusal(f"cannot read input manifest: {error}") from error
    if not data.endswith(b"\n") or b"\r" in data or b"\0" in data:
        raise Refusal("input manifest framing is invalid")
    try:
        lines = data.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        raise Refusal("input manifest is not UTF-8") from error
    if len(lines) != len(FILES):
        raise Refusal("input manifest must name exactly eight files")
    result: dict[str, tuple[Path, str]] = {}
    for expected_name, line in zip(FILES, lines, strict=True):
        fields = line.split("\t")
        if len(fields) != 3 or fields[0] != expected_name:
            raise Refusal("input manifest names are missing, unknown, duplicate, or unsorted")
        source = Path(fields[1])
        digest = fields[2]
        if not source.is_absolute() or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise Refusal(f"invalid path or SHA-256 for {expected_name}")
        result[expected_name] = (source, digest)
    return result


def tree_hash(inventory: dict[str, str]) -> str:
    digest = hashlib.sha256()
    for name in FILES:
        digest.update(name.encode("ascii") + b"\0" + inventory[name].encode("ascii") + b"\n")
    return digest.hexdigest()


def copy_verified_source(source: Path, destination: Path, expected: str) -> None:
    input_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    output_flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0)
    try:
        input_descriptor = os.open(source, input_flags)
    except OSError as error:
        raise Refusal(f"cannot open source for sealed copy: {source.name}") from error
    output_descriptor = -1
    digest = hashlib.sha256()
    size = 0
    try:
        if not stat.S_ISREG(os.fstat(input_descriptor).st_mode):
            raise Refusal(f"source is not regular: {source.name}")
        output_descriptor = os.open(destination, output_flags, 0o400)
        while block := os.read(input_descriptor, 1024 * 1024):
            size += len(block)
            if size > MAX_FILE_BYTES:
                raise Refusal(f"source exceeds {MAX_FILE_BYTES} bytes: {source.name}")
            digest.update(block)
            view = memoryview(block)
            while view:
                written = os.write(output_descriptor, view)
                view = view[written:]
    finally:
        if output_descriptor >= 0:
            os.close(output_descriptor)
        os.close(input_descriptor)
    if digest.hexdigest() != expected:
        destination.unlink(missing_ok=True)
        raise Refusal(f"source changed or hash mismatched during copy: {source.name}")


def copy_manifest(manifest: Path, output: Path) -> str:
    entries = read_manifest(manifest)
    if output.exists() or output.is_symlink():
        raise Refusal("sealed package output already exists")
    output.mkdir(mode=0o700)
    try:
        for name in FILES:
            source, expected = entries[name]
            destination = output / name
            copy_verified_source(source, destination, expected)
            copied, _ = digest_file(destination)
            if copied != expected:
                raise Refusal(f"sealed copy hash mismatch: {name}")
        return verify_manifest(manifest, output)
    except Exception:
        shutil.rmtree(output, ignore_errors=True)
        raise


def verify_manifest(manifest: Path, directory: Path) -> str:
    entries = read_manifest(manifest)
    inventory = exact_directory(directory)
    digests: dict[str, str] = {}
    for name in FILES:
        actual = inventory[name][0]
        if actual != entries[name][1]:
            raise Refusal(f"sealed package hash mismatch: {name}")
        digests[name] = actual
    return tree_hash(digests)


def stage_share(manifest: Path, directory: Path, share: Path, chunk_bytes: int = CHUNK_BYTES) -> str:
    package_hash = verify_manifest(manifest, directory)
    if chunk_bytes < 1 or chunk_bytes > CHUNK_BYTES:
        raise Refusal("invalid chunk bound")
    reserved = [entry.name for entry in share.iterdir() if entry.name.startswith("b4pkg-")]
    if reserved or (share / "b4-package-manifest.tsv").exists():
        raise Refusal("share already contains B4 package staging files")
    lines = ["format\tbridgevm-b4-package-v1\n", f"package_sha256\t{package_hash}\n"]
    for file_index, name in enumerate(FILES):
        expected, size = digest_file(directory / name)
        chunks: list[tuple[str, int, str]] = []
        input_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
        input_descriptor = os.open(directory / name, input_flags)
        staged_digest = hashlib.sha256()
        staged_size = 0
        try:
            if not stat.S_ISREG(os.fstat(input_descriptor).st_mode):
                raise Refusal(f"staged source is not regular: {name}")
            chunk_index = 0
            while block := os.read(input_descriptor, chunk_bytes):
                staged_size += len(block)
                if staged_size > MAX_FILE_BYTES:
                    raise Refusal(f"staged source exceeds {MAX_FILE_BYTES} bytes: {name}")
                staged_digest.update(block)
                chunk_name = f"b4pkg-{file_index:02d}-{chunk_index:03d}.bin"
                chunk_path = share / chunk_name
                with chunk_path.open("xb") as output_file:
                    output_file.write(block)
                os.chmod(chunk_path, 0o400)
                chunks.append((chunk_name, len(block), hashlib.sha256(block).hexdigest()))
                chunk_index += 1
        finally:
            os.close(input_descriptor)
        if staged_size != size or staged_digest.hexdigest() != expected:
            raise Refusal(f"sealed source changed during share staging: {name}")
        if not chunks:
            raise Refusal(f"empty package file is not allowed: {name}")
        lines.append(f"file\t{file_index}\t{name}\t{size}\t{expected}\t{len(chunks)}\n")
        for chunk_index, (chunk_name, chunk_size, chunk_hash) in enumerate(chunks):
            lines.append(
                f"chunk\t{file_index}\t{chunk_index}\t{chunk_name}\t{chunk_size}\t{chunk_hash}\n"
            )
    output = share / "b4-package-manifest.tsv"
    with output.open("x", encoding="ascii") as share_manifest:
        share_manifest.write("".join(lines))
    os.chmod(output, 0o400)
    return package_hash


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        source = root / "source"
        source.mkdir()
        for index, name in enumerate(FILES):
            payload = (name.encode("ascii") + b"\n") * (index + 1)
            if name == "viogpu3d.inf":
                payload += b"DriverVer=08/25/2026,120.50.0.0\n"
            if name == "bridgevm-package-provenance.env":
                payload += EXPECTED_PROVENANCE + b"\n"
            if name == "viogpu_d3d10.dll":
                payload += b"\0".join(REQUIRED_UMD_MARKERS)
            (source / name).write_bytes(payload)
        manifest = root / "input.tsv"
        expected = write_manifest(source, manifest)
        sealed = root / "sealed"
        if copy_manifest(manifest, sealed) != expected or verify_manifest(manifest, sealed) != expected:
            raise AssertionError("copy/verify hash mismatch")
        share = root / "share"
        share.mkdir()
        if stage_share(manifest, sealed, share, chunk_bytes=17) != expected:
            raise AssertionError("share staging hash mismatch")
        chunks = sorted(share.glob("b4pkg-*.bin"))
        if len(chunks) <= len(FILES) or any(path.stat().st_size > 17 for path in chunks):
            raise AssertionError("chunk bound was not enforced")
        os.chmod(sealed / FILES[0], 0o600)
        (sealed / FILES[0]).write_bytes(b"mutation")
        try:
            verify_manifest(manifest, sealed)
        except Refusal:
            pass
        else:
            raise AssertionError("sealed mutation was accepted")
        (sealed / FILES[0]).unlink()
        (sealed / FILES[0]).symlink_to(source / FILES[0])
        try:
            verify_manifest(manifest, sealed)
        except Refusal:
            pass
        else:
            raise AssertionError("sealed symlink was accepted")
        for name, bad in (
            ("viogpu_d3d10.dll", b"uninstrumented"),
            ("viogpu3d.inf", b"DriverVer=07/25/2026,120.45.0.0\n"),
            ("bridgevm-package-provenance.env", b"VIOGPU3D_SOURCE_REF=movable\n"),
        ):
            (source / name).write_bytes(bad)
            try:
                exact_directory(source)
            except Refusal:
                pass
            else:
                raise AssertionError(f"ineligible diagnostic package was accepted: {name}")
    print("PASS: B4 diagnostic package seal and chunk mutations")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("write-manifest", "copy", "verify", "stage-share", "self-test"))
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--dir", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--share", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "self-test":
            self_test()
        elif args.command == "write-manifest":
            if args.dir is None or args.out is None:
                raise Refusal("write-manifest needs --dir and --out")
            print(write_manifest(args.dir, args.out))
        elif args.command == "copy":
            if args.manifest is None or args.out is None:
                raise Refusal("copy needs --manifest and --out")
            print(copy_manifest(args.manifest, args.out))
        elif args.command == "verify":
            if args.manifest is None or args.dir is None:
                raise Refusal("verify needs --manifest and --dir")
            print(verify_manifest(args.manifest, args.dir))
        else:
            if args.manifest is None or args.dir is None or args.share is None:
                raise Refusal("stage-share needs --manifest, --dir, and --share")
            print(stage_share(args.manifest, args.dir, args.share))
    except (OSError, Refusal) as error:
        print(f"REFUSED: {error}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
