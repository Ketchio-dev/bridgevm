#!/usr/bin/env python3
"""Compare exactly against reviewed reference RGB pixels; not a B6 campaign pass.
Callers must prove reference correctness, effective DPI, independent live runs,
presentation freshness and frame time; candidate/background agreement is not proof.
Review strokes and guards. A mask covers one scene; verify-b6-cell.py requires all three.
Mask JSON: {"reference_sha256": .., "regions": {"caption": {"box": [x,y,w,h],
"pixels": [i, ..]}, ..}}.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from glyph_frame import Frame


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
    if len(captures) != 3 or any(p.samefile(q) for i, p in enumerate(captures) for q in captures[:i]):
        raise ValueError("exactly three distinct capture files required")
    if any(reference.samefile(p) for p in captures):
        raise ValueError("reference cannot be a candidate capture")
    expected = Frame.load(reference)
    if expected.source_sha256 != mask["reference_sha256"]:
        raise ValueError("reference hash mismatch")
    regions = mask["regions"]
    if not regions or regions.keys() - {"caption", "menu", "tab"}:
        raise ValueError("regions must be drawn from caption, menu and tab")
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
        reference_alias, capture_alias = root / "reference-alias.ppm", root / "capture-alias.ppm"
        os.link(reference, reference_alias)
        os.link(captures[0], capture_alias)
        for bad_mask, bad_captures in ((mask, [reference_alias, *captures[1:]]),
                                       (mask, [captures[0], capture_alias, captures[2]]),
                                       (dict(mask, regions={}), captures),
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
