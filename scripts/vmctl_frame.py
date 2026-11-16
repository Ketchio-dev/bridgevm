"""Read a stable published 2D BVFB frame and encode an RGB PNG."""
import struct
import time
import zlib


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def capture(source, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        source.seek(24)
        sequence_bytes = source.read(8)
        if len(sequence_bytes) != 8:
            raise ValueError("Incomplete framebuffer header.")
        sequence = struct.unpack("<Q", sequence_bytes)[0]
        if sequence & 1:
            time.sleep(0.01)
            continue
        source.seek(0)
        header = source.read(64)
        magic, version, width, height, stride, fourcc = struct.unpack_from("<6I", header)
        if (magic, version, fourcc) != (0x42564642, 1, 0x34325258):
            raise ValueError("Expected a version-1 XRGB8888 2D framebuffer.")
        if not (0 < width <= 16384 and 0 < height <= 16384 and width * 4 <= stride <= 65536 and stride * height <= 256 * 1024 * 1024):
            raise ValueError("Invalid framebuffer geometry.")
        pixels = source.read(stride * height)
        source.seek(24)
        if source.read(8) != sequence_bytes:
            continue
        if len(pixels) != stride * height:
            raise ValueError("Incomplete framebuffer pixels.")
        rows = bytearray()
        for y in range(height):
            row = pixels[y * stride:y * stride + width * 4]
            rgb = bytearray(width * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = row[2::4], row[1::4], row[0::4]
            rows += b"\0" + rgb
        png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", width, height, 8, 2, 0, 0, 0))
        png += chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
        return png, {"width": width, "height": height, "sequence": sequence, "scope": "last-published-2d-frame"}
    raise ValueError("Framebuffer stayed busy; no capture was saved.")
