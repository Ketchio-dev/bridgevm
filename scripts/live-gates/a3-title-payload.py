#!/usr/bin/env python3
"""Validate a sealed PPSSPP ZIP and print its embedded ARM64 executable hash."""

from __future__ import annotations

import hashlib
import re
import stat
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

EXECUTABLE = "ppsspp/PPSSPPWindowsARM64.exe"
MAX_ARCHIVE_BYTES = 256 * 1024 * 1024
MAX_EXPANDED_BYTES = 256 * 1024 * 1024
MAX_ENTRIES = 4096
ALLOWED_COMPRESSION = {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED}
DOS_DEVICE = re.compile(r"^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:[.]|$)", re.I)


def validate(path: Path) -> str:
    if not path.is_file() or path.is_symlink() or path.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("PPSSPP payload must be a bounded direct regular file")
    with zipfile.ZipFile(path) as payload:
        infos = payload.infolist()
        if not infos or len(infos) > MAX_ENTRIES:
            raise ValueError("PPSSPP payload entry count is invalid")
        total = 0
        matches: list[zipfile.ZipInfo] = []
        seen: set[str] = set()
        for info in infos:
            name = info.filename
            normalized = name.rstrip("/")
            archive_path = PurePosixPath(normalized)
            mode = (info.external_attr >> 16) & 0o170000
            parts = archive_path.parts
            unsafe_component = any(
                not part or part[-1] in " ." or ":" in part
                or any(ord(char) < 32 for char in part) or DOS_DEVICE.match(part)
                for part in parts
            )
            key = normalized.casefold()
            if (not normalized or "\\" in name or archive_path.is_absolute()
                    or ".." in parts or normalized != archive_path.as_posix()
                    or not (normalized == "ppsspp" or normalized.startswith("ppsspp/"))
                    or "/._" in normalized
                    or unsafe_component or key in seen or info.flag_bits & 1
                    or mode not in (0, stat.S_IFREG, stat.S_IFDIR)
                    or info.compress_type not in ALLOWED_COMPRESSION):
                raise ValueError("PPSSPP payload contains an unsafe entry")
            seen.add(key)
            total += info.file_size
            if total > MAX_EXPANDED_BYTES:
                raise ValueError("PPSSPP payload expands beyond its bound")
            if normalized == EXECUTABLE:
                matches.append(info)
        if len(matches) != 1:
            raise ValueError("PPSSPP payload must contain exactly one ARM64 executable")
        bad_crc = payload.testzip()
        if bad_crc is not None:
            raise ValueError(f"PPSSPP payload has a corrupt entry: {bad_crc}")
        digest = hashlib.sha256()
        with payload.open(matches[0]) as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()


def self_test() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        good = root / "good.zip"
        with zipfile.ZipFile(good, "w") as payload:
            payload.writestr(EXECUTABLE, b"arm64-payload")
            payload.writestr("ppsspp/assets/font", b"asset")
        expected = hashlib.sha256(b"arm64-payload").hexdigest()
        assert validate(good) == expected
        bad_entries = {
            "traversal": [("../escape", b"bad"), (EXECUTABLE, b"arm64-payload")],
            "case-alias": [(EXECUTABLE, b"arm64-payload"),
                           ("ppsspp/ppssppwindowsarm64.EXE", b"alias")],
            "ads": [(EXECUTABLE, b"arm64-payload"), ("ppsspp/file:stream", b"bad")],
            "outside-root": [(EXECUTABLE, b"arm64-payload"), ("other/file", b"bad")],
        }
        for label, entries in bad_entries.items():
            path = root / f"{label}.zip"
            with zipfile.ZipFile(path, "w") as payload:
                for name, data in entries:
                    payload.writestr(name, data)
            try:
                validate(path)
            except ValueError:
                pass
            else:
                raise AssertionError(f"unsafe payload accepted: {label}")
        symlink = root / "symlink.zip"
        with zipfile.ZipFile(symlink, "w") as payload:
            payload.writestr(EXECUTABLE, b"arm64-payload")
            info = zipfile.ZipInfo("ppsspp/link")
            info.create_system = 3
            info.external_attr = (stat.S_IFLNK | 0o777) << 16
            payload.writestr(info, b"target")
        try:
            validate(symlink)
        except ValueError:
            pass
        else:
            raise AssertionError("symlink payload entry accepted")
    print("PASS: A3 PPSSPP payload policy")


def main() -> int:
    try:
        if sys.argv[1:] == ["--self-test"]:
            self_test()
        elif len(sys.argv) == 2:
            print(validate(Path(sys.argv[1])))
        else:
            raise ValueError("usage: a3-title-payload.py ARCHIVE | --self-test")
    except (OSError, RuntimeError, ValueError, zipfile.BadZipFile) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
