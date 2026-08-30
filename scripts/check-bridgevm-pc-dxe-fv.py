#!/usr/bin/env python3
"""Validate the bounded DXE firmware volume assembled by the build script."""

import pathlib
import struct
import sys


def pe_size(image: bytes) -> int:
    pe_offset = struct.unpack_from("<I", image, 0x3C)[0]
    assert image[pe_offset : pe_offset + 4] == b"PE\0\0"
    return struct.unpack_from("<I", image, pe_offset + 24 + 56)[0]


def main() -> None:
    if len(sys.argv) != 6:
        raise SystemExit("usage: check-bridgevm-pc-dxe-fv.py CORE RUNTIME PLATFORM PROBE FV")
    core, runtime, platform, probe, fv = (
        pathlib.Path(path).read_bytes() for path in sys.argv[1:]
    )
    assert len(core) == 0x17000
    assert len(runtime) == 0x6000
    assert len(platform) == 0x3000
    assert 0 < len(probe) <= 0x10000 and len(probe) % 0x1000 == 0
    assert len(fv) == 0x100000
    offsets = [fv.find(image) for image in (core, runtime, platform, probe)]
    assert offsets[0] == 0x94 and offsets == sorted(offsets) and min(offsets) >= 0
    assert struct.unpack_from("<Q", fv, 0x20)[0] == len(fv)
    assert fv[0x28:0x2C] == b"_FVH"
    header_size = struct.unpack_from("<H", fv, 0x30)[0]
    assert header_size == 0x48
    assert sum(struct.unpack(f"<{header_size // 2}H", fv[:header_size])) & 0xFFFF == 0
    assert all(pe_size(image) == len(image) for image in (core, runtime, platform, probe))
    pe_offset = struct.unpack_from("<I", core, 0x3C)[0]
    optional = pe_offset + 24
    assert struct.unpack_from("<I", core, optional + 16)[0] == 0x6BEC
    assert struct.unpack_from("<Q", core, optional + 24)[0] == 0x100400000


if __name__ == "__main__":
    main()
