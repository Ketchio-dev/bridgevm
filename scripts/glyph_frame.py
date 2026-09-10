"""Shared exact P6 parsing; the digest covers the same bytes as the pixels."""
import hashlib


class Frame:
    def __init__(self, width, height, pixels, source_sha256=None):
        self.width, self.height, self.pixels = width, height, pixels
        self.source_sha256 = source_sha256

    @classmethod
    def load(cls, path):
        data = path.read_bytes()
        offset, fields = 0, []
        while len(fields) < 4:
            while offset < len(data) and data[offset] in b" \t\r\n\v\f":
                offset += 1
            if offset == len(data):
                raise ValueError("truncated PPM header")
            if data[offset] == 35:
                end = data.find(b"\n", offset)
                if end < 0:
                    raise ValueError("unterminated PPM comment")
                offset = end + 1
                continue
            start = offset
            while offset < len(data) and data[offset] not in b" \t\r\n\v\f":
                offset += 1
            fields.append(data[start:offset])
        if fields[0] != b"P6" or fields[3] != b"255":
            raise ValueError("expected 8-bit binary PPM")
        width, height = int(fields[1]), int(fields[2])
        if width <= 0 or height <= 0 or offset == len(data):
            raise ValueError("invalid PPM geometry or separator")
        size = width * height * 3
        # Accept CRLF without eating an LF-valued first sample after a lone CR.
        separator = data[offset:offset + 2]
        offset += 1
        if separator == b"\r\n" and len(data) - offset == size + 1:
            offset += 1
        pixels = data[offset:]
        if len(pixels) != size:
            raise ValueError("PPM raster length mismatch")
        return cls(width, height, pixels, hashlib.sha256(data).hexdigest())
