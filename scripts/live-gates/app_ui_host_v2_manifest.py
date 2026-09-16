"""Strict v2 executable and complete signed-bundle input seals."""
import os
from pathlib import Path
import re
import shutil
import sys

from app_ui_diagnostic import digest, regular
from app_ui_host_manifest import verify_executable

TIER = "d6-app-ui-host-v2"
FORMAT = "bridgevm-app-ui-host-v2"
FILES = {
    "binary": "hvf_gic_boot_probe", "launcher": "app-ui-launcher",
    "host_info": "app-ui-host-info.plist", "host_resources": "app-ui-host-CodeResources",
    "driver_info": "app-ui-driver-info.plist", "driver_resources": "app-ui-driver-CodeResources",
}


def safe_regular(path, maximum):
    path = Path(path)
    if not path.is_absolute() or path != path.resolve(strict=True):
        raise ValueError("sealed path must be absolute and contain no symlink ancestors")
    return regular(path, maximum)


def parse_manifest(path, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("invalid commit")
    fields = {}
    for line in safe_regular(path, 16_384).read_text(encoding="utf-8").splitlines():
        row = line.split("\t")
        if not row[0] or row[0] in fields:
            raise ValueError("duplicate or empty manifest field")
        fields[row[0]] = row[1:]
    if set(fields) != {"format", "commit", *FILES}:
        raise ValueError("v2 manifest fields differ")
    if fields["format"] != [FORMAT] or fields["commit"] != [commit]:
        raise ValueError("v2 manifest format or commit differs")
    result = {}
    for key in FILES:
        row = fields[key]
        if (len(row) != 2 or not Path(row[0]).is_absolute()
                or not re.fullmatch(r"[0-9a-f]{64}", row[1])):
            raise ValueError("sealed input requires absolute path and digest")
        result[key] = (Path(row[0]), row[1])
    return result


def verify_input(key, path, expected):
    maximum = 1024 * 1024 * 1024 if key in ("binary", "launcher") else (
        65_536 if key.endswith("_info") else 1024 * 1024)
    safe_regular(path, maximum)
    if key in ("binary", "launcher"):
        verify_executable(path, expected)
    elif digest(path) != expected:
        raise ValueError("sealed metadata digest differs")


def validate_manifest(path, commit):
    fields = parse_manifest(path, commit)
    for key, (source, expected) in fields.items():
        verify_input(key, source, expected)
    return fields


def copy_input(key, source, destination, expected):
    verify_input(key, source, expected)
    with Path(source).open("rb") as reader, Path(destination).open("xb") as writer:
        os.fchmod(writer.fileno(), 0o500 if key in ("binary", "launcher") else 0o600)
        shutil.copyfileobj(reader, writer, 1024 * 1024)
        writer.flush()
        os.fsync(writer.fileno())
    verify_input(key, destination, expected)


def seal_inputs(manifest, commit, staging):
    staging = Path(staging)
    if not staging.is_dir() or staging != staging.resolve(strict=True):
        raise ValueError("staging must be a canonical owned directory")
    fields = parse_manifest(manifest, commit)
    verify_input("binary", staging / FILES["binary"], fields["binary"][1])
    for key in FILES:
        if key != "binary":
            copy_input(key, fields[key][0], staging / FILES[key], fields[key][1])


if __name__ == "__main__":
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "validate-manifest":
            validate_manifest(*sys.argv[2:])
        elif len(sys.argv) == 5 and sys.argv[1] == "seal-inputs":
            seal_inputs(*sys.argv[2:])
        else:
            raise ValueError("expected validate-manifest or seal-inputs")
    except (OSError, ValueError) as error:
        print(f"v2 app UI inputs refused: {error}", file=sys.stderr)
        sys.exit(2)
