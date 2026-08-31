#!/usr/bin/env python3
"""Check the bounded BridgeVM PC BDS firmware volume and image order."""
import pathlib
import struct
import sys


def pe_size(image: bytes) -> int:
    pe_offset = struct.unpack_from("<I", image, 0x3C)[0]
    if image[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise ValueError("embedded image is not PE/COFF")
    return struct.unpack_from("<I", image, pe_offset + 24 + 56)[0]


def main() -> None:
    if len(sys.argv) != 28:
        raise SystemExit("usage: check-bridgevm-pc-boot-fv.py FV APP IMAGE...")
    fv = pathlib.Path(sys.argv[1]).read_bytes()
    app = pathlib.Path(sys.argv[2]).read_bytes()
    images = [pathlib.Path(path).read_bytes() for path in sys.argv[3:]]
    if len(fv) != 0x100000 or len(images) != 25:
        raise ValueError("boot FV has the wrong bounded size or image count")
    if struct.unpack_from("<Q", fv, 0x20)[0] != len(fv) or fv[0x28:0x2C] != b"_FVH":
        raise ValueError("boot FV header is invalid")
    header_size = struct.unpack_from("<H", fv, 0x30)[0]
    if header_size != 0x48 or sum(struct.unpack(f"<{header_size // 2}H", fv[:header_size])) & 0xFFFF:
        raise ValueError("boot FV header checksum is invalid")
    offsets = [fv.find(image) for image in images]
    if offsets[0] != 0x94 or min(offsets) < 0 or offsets != sorted(offsets):
        raise ValueError(f"boot FV image order is invalid: {offsets}")
    if any(pe_size(image) != len(image) for image in images):
        raise ValueError("boot FV contains a truncated PE/COFF image")
    if fv.find(app) >= 0:
        raise ValueError("removable-media application was incorrectly embedded in firmware")


if __name__ == "__main__":
    main()
