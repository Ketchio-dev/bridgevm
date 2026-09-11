"""Exact private inputs and independent clones for B6 cell observations."""
import contextlib
import hashlib
import json
import os
import pathlib
import re
import stat
import subprocess

KEYS = frozenset(("image", "vars", "binary", "virglrenderer", "moltenvk",
                  "viogpu_dir", "presentmon", "config", "render_server"))
RESOLUTIONS = frozenset(((1280, 720), (1600, 900), (1920, 1080)))


@contextlib.contextmanager
def regular_file(path):
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    if pathlib.Path(path).is_symlink():
        raise ValueError("symlink input refused")
    with os.fdopen(os.open(path, flags), "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("non-regular input refused")
        yield stream


def small_bytes(path, limit):
    with regular_file(path) as stream:
        if os.fstat(stream.fileno()).st_size > limit:
            raise ValueError("metadata input too large")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("metadata input grew beyond limit")
    return raw


def file_hash(path):
    digest = hashlib.sha256()
    with regular_file(path) as stream:
        before = os.fstat(stream.fileno())
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
        after = os.fstat(stream.fileno())
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns)
    if identity(before) != identity(after) or identity(after) != identity(os.stat(path, follow_symlinks=False)):
        raise ValueError("input changed while hashing")
    return digest.hexdigest()


def tree_hash(root):
    root = pathlib.Path(root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError("driver store must be a real directory")
    entries = []
    for directory, folders, files in os.walk(root, followlinks=False):
        for name in folders + files:
            path = pathlib.Path(directory) / name
            if path.is_symlink() or any(char in name for char in "\r\n\t\\"):
                raise ValueError("unsafe driver-store entry")
        for name in files:
            path = pathlib.Path(directory) / name
            relative = "./" + path.relative_to(root).as_posix()
            entries.append((relative, file_hash(path)))
    if not entries:
        raise ValueError("empty driver store")
    encoded = "".join(digest + "  " + name + "\n" for name, digest in sorted(entries))
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def verify_inputs(records, sealed_binary):
    for key, (path, expected) in records.items():
        actual = tree_hash(path) if key == "viogpu_dir" else file_hash(path)
        if actual != expected:
            raise ValueError("input hash mismatch: " + key)
    for key in ("image", "vars"):
        if records[key][0].stat().st_mode & 0o222:
            raise ValueError("canonical media must be read-only: " + key)
    first, second = (records[key][0].stat() for key in ("image", "vars"))
    if (first.st_dev, first.st_ino) == (second.st_dev, second.st_ino):
        raise ValueError("image and vars alias the same inode")
    if file_hash(sealed_binary) != records["binary"][1]:
        raise ValueError("sealed binary differs from manifest")


def load_inputs(manifest, sealed_binary):
    raw = small_bytes(manifest, 32768)
    records = {}
    for line in raw.decode("utf-8").splitlines():
        fields = line.split("\t")
        if len(fields) != 3:
            raise ValueError("manifest requires three tab-separated fields")
        key, name, digest = fields
        if key not in KEYS or key in records or not pathlib.Path(name).is_absolute():
            raise ValueError("unknown, duplicate, or non-absolute manifest entry")
        if not re.fullmatch("[0-9a-f]{64}", digest):
            raise ValueError("manifest digest is not SHA256")
        records[key] = (pathlib.Path(name), digest)
    if records.keys() != KEYS:
        raise ValueError("manifest input set is incomplete")
    verify_inputs(records, sealed_binary)
    raw_config = small_bytes(records["config"][0], 4096)
    if hashlib.sha256(raw_config).hexdigest() != records["config"][1]:
        raise ValueError("configuration changed after authentication")
    config = json.loads(raw_config)
    if not isinstance(config, dict) or set(config) != {"width", "height", "logpixels"}:
        raise ValueError("unexpected cell configuration fields")
    if any(type(value) is not int for value in config.values()):
        raise ValueError("cell dimensions and DPI must be integers")
    if (config["width"], config["height"]) not in RESOLUTIONS or config["logpixels"] not in (96, 120, 144):
        raise ValueError("cell is outside the original B6 matrix")
    return records, config


def clone_pair(records, work, copy=None):
    copy = subprocess.run if copy is None else copy
    work = pathlib.Path(work)
    work.mkdir(mode=0o700, exist_ok=False)
    device = work.stat().st_dev
    sources = [records[key][0] for key in ("image", "vars")]
    if any(source.stat().st_dev != device for source in sources):
        raise ValueError("stage external media internally before cloning")
    targets = [work / "disk.raw", work / "vars.fd"]
    copy(["cp", "-c", str(sources[0]), str(targets[0])], check=True)
    copy(["cp", str(sources[1]), str(targets[1])], check=True)
    for source, target in zip(sources, targets):
        before, after = source.stat(), target.stat()
        if target.is_symlink() or (before.st_dev, before.st_ino) == (after.st_dev, after.st_ino):
            raise ValueError("clone aliases canonical media")
    for key, target in zip(("image", "vars"), targets):
        if file_hash(target) != records[key][1]:
            raise ValueError("clone differs from sealed media")
        target.chmod(0o600)
    return tuple(targets)
