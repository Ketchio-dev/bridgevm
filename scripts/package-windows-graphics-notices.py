#!/usr/bin/env python3
"""Assemble and verify the flat notice set carried by Windows graphics artifacts."""

from __future__ import annotations

import argparse
import csv
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import NoReturn
import zipfile

import windows_graphics_notice_git as notice_git


MESA_PATCHES = (
    "virtio-win-mesa-submit-trace.patch",
    "virtio-win-mesa-unbound-clear.patch",
)
NOTICE_FILES = (
    "BridgeVM-MODIFICATIONS.txt",
    "Mesa-license.rst",
    "Mesa-patched-files-MIT.txt",
    "Mesa-upstream-licenses.zip",
    "virtio-win-BSD-3-Clause.txt",
    *MESA_PATCHES,
)
SUMS_FILE = "THIRD-PARTY-NOTICE-SHA256SUMS"
DRIVER_REVISION = "4c27e477e6560cea724d848b98149f03cb1f2083"
DRIVER_LICENSE_SHA256 = (
    "101c9fb1e823cdbd6c20ef232ba9bc3cf5b4c57cf62e0f5b0bcc8ddad85d7ea1"
)
MESA_OVERVIEW_SHA256 = (
    "0d1a0472ecc81830e75c20d59b0ea02841e3db21255e0ebad97ab682c54d6615"
)
MESA_LICENSE_ARCHIVE_SHA256 = (
    "a4f9d06a13b810a6e4054ff54f4a8bb9ab8edfee4839b2a7ee83db40aba0ecc0"
)


