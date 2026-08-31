#!/usr/bin/env python3
"""Validate the bounded BridgeVM DXE firmware volume and its embedded images."""
import pathlib
import struct
import sys

def pe_size(image: bytes) -> int:
    pe_offset = struct.unpack_from("<I", image, 0x3C)[0]
    assert image[pe_offset : pe_offset + 4] == b"PE\0\0"
    return struct.unpack_from("<I", image, pe_offset + 24 + 56)[0]

def main() -> None:
    if len(sys.argv) != 12:
        raise SystemExit(
            "usage: check-bridgevm-pc-dxe-fv.py CORE RUNTIME VARIABLES PLATFORM "
            "CPU CPU_IO PCI_BUS PCI_HOST NVME PROBE FV"
        )
    *image_paths, fv_path = sys.argv[1:]
    images = [pathlib.Path(path).read_bytes() for path in image_paths]
    fv = pathlib.Path(fv_path).read_bytes()
    expected_sizes = [0x17000, 0x4000, 0xA000, 0x3000, 0x9000, 0x4000, 0x11000, 0x6000]
    assert [len(image) for image in images[:8]] == expected_sizes
    assert 0 < len(images[8]) <= 0x40000 and len(images[8]) % 0x1000 == 0 and 0 < len(images[9]) <= 0x10000 and len(images[9]) % 0x1000 == 0
    assert len(fv) == 0x100000
    offsets = [fv.find(image) for image in images]
    assert offsets[0] == 0x94 and offsets == sorted(offsets) and min(offsets) >= 0
    assert struct.unpack_from("<Q", fv, 0x20)[0] == len(fv)
    assert fv[0x28:0x2C] == b"_FVH"
    header_size = struct.unpack_from("<H", fv, 0x30)[0]
    assert header_size == 0x48
    assert sum(struct.unpack(f"<{header_size // 2}H", fv[:header_size])) & 0xFFFF == 0
    assert all(pe_size(image) == len(image) for image in images)
    pe_offset = struct.unpack_from("<I", images[0], 0x3C)[0]
    optional = pe_offset + 24
    assert struct.unpack_from("<I", images[0], optional + 16)[0] == 0x6BF4
    assert struct.unpack_from("<Q", images[0], optional + 24)[0] == 0x100400000

if __name__ == "__main__":
    main()
