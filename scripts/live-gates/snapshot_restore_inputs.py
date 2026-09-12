"""Validate and APFS-clone the exact private inputs to the restore-boot gate."""
import hashlib
import re
import stat
import subprocess
from pathlib import Path

FILES = {"image", "vars", "binary"}
METADATA = {"binary_source_commit", "binary_profile", "binary_features", "rust_toolchain"}


def parse_manifest(data, commit):
    if len(data) > 65536 or b"\0" in data:
        raise ValueError("invalid manifest size or bytes")
    rows = {}
    for line in data.decode("utf-8").splitlines():
        fields = line.split("\t")
        key = fields[0]
        if key in rows or key not in FILES | METADATA:
            raise ValueError("duplicate or unknown manifest key")
        if len(fields) != (3 if key in FILES else 2):
            raise ValueError("invalid manifest field count")
        if key in FILES:
            if not Path(fields[1]).is_absolute() or not re.fullmatch(r"[0-9a-f]{64}", fields[2]):
                raise ValueError("invalid artifact path or digest")
        rows[key] = fields[1:]
    if rows.keys() != FILES | METADATA:
        raise ValueError("missing manifest keys")
    expected = {"binary_source_commit": commit, "binary_profile": "release",
                "binary_features": "venus", "rust_toolchain": "1.97.0"}
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or any(rows[k] != [v] for k, v in expected.items()):
        raise ValueError("binary source/build identity mismatch")
    return rows


def regular(path):
    if not stat.S_ISREG(Path(path).lstat().st_mode):
        raise ValueError("artifact must be a non-symlink regular file")


def digest(path):
    regular(path)
    output = subprocess.check_output(["openssl", "dgst", "-sha256", "-r", str(path)], text=True)
    value = output.split()[0]
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ValueError("invalid hash result")
    return value


def clone(source, destination):
    subprocess.run(["cp", "-c", str(source), str(destination)], check=True)


def prepare(manifest, binary, commit, directory, clone_file=clone):
    regular(manifest)
    with open(manifest, "rb") as stream:
        data = stream.read(65537)
    rows = parse_manifest(data, commit)
    if digest(binary) != rows["binary"][1]:
        raise ValueError("sealed binary digest mismatch")
    directory = Path(directory)
    directory.mkdir(mode=0o700)
    for key, filename in (("image", "disk.raw"), ("vars", "vars.fd")):
        source = Path(rows[key][0])
        regular(source)
        destination = directory / filename
        clone_file(source, destination)
        if digest(destination) != rows[key][1]:
            raise ValueError("cloned input digest mismatch")
    if digest(binary) != rows["binary"][1]:
        raise ValueError("sealed binary changed during staging")
    return {"image_sha256": rows["image"][1], "vars_sha256": rows["vars"][1],
            "binary_hash": rows["binary"][1], "binary_source_commit": commit,
            "binary_profile": "release", "binary_features": "venus", "rust_toolchain": "1.97.0",
            "input_manifest_sha256": hashlib.sha256(data).hexdigest()}
