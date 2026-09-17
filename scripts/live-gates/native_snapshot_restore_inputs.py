"""Authenticate and clone inputs for the native app snapshot restore tier."""
from __future__ import annotations

import hashlib
import argparse
import os
import re
import stat
import subprocess
from pathlib import Path

FILES = {"app_bundle", "app_cli", "app_executable", "snapshot_helper", "image", "vars", "binary"}
METADATA = {"source_commit", "app_profile", "binary_profile", "binary_features", "rust_toolchain"}
RELATIONS = {
    "app_cli": "Contents/Resources/target/release/bridgevm",
    "app_executable": "Contents/MacOS/BridgeVMControl",
    "snapshot_helper": "Contents/Resources/target/release/examples/snapshot_pair_cli",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def regular(path: Path) -> None:
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError("artifact must be a non-symlink regular file")


def digest(path: Path) -> str:
    regular(path)
    output = subprocess.check_output(["openssl", "dgst", "-sha256", "-r", str(path)], text=True)
    value = output.split()[0]
    if not SHA256.fullmatch(value):
        raise ValueError("invalid artifact hash")
    return value


def tree_hash(root: Path) -> str:
    if not root.is_dir() or root.is_symlink():
        raise ValueError("app bundle must be a non-symlink directory")
    records: list[bytes] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix()
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            target = os.readlink(path)
            if os.path.isabs(target) or root.resolve() not in path.resolve().parents:
                raise ValueError("app bundle symlink escapes its root")
            records.append(f"L\t{relative}\t{target}\n".encode())
        elif stat.S_ISREG(mode):
            executable = "1" if mode & 0o111 else "0"
            records.append(f"F\t{relative}\t{executable}\t{digest(path)}\n".encode())
        elif stat.S_ISDIR(mode):
            records.append(f"D\t{relative}\n".encode())
        else:
            raise ValueError("app bundle contains an unsupported entry")
    return hashlib.sha256(b"".join(records)).hexdigest()


def parse_manifest(data: bytes, commit: str) -> dict[str, list[str]]:
    if len(data) == 0 or len(data) > 65_536 or b"\0" in data:
        raise ValueError("invalid manifest size or bytes")
    rows: dict[str, list[str]] = {}
    for line in data.decode("utf-8").splitlines():
        fields = line.split("\t")
        key = fields[0]
        if key in rows or key not in FILES | METADATA:
            raise ValueError("duplicate or unknown manifest key")
        if len(fields) != (3 if key in FILES else 2):
            raise ValueError("invalid manifest field count")
        if key in FILES:
            if not Path(fields[1]).is_absolute() or not SHA256.fullmatch(fields[2]):
                raise ValueError("invalid artifact path or digest")
            if os.path.normpath(fields[1]) != fields[1]:
                raise ValueError("artifact path is not normalized")
        rows[key] = fields[1:]
    if set(rows) != FILES | METADATA:
        raise ValueError("missing manifest keys")
    expected = {
        "source_commit": commit,
        "app_profile": "release",
        "binary_profile": "release",
        "binary_features": "venus",
        "rust_toolchain": "1.97.0",
    }
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or any(rows[key] != [value] for key, value in expected.items()):
        raise ValueError("source or build identity mismatch")
    app = Path(rows["app_bundle"][0])
    for key, relative in RELATIONS.items():
        if Path(rows[key][0]) != app / relative:
            raise ValueError(f"{key} is outside its fixed app location")
    return rows


def clone(source: Path, destination: Path) -> None:
    subprocess.run(["cp", "-c", str(source), str(destination)], check=True)


def authenticate(rows: dict[str, list[str]], sealed_binary: Path) -> None:
    app = Path(rows["app_bundle"][0])
    if tree_hash(app) != rows["app_bundle"][1]:
        raise ValueError("app bundle hash mismatch")
    for key in FILES - {"app_bundle", "binary"}:
        if digest(Path(rows[key][0])) != rows[key][1]:
            raise ValueError(f"{key} hash mismatch")
    if digest(sealed_binary) != rows["binary"][1]:
        raise ValueError("sealed binary hash mismatch")


def prepare(manifest: Path, sealed_binary: Path, commit: str, directory: Path, clone_file=clone) -> tuple[dict, dict]:
    regular(manifest)
    data = manifest.read_bytes()
    rows = parse_manifest(data, commit)
    authenticate(rows, sealed_binary)
    directory.mkdir(mode=0o700)
    for key, filename in (("image", "disk.raw"), ("vars", "vars.fd")):
        destination = directory / filename
        clone_file(Path(rows[key][0]), destination)
        if digest(destination) != rows[key][1]:
            raise ValueError("cloned input hash mismatch")
    authenticate(rows, sealed_binary)
    public = {
        "input_manifest_sha256": hashlib.sha256(data).hexdigest(),
        "app_artifact_sha256": rows["app_bundle"][1],
        "app_cli_sha256": rows["app_cli"][1],
        "app_executable_sha256": rows["app_executable"][1],
        "snapshot_helper_sha256": rows["snapshot_helper"][1],
        "image_sha256": rows["image"][1],
        "vars_sha256": rows["vars"][1],
        "binary_hash": rows["binary"][1],
        "binary_source_commit": commit,
        "binary_profile": "release",
        "binary_features": "venus",
        "rust_toolchain": "1.97.0",
    }
    private = {"rows": rows, "app_cli": rows["app_cli"][0]}
    return public, private


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("commit")
    args = parser.parse_args()
    regular(args.manifest)
    rows = parse_manifest(args.manifest.read_bytes(), args.commit)
    authenticate(rows, Path(rows["binary"][0]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
