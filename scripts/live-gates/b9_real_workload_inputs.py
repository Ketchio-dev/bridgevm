"""Sealed private inputs for one genuine B9 VLC/AV1 diagnostic pilot."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys

from b6_cell_inputs import tree_hash

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from b9_workload_diagnostic import load_declaration

KEYS = frozenset(("image", "vars", "binary", "firmware", "virglrenderer",
                  "moltenvk", "viogpu_dir", "render_server", "presentmon",
                  "vlc_zip", "media", "guest_script", "profile"))
MANIFEST_LIMIT = 32_768
PROFILE_LIMIT = 4_096
VARS_SIZE = 64 * 1024 * 1024
CHUNK_BYTES = 7_500_000
SHA = re.compile(r"[0-9a-f]{64}\Z")
SOURCE = re.compile(r"[0-9a-f]{40}\Z")


def parent_chain(path: Path) -> None:
    """Refuse aliases in every existing ancestor, not only the leaf."""
    if not path.is_absolute() or str(path) != str(Path(os.path.normpath(path))):
        raise ValueError("input path must be absolute and normalized")
    for parent in (path, *path.parents):
        if stat.S_ISLNK(os.lstat(parent).st_mode):
            raise ValueError("symlink in input path")


def stable_file(path: Path, *, maximum: int | None = None) -> tuple[int, str]:
    parent_chain(path)
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size <= 0:
            raise ValueError("input must be a nonempty regular file")
        if maximum is not None and before.st_size > maximum:
            raise ValueError("input exceeds size bound")
        digest = hashlib.sha256()
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            digest.update(block)
        after = os.fstat(stream.fileno())
    identity = lambda x: (x.st_dev, x.st_ino, x.st_size, x.st_mtime_ns, x.st_ctime_ns)
    if identity(before) != identity(after) or identity(after) != identity(os.lstat(path)):
        raise ValueError("input changed while hashing")
    return before.st_size, digest.hexdigest()


def bounded_bytes(path: Path, limit: int) -> bytes:
    size, _ = stable_file(path, maximum=limit)
    with path.open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) != size:
        raise ValueError("metadata changed while reading")
    return raw


def profile_at(path: Path, source_commit: str) -> dict:
    raw = bounded_bytes(path, PROFILE_LIMIT)
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("malformed B9 profile") from error
    declaration, declaration_hash = load_declaration()
    expected = {"schema": "bridgevm.b9-real-pilot-profile.v1",
                "source_commit": source_commit,
                "candidate_id": declaration["id"],
                "candidate_declaration_sha256": declaration_hash,
                "purpose": "diagnostic-only",
                "three_d": True}
    if value != expected or any(type(value[key]) is not type(item)
                                for key, item in expected.items()):
        raise ValueError("B9 profile or exact source differs")
    return value


def _unique(pairs: list[tuple[str, object]]) -> dict:
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate JSON key")
        value[key] = item
    return value


def load_inputs(manifest: Path, source_commit: str, sealed_binary: Path | None = None) -> dict:
    if not SOURCE.fullmatch(source_commit):
        raise ValueError("exact source commit required")
    observed_commit = subprocess.check_output(
        ["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).strip()
    if observed_commit != source_commit:
        raise ValueError("B9 input source differs from exact checkout")
    raw = bounded_bytes(manifest, MANIFEST_LIMIT)
    try:
        lines = raw.decode("utf-8").splitlines()
    except UnicodeError as error:
        raise ValueError("manifest must be UTF-8") from error
    records: dict[str, tuple[Path, str]] = {}
    seen_inodes: set[tuple[int, int]] = set()
    for line in lines:
        fields = line.split("\t")
        if len(fields) != 3:
            raise ValueError("B9 manifest requires three TSV fields")
        key, name, digest = fields
        if key not in KEYS or key in records or not SHA.fullmatch(digest):
            raise ValueError("unknown, duplicate or malformed manifest input")
        path = Path(name)
        parent_chain(path)
        metadata = os.lstat(path)
        if key == "viogpu_dir":
            if not stat.S_ISDIR(metadata.st_mode):
                raise ValueError("driver store must be directory")
            observed = tree_hash(path)
        else:
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError("manifest input must be regular")
            observed = stable_file(path)[1]
        inode = (metadata.st_dev, metadata.st_ino)
        if inode in seen_inodes:
            raise ValueError("manifest aliases input inode")
        seen_inodes.add(inode)
        if observed != digest:
            raise ValueError("manifest hash mismatch: " + key)
        records[key] = (path, digest)
    if records.keys() != KEYS:
        raise ValueError("incomplete B9 manifest")
    declaration, _ = load_declaration()
    pins = {"vlc_zip": declaration["application"],
            "media": declaration["media"],
            "presentmon": declaration["collector"]}
    for key, pin in pins.items():
        path, digest = records[key]
        if path.name != pin["asset"] or path.stat().st_size != pin["bytes"] or digest != pin["sha256"]:
            raise ValueError("declared B9 asset differs: " + key)
    image, variables = (records[key][0] for key in ("image", "vars"))
    if image.stat().st_size % 512 or variables.stat().st_size != VARS_SIZE:
        raise ValueError("disk alignment or 64 MiB vars size differs")
    if any(path.stat().st_mode & 0o222 for path in (image, variables)):
        raise ValueError("canonical disk and vars must be immutable")
    pinned = {"firmware": REPO / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd",
              "guest_script": REPO / "scripts/win-assets/bv-b9-vlc-playback.ps1"}
    for key, checkout_path in pinned.items():
        if stable_file(checkout_path)[1] != records[key][1]:
            raise ValueError("B9 source asset differs from exact checkout: " + key)
    if records["guest_script"][0].stat().st_size >= 8_000_000:
        raise ValueError("guest script exceeds share file limit")
    profile_at(records["profile"][0], source_commit)
    if sealed_binary is not None and stable_file(sealed_binary)[1] != records["binary"][1]:
        raise ValueError("queue-sealed binary differs from manifest")
    return records


def verify_inputs(records: dict, sealed_binary: Path | None = None) -> None:
    for key, (path, expected) in records.items():
        actual = tree_hash(path) if key == "viogpu_dir" else stable_file(path)[1]
        if actual != expected:
            raise ValueError("B9 input changed after manifest validation: " + key)
    if sealed_binary is not None and stable_file(sealed_binary)[1] != records["binary"][1]:
        raise ValueError("queue-sealed B9 binary changed")


def driver_umd_hash(records: dict) -> str:
    root = records["viogpu_dir"][0]
    matches = [path for path in root.rglob("*") if path.name.lower() == "viogpu_d3d10.dll"]
    if len(matches) != 1:
        raise ValueError("sealed 3D driver input needs one native D3D11 UMD")
    return stable_file(matches[0])[1]


def stage_external_pair(records: dict, cache: Path) -> dict:
    """Make a private internal source before same-volume APFS lane clones.

    Cross-volume copies are never described as clones. The staged copy is
    hashed before it can become the source of a lane clone.
    """
    cache.mkdir(mode=0o700, parents=False, exist_ok=False)
    staged = dict(records)
    for key in ("image", "vars"):
        source, expected = records[key]
        if source.stat().st_dev == cache.stat().st_dev:
            continue
        target = cache / ("canonical-staged.raw" if key == "image" else "canonical-staged-vars.fd")
        if shutil.disk_usage(cache).free < source.stat().st_size + VARS_SIZE:
            raise ValueError("insufficient free space for external source staging")
        subprocess.run(["/bin/cp", "-c", str(source), str(target)], check=True, timeout=1800)
        if target.stat().st_dev != cache.stat().st_dev or stable_file(target)[1] != expected:
            raise ValueError("external source staging failed integrity check")
        target.chmod(0o400)
        staged[key] = (target, expected)
    verify_inputs(records)
    return staged


def clone_pair(records: dict, work: Path) -> tuple[Path, Path]:
    """Only same-device APFS clone copies are allowed for writable lane media."""
    work.mkdir(mode=0o700, parents=False, exist_ok=False)
    sources = [records[key][0] for key in ("image", "vars")]
    targets = [work / "disk.raw", work / "vars.fd"]
    if any(source.stat().st_dev != work.stat().st_dev for source in sources):
        raise ValueError("stage external canonical media in an internal sparse cache first")
    pair_bytes = sum(source.stat().st_size for source in sources)
    required = pair_bytes + max(VARS_SIZE, pair_bytes // 10)
    if shutil.disk_usage(work).free < required:
        raise ValueError("insufficient conservative APFS clone free space")
    for source, target, key in zip(sources, targets, ("image", "vars")):
        subprocess.run(["/bin/cp", "-c", str(source), str(target)], check=True, timeout=300)
        if target.is_symlink() or (source.stat().st_dev, source.stat().st_ino) == (target.stat().st_dev, target.stat().st_ino):
            raise ValueError("APFS clone aliases canonical media")
        if stable_file(target)[1] != records[key][1]:
            raise ValueError("APFS clone hash mismatch")
        target.chmod(0o600)
    return tuple(targets)


def _exclusive_copy(source: Path, target: Path) -> tuple[int, str]:
    with source.open("rb") as incoming:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as outgoing:
            shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)
    return stable_file(target, maximum=CHUNK_BYTES)


def stage_share(records: dict, share: Path) -> dict:
    """Every synchronized file is below 8 MB; no whole VLC archive is shared."""
    share.mkdir(mode=0o700, parents=False, exist_ok=False)
    staged: dict[str, tuple[int, str]] = {}
    for key in ("media", "presentmon", "guest_script"):
        source, expected = records[key]
        if source.stat().st_size > CHUNK_BYTES:
            raise ValueError("B9 share asset exceeds per-file bound: " + key)
        target = share / source.name
        size, digest = _exclusive_copy(source, target)
        if digest != expected:
            raise ValueError("B9 share copy differs: " + key)
        staged[target.name] = (size, digest)
    source, archive_hash = records["vlc_zip"]
    lines = []
    archive = hashlib.sha256()
    with source.open("rb") as stream:
        for index in range(10):
            data = stream.read(CHUNK_BYTES)
            if not data or len(data) > CHUNK_BYTES:
                raise ValueError("VLC chunk count or size differs")
            name = f"b9-vlc-part-{index:02d}.bin"
            target = share / name
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "wb") as output:
                output.write(data)
            part_hash = hashlib.sha256(data).hexdigest()
            if stable_file(target, maximum=CHUNK_BYTES) != (len(data), part_hash):
                raise ValueError("staged VLC chunk changed")
            archive.update(data)
            staged[name] = (len(data), part_hash)
            lines.append(f"{name}\t{len(data)}\t{part_hash}\n")
        if stream.read(1) or archive.hexdigest() != archive_hash:
            raise ValueError("staged VLC archive differs")
    manifest = share / "b9-vlc-parts.tsv"
    fd = os.open(manifest, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write("".join(lines).encode("ascii"))
    staged[manifest.name] = stable_file(manifest, maximum=8_000_000)
    verify_inputs(records)
    return staged


def asset_seal(records: dict) -> str:
    return "".join(f"asset_{key}_sha256={records[key][1]}\n" for key in sorted(KEYS))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("validate", "seal"))
    parser.add_argument("manifest", type=Path)
    parser.add_argument("source_commit")
    parser.add_argument("staging", type=Path, nargs="?")
    args = parser.parse_args()
    try:
        records = load_inputs(args.manifest, args.source_commit)
        if args.mode == "seal":
            if args.staging is None or not args.staging.is_absolute() or not args.staging.is_dir():
                raise ValueError("queue staging directory required")
            target = args.staging / "input-asset-seal.env"
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "w", encoding="ascii") as output:
                output.write(asset_seal(records))
        elif args.staging is not None:
            raise ValueError("validate mode takes no staging directory")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("B9 real-workload inputs refused: " + str(error), file=sys.stderr)
        return 2
    print("B9 real-workload private inputs: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
