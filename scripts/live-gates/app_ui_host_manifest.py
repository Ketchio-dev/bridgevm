#!/usr/bin/env python3
"""Seal the two immutable native executables for the normal-app diagnostic."""
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import sys

from app_ui_diagnostic import digest, regular

TIER = "d6-app-ui-host-v1"
FORMAT = "bridgevm-app-ui-host-v1"
LAUNCHER = "app-ui-launcher"


def parse_manifest(path, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("invalid commit")
    fields = {}
    for line in regular(path, 16_384).read_text(encoding="utf-8").splitlines():
        values = line.split("\t")
        if values[0] in fields:
            raise ValueError("duplicate manifest field")
        fields[values[0]] = values[1:]
    if set(fields) != {"format", "commit", "binary", "launcher"}:
        raise ValueError("manifest fields differ")
    if fields["format"] != [FORMAT] or fields["commit"] != [commit]:
        raise ValueError("manifest format or commit differs")
    result = {}
    for key in ("binary", "launcher"):
        row = fields[key]
        if (len(row) != 2 or not Path(row[0]).is_absolute()
                or not re.fullmatch(r"[0-9a-f]{64}", row[1])):
            raise ValueError("executable needs an absolute path and digest")
        result[key] = (Path(row[0]), row[1])
    return result


def verify_executable(path, expected):
    path = regular(path, 1024 * 1024 * 1024)
    with path.open("rb") as source:
        header = source.read(32)
    if len(header) != 32 or header[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("expected a native 64-bit Mach-O executable")
    cpu, _, kind = struct.unpack_from("<III", header, 4)
    if cpu not in (0x0100000C, 0x01000007) or kind != 2:
        raise ValueError("expected Mach-O MH_EXECUTE")
    native = {"arm64": 0x0100000C, "x86_64": 0x01000007}.get(platform.machine())
    if sys.platform == "darwin" and cpu != native:
        raise ValueError("executable architecture differs from the native host")
    if digest(path) != expected:
        raise ValueError("sealed executable digest differs")


def validate_manifest(path, commit):
    fields = parse_manifest(path, commit)
    for executable, expected in fields.values():
        verify_executable(executable, expected)
    return fields


def seal_launcher(manifest, commit, staging):
    staging = Path(staging)
    if not staging.is_dir() or staging.is_symlink():
        raise ValueError("staging must be an owned directory")
    fields = parse_manifest(manifest, commit)
    verify_executable(staging / "hvf_gic_boot_probe", fields["binary"][1])
    source, expected = fields["launcher"]
    verify_executable(source, expected)
    destination = staging / LAUNCHER
    with source.open("rb") as reader, destination.open("xb") as writer:
        shutil.copyfileobj(reader, writer, 1024 * 1024)
        writer.flush()
        os.fsync(writer.fileno())
    destination.chmod(0o500)
    verify_executable(destination, expected)


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "validate-manifest":
        validate_manifest(*sys.argv[2:])
    elif len(sys.argv) == 5 and sys.argv[1] == "seal-launcher":
        seal_launcher(*sys.argv[2:])
    else:
        raise ValueError("expected validate-manifest MANIFEST COMMIT or seal-launcher MANIFEST COMMIT STAGING")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(f"app UI host manifest refused: {error}", file=sys.stderr)
        sys.exit(2)
