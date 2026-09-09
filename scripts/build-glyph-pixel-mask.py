#!/usr/bin/env python3
"""Propose a B6 glyph mask from one reference capture, for a human to review.

verify-glyph-pixel-mask.py compares captures against reviewed reference pixels,
and the criterion says the mask is reviewed. A mask nobody can see is reviewed
in name only, so this writes, beside the JSON, one PPM per region showing which
pixels it selected: chosen glyph strokes in red, background guards in blue,
everything else as captured. Look at those before trusting the mask.

Selection is deterministic and dumb on purpose -- the darkest pixels in the
region are taken as strokes and the lightest as guards, ties broken by index.
It proposes; it does not certify. A region whose strokes land on wallpaper
instead of text is a badly chosen box, and the review PPM is where that shows.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import sys
from pathlib import Path

STROKES = 48
GUARDS = 16


def load_ppm(path):
    data = path.read_bytes()
    fields, offset = [], 0
    while len(fields) < 4:
        end = offset
        while end < len(data) and data[end : end + 1] not in b" \t\r\n":
            end += 1
        token = data[offset:end]
        if token.startswith(b"#"):
            while end < len(data) and data[end : end + 1] not in b"\r\n":
                end += 1
        elif token:
            fields.append(token)
        offset = end + 1
    if fields[0] != b"P6" or fields[3] != b"255":
        raise ValueError("only binary 8-bit P6 PPM is supported")
    width, height = int(fields[1]), int(fields[2])
    pixels = data[offset : offset + width * height * 3]
    if len(pixels) != width * height * 3:
        raise ValueError("truncated PPM payload")
    return width, height, bytearray(pixels)


def region_pixels(width, box):
    x, y, w, h = box
    return [((y + i // w) * width + x + i % w) * 3 for i in range(w * h)]


def select(width, height, pixels, box):
    x, y, w, h = box
    if min(x, y) < 0 or min(w, h) <= 0 or x + w > width or y + h > height:
        raise ValueError(f"region {box} does not fit in {width}x{height}")
    if w * h < STROKES + GUARDS:
        raise ValueError(f"region {box} is too small to select from")
    offsets = region_pixels(width, box)
    # Rank by luminance, index breaking ties so the same reference always
    # yields the same mask.
    ranked = sorted(
        range(len(offsets)),
        key=lambda i: (
            pixels[offsets[i]] * 299 + pixels[offsets[i] + 1] * 587 + pixels[offsets[i] + 2] * 114,
            i,
        ),
    )
    return sorted(ranked[:STROKES]), sorted(ranked[-GUARDS:])


def review_ppm(width, pixels, box, strokes, guards, out):
    x, y, w, h = box
    crop = bytearray()
    marked = {i: (255, 0, 0) for i in strokes}
    marked.update({i: (0, 0, 255) for i in guards})
    for i in range(w * h):
        offset = ((y + i // w) * width + x + i % w) * 3
        crop += bytes(marked[i]) if i in marked else pixels[offset : offset + 3]
    out.write_bytes(b"P6\n%d %d\n255\n" % (w, h) + bytes(crop))


def build(reference, boxes, out, review_dir):
    width, height, pixels = load_ppm(reference)
    regions = {}
    for name, box in boxes.items():
        strokes, guards = select(width, height, pixels, box)
        regions[name] = {"box": list(box), "pixels": sorted(strokes + guards)}
        if review_dir is not None:
            review_dir.mkdir(parents=True, exist_ok=True)
            review_ppm(width, pixels, box, strokes, guards, review_dir / f"{name}.ppm")
    mask = {
        "reference_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
        "regions": regions,
    }
    out.write_text(json.dumps(mask, indent=2) + "\n")
    return mask


def parse_region(value):
    name, _, rest = value.partition("=")
    box = tuple(int(v) for v in rest.split(","))
    if not name or len(box) != 4:
        raise argparse.ArgumentTypeError(f"expected name=x,y,w,h, got {value!r}")
    return name, box


def self_test():
    import tempfile

    with tempfile.TemporaryDirectory() as root:
        root = Path(root)
        width = height = 16
        pixels = bytearray([200] * (width * height * 3))
        for i in range(0, 64):
            pixels[i * 3 : i * 3 + 3] = bytes([10, 10, 10])
        reference = root / "ref.ppm"
        reference.write_bytes(b"P6\n16 16\n255\n" + bytes(pixels))
        mask = build(reference, {"caption": (0, 0, 16, 16)}, root / "m.json", root / "review")
        chosen = mask["regions"]["caption"]["pixels"]
        assert len(chosen) == STROKES + GUARDS
        assert len(set(chosen)) == len(chosen)
        assert all(p < 64 for p in chosen[:STROKES]), "strokes must be the dark pixels"
        assert (root / "review" / "caption.ppm").exists()
        again = build(reference, {"caption": (0, 0, 16, 16)}, root / "m2.json", None)
        assert again == mask, "selection must be deterministic"
        for bad in ((0, 0, 4, 4), (10, 10, 16, 16)):
            try:
                build(reference, {"caption": bad}, root / "m3.json", None)
            except ValueError:
                continue
            raise AssertionError(f"accepted an invalid region {bad}")
    print("glyph pixel mask builder self-test: PASS")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--region", type=parse_region, action="append", default=[])
    parser.add_argument("--out", type=Path)
    parser.add_argument("--review-dir", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.reference or not args.out or not args.region:
        parser.error("--reference, --region and --out are required")
    build(args.reference, dict(args.region), args.out, args.review_dir)
    print(f"proposed mask written to {args.out}; review the PPMs before using it")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
