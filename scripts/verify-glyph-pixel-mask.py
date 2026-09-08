#!/usr/bin/env python3
"""Compare captures with an independently reviewed reference and explicit mask.

A match is only a pixel comparison, not a B6 campaign pass. The caller must
separately prove reference correctness, effective DPI, independent live runs,
presentation freshness and the frame-time budget. Background variation and
agreement between candidate images are not evidence of correct glyphs.

Mask JSON: {"reference_sha256": "...", "regions": {"caption":
{"box": [x,y,w,h], "pixels": [local_pixel_index, ...]}, ...}}.
Each selected pixel is compared exactly with the corresponding reference RGB.
Include both expected glyph strokes and background guards in reviewed masks.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


class Frame:
    def __init__(self, width, height, pixels):
        self.width, self.height, self.pixels = width, height, pixels

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
        offset += 2 if data[offset:offset + 2] == b"\r\n" else 1
        pixels = data[offset:]
        if len(pixels) != width * height * 3:
            raise ValueError("PPM raster length mismatch")
        return cls(width, height, pixels)


def selected_offsets(region, frame):
    box, pixels = region["box"], region["pixels"]
    if len(box) != 4 or any(type(v) is not int for v in box):
        raise ValueError("box must contain four integers")
    x, y, width, height = box
    if min(x, y) < 0 or min(width, height) <= 0:
        raise ValueError("invalid mask rectangle")
    if x + width > frame.width or y + height > frame.height:
        raise ValueError("mask rectangle exceeds frame")
    if not pixels or any(type(v) is not int or not 0 <= v < width * height for v in pixels):
        raise ValueError("mask must select valid pixels")
    if len(set(pixels)) != len(pixels):
        raise ValueError("duplicate mask pixels")
    return [((y + p // width) * frame.width + x + p % width) * 3 for p in pixels]


def verify_cell(mask, reference, captures):
    if len(captures) != 3 or len({p.resolve() for p in captures}) != 3:
        raise ValueError("exactly three distinct capture paths required")
    if reference.resolve() in {p.resolve() for p in captures}:
        raise ValueError("reference cannot be a candidate capture")
    if hashlib.sha256(reference.read_bytes()).hexdigest() != mask["reference_sha256"]:
        raise ValueError("reference hash mismatch")
    regions = mask["regions"]
    if set(regions) != {"caption", "menu", "tab"}:
        raise ValueError("caption, menu and tab masks are all required")
    expected = Frame.load(reference)
    frames = [Frame.load(p) for p in captures]
    if any((f.width, f.height) != (expected.width, expected.height) for f in frames):
        raise ValueError("capture geometry differs from reference")
    results = {}
    for name, region in regions.items():
        offsets = selected_offsets(region, expected)
        if len({expected.pixels[o:o + 3] for o in offsets}) < 2:
            raise ValueError("mask must include foreground and background")
        mismatches = [sum(f.pixels[o:o + 3] != expected.pixels[o:o + 3] for o in offsets)
                      for f in frames]
        results[name] = {"selected_pixels": len(offsets), "mismatches": mismatches,
                         "matches_reference": not any(mismatches)}
    return {"scope": "reference-mask-comparison-only", "regions": results,
            "matches_reference": all(r["matches_reference"] for r in results.values())}


def self_test():
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        reference = root / "reference.ppm"
        reference.write_bytes(b"P6\n2 1\n255\n" + bytes([0, 0, 0, 255, 255, 255]))
        captures = [root / f"run{i}.ppm" for i in range(3)]
        for p in captures:
            p.write_bytes(reference.read_bytes())
        mask = {"reference_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
                "regions": {n: {"box": [0, 0, 2, 1], "pixels": [0, 1]}
                            for n in ("caption", "menu", "tab")}}
        assert verify_cell(mask, reference, captures)["matches_reference"]
        # Repeatable non-text background must not pass in place of the glyph.
        for p in captures:
            p.write_bytes(b"P6\n2 1\n255\n" + bytes([240, 240, 240, 241, 241, 241]))
        assert not verify_cell(mask, reference, captures)["matches_reference"]
        for bad_mask, bad_captures in ((dict(mask, regions={}), captures),
                                       (mask, [captures[0]] * 3),
                                       (dict(mask, reference_sha256="0" * 64), captures)):
            try:
                verify_cell(bad_mask, reference, bad_captures)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid evidence accepted")
    print("glyph pixel mask self-test: PASS")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mask", type=Path)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--captures", nargs=3, type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not all((args.mask, args.reference, args.captures)):
        parser.error("--mask, --reference and --captures are required")
    try:
        result = verify_cell(json.loads(args.mask.read_text()), args.reference, args.captures)
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.exit(2, f"invalid mask evidence: {error}\n")
    print(json.dumps(result, indent=2))
    return 0 if result["matches_reference"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
