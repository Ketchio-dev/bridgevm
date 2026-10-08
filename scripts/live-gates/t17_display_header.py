"""Bounded exported-frame header interpretation for private packets."""
import struct

def display_header(header: bytes, size: int, cap: int = 64 * 1024 * 1024) -> tuple[int, int] | None:
    if len(header) != 64:
        return None
    magic, version, width, height, stride, fourcc = struct.unpack_from("<6I", header)
    sequence = struct.unpack_from("<Q", header, 24)[0]
    if (magic != 0x42564642 or version != 1 or fourcc != 0x34325258
            or not (0 < width <= 16384 and 0 < height <= 16384)
            or stride < width * 4 or stride % 4 or sequence == 0 or sequence & 1):
        return None
    count = 64 + height * stride
    if count > cap or count > size:
        return None
    return sequence, count