def fail(message: str) -> NoReturn:
    raise SystemExit(f"windows graphics notices: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_root() -> Path:
    script_dir = Path(__file__).resolve().parent
    for candidate in (script_dir.parent, script_dir):
        if (candidate / "THIRD-PARTY-PATCHES.tsv").is_file():
            return candidate
    fail("THIRD-PARTY-PATCHES.tsv is unavailable beside the packager")


def source_asset(root: Path, registered_path: str) -> Path:
    nested = root / registered_path
    flat = root / Path(registered_path).name
    path = nested if nested.is_file() else flat
    if not path.is_file() or path.is_symlink():
        fail(f"source asset is missing or is a symlink: {registered_path}")
    return path


def mesa_rows(root: Path) -> dict[str, dict[str, str]]:
    registry = root / "THIRD-PARTY-PATCHES.tsv"
    with registry.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    selected = {
        Path(row["path"]).name: row
        for row in rows
        if Path(row["path"]).name in MESA_PATCHES
    }
    if set(selected) != set(MESA_PATCHES):
        fail("patch registry does not contain the exact Mesa patch set")
    revisions = {row["upstream_revision"] for row in selected.values()}
    scopes = {row["distribution_scope"] for row in selected.values()}
    licenses = {row["license"] for row in selected.values()}
    if len(revisions) != 1 or scopes != {"graphics-lab-artifact"} or licenses != {"MIT"}:
        fail("Mesa registry rows disagree on revision, scope, or licence")
    for row in selected.values():
        patch = source_asset(root, row["path"])
        if sha256(patch) != row["patch_sha256"]:
            fail(f"registered patch digest is stale: {row['path']}")
    return selected


def local_notice_sources(root: Path) -> tuple[dict[str, dict[str, str]], Path, Path]:
    rows = mesa_rows(root)
    mesa_license = source_asset(root, next(iter(rows.values()))["license_text"])
    driver_license = source_asset(root, "docs/licenses/virtio-win-BSD-3-Clause.txt")
    if sha256(driver_license) != DRIVER_LICENSE_SHA256:
        fail("pinned virtio-win BSD-3-Clause text has changed")
    return rows, mesa_license, driver_license


def regular_file(path: Path, label: str) -> None:
    if not path.is_file() or path.is_symlink():
        fail(f"{label} is missing or is a symlink: {path}")


def modification_record(mesa_revision: str, driver_binary_included: bool) -> str:
    return "\n".join(
        (
            "format=bridgevm-windows-graphics-third-party-v1",
            "distribution_scope=graphics-lab-artifact",
            "general_preview_included=false",
            "driver_component=virtio-win viogpu3d",
            "driver_upstream=https://github.com/arehnman/kvm-guest-drivers-windows",
            f"driver_revision={DRIVER_REVISION}",
            f"driver_binary_included={str(driver_binary_included).lower()}",
            "driver_modified=false",
            "mesa_component=Mesa Windows ARM64 UMD",
            "mesa_upstream=https://github.com/arehnman/virtio-win-mesa",
            f"mesa_revision={mesa_revision}",
            "mesa_license_model=per-file",
            "mesa_modified=true",
            "mesa_patches=" + ",".join(MESA_PATCHES),
            "",
        )
    )


def write_sums(output: Path) -> None:
    lines = [f"{sha256(output / name)}  {name}" for name in sorted(NOTICE_FILES)]
    (output / SUMS_FILE).write_text("\n".join(lines) + "\n", encoding="ascii")


def assemble(mesa_source: Path, output: Path) -> None:
    root = source_root()
    rows, mesa_license, driver_license = local_notice_sources(root)
    mesa_revision = next(iter(rows.values()))["upstream_revision"]
    try:
        actual_revision = subprocess.run(
            ["git", "-C", str(mesa_source), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"cannot read Mesa source revision: {error}")
    if actual_revision != mesa_revision:
        fail(f"Mesa source is not the registered revision: {actual_revision}")
    output.mkdir(parents=True, exist_ok=True)
    try:
        notice_git.write_pinned_notices(
            mesa_source,
            mesa_revision,
            output / "Mesa-license.rst",
            output / "Mesa-upstream-licenses.zip",
        )
    except notice_git.NoticeObjectError as error:
        fail(str(error))
    shutil.copyfile(mesa_license, output / mesa_license.name)
    shutil.copyfile(driver_license, output / driver_license.name)
    for name, row in rows.items():
        shutil.copyfile(source_asset(root, row["path"]), output / name)
    if sha256(output / "Mesa-license.rst") != MESA_OVERVIEW_SHA256:
        fail("Mesa licence overview differs from the registered revision")
    if sha256(output / "Mesa-upstream-licenses.zip") != MESA_LICENSE_ARCHIVE_SHA256:
        fail("Mesa upstream licence archive differs from the registered revision")
    (output / "BridgeVM-MODIFICATIONS.txt").write_text(
        modification_record(mesa_revision, (output / "viogpu3d.sys").is_file()),
        encoding="ascii",
    )
    write_sums(output)
    verify(output)


def read_sums(package: Path) -> dict[str, str]:
    sums_path = package / SUMS_FILE
    regular_file(sums_path, "notice digest manifest")
    entries: dict[str, str] = {}
    for line in sums_path.read_text(encoding="ascii").splitlines():
        parts = line.split("  ", 1)
        if (
            len(parts) != 2
            or len(parts[0]) != 64
            or any(c not in "0123456789abcdef" for c in parts[0])
        ):
            fail(f"invalid notice digest line: {line}")
        digest, name = parts
        if name in entries:
            fail(f"duplicate notice digest entry: {name}")
        entries[name] = digest
    if set(entries) != set(NOTICE_FILES):
        fail("notice digest manifest does not name the exact required set")
    return entries


def verify(package: Path) -> None:
    root = source_root()
    rows, mesa_license, driver_license = local_notice_sources(root)
    for name in (*NOTICE_FILES, SUMS_FILE):
        regular_file(package / name, "packaged notice")
    entries = read_sums(package)
    for name, expected in entries.items():
        if sha256(package / name) != expected:
            fail(f"packaged notice digest mismatch: {name}")
    if (package / mesa_license.name).read_bytes() != mesa_license.read_bytes():
        fail("packaged Mesa patched-file licence differs from its source")
    if (package / driver_license.name).read_bytes() != driver_license.read_bytes():
        fail("packaged virtio-win licence differs from its source")
    for name, row in rows.items():
        if (package / name).read_bytes() != source_asset(root, row["path"]).read_bytes():
            fail(f"packaged Mesa patch differs from its source: {name}")
    overview = (package / "Mesa-license.rst").read_text(encoding="utf-8")
    if "individual files may have their own licenses" not in overview:
        fail("Mesa licence overview does not disclose its per-file licence model")
    with zipfile.ZipFile(package / "Mesa-upstream-licenses.zip") as archive:
        members = archive.infolist()
        names = [member.filename for member in members]
        unsafe = any(
            name.startswith(("/", "../")) or "/../" in name or "\\" in name
            for name in names
        )
        symlink = any(
            ((member.external_attr >> 16) & 0o170000) == 0o120000
            for member in members
        )
        noncanonical = any(
            member.compress_type != zipfile.ZIP_STORED
            or member.create_system != 3
            or member.date_time != notice_git.FIXED_TIMESTAMP
            or member.external_attr >> 16 != 0o100644
            for member in members
        )
        if not names or len(names) != len(set(names)) or unsafe or symlink or noncanonical:
            fail("Mesa upstream licence archive is empty, unsafe, or noncanonical")
    mesa_revision = next(iter(rows.values()))["upstream_revision"]
    record = (package / "BridgeVM-MODIFICATIONS.txt").read_text(encoding="ascii")
    driver_binary_included = str((package / "viogpu3d.sys").is_file()).lower()
    for marker in (
        "distribution_scope=graphics-lab-artifact",
        "general_preview_included=false",
        f"driver_revision={DRIVER_REVISION}",
        f"driver_binary_included={driver_binary_included}",
        "driver_modified=false",
        f"mesa_revision={mesa_revision}",
        "mesa_modified=true",
    ):
        if marker not in record.splitlines():
            fail(f"modification record is missing: {marker}")


def stage(source: Path, output: Path) -> None:
    verify(source)
    output.mkdir(parents=True, exist_ok=True)
    for name in (*NOTICE_FILES, SUMS_FILE):
        shutil.copyfile(source / name, output / name)
    mesa_revision = next(iter(mesa_rows(source_root()).values()))["upstream_revision"]
    (output / "BridgeVM-MODIFICATIONS.txt").write_text(
        modification_record(mesa_revision, (output / "viogpu3d.sys").is_file()),
        encoding="ascii",
    )
    write_sums(output)
    verify(output)


def create_self_test_fixture(output: Path) -> None:
    root = source_root()
    rows, mesa_license, driver_license = local_notice_sources(root)
    mesa_revision = next(iter(rows.values()))["upstream_revision"]
    output.mkdir(parents=True, exist_ok=True)
    (output / "Mesa-license.rst").write_text(
        "Fixture overview: individual files may have their own licenses\n",
        encoding="utf-8",
    )
    shutil.copyfile(mesa_license, output / mesa_license.name)
    shutil.copyfile(driver_license, output / driver_license.name)
    for name, row in rows.items():
        shutil.copyfile(source_asset(root, row["path"]), output / name)
    notice_git.write_canonical_zip(
        [("MIT.txt", b"synthetic upstream MIT fixture\n")],
        output / "Mesa-upstream-licenses.zip",
    )
    (output / "BridgeVM-MODIFICATIONS.txt").write_text(
        modification_record(mesa_revision, (output / "viogpu3d.sys").is_file()),
        encoding="ascii",
    )
    write_sums(output)
    verify(output)


def expect_failure(action, marker: str) -> None:
    try:
        action()
    except SystemExit as error:
        if marker not in str(error):
            fail(f"self-test failed for the wrong reason: {error}")
        return
    fail(f"self-test mutation was accepted: {marker}")


def self_test() -> None:
    notice_git.self_test()
    with tempfile.TemporaryDirectory(prefix="bridgevm-windows-graphics-notices.") as temp:
        package = Path(temp)
        create_self_test_fixture(package)
        record = package / "BridgeVM-MODIFICATIONS.txt"
        record.write_text(
            record.read_text(encoding="ascii") + "tampered=true\n", encoding="ascii"
        )
        expect_failure(lambda: verify(package), "digest mismatch")
        create_self_test_fixture(package)
        record.write_text(
            record.read_text(encoding="ascii").replace(
                "general_preview_included=false", "general_preview_included=true"
            ),
            encoding="ascii",
        )
        write_sums(package)
        expect_failure(lambda: verify(package), "general_preview_included=false")
        create_self_test_fixture(package)
        (package / MESA_PATCHES[0]).unlink()
        expect_failure(lambda: verify(package), "missing or is a symlink")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    assemble_parser = subparsers.add_parser("assemble")
    assemble_parser.add_argument("--mesa-source", type=Path, required=True)
    assemble_parser.add_argument("--output", type=Path, required=True)
    for command in ("verify", "stage"):
        child = subparsers.add_parser(command)
        child.add_argument(
            "--package" if command == "verify" else "--source",
            type=Path,
            required=True,
        )
        if command == "stage":
            child.add_argument("--output", type=Path, required=True)
    subparsers.add_parser("self-test")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "assemble":
        assemble(args.mesa_source.resolve(), args.output.resolve())
    elif args.command == "verify":
        verify(args.package.resolve())
    elif args.command == "self-test":
        self_test()
    else:
        stage(args.source.resolve(), args.output.resolve())
    print(f"windows_graphics_notices=pass mode={args.command}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
