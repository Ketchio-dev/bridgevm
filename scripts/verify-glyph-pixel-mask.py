#!/usr/bin/env python3
"""Verify glyph regions across repeated captures of the same B6 matrix cell.

The B6 criterion requires "3/3 ... with a verified pixel mask": each declared
region (caption, tab, menu) must show real ink -- not the blank-glyph defect
this whole investigation exists to catch -- and must render byte-identical
across the repeated runs of one cell, proving the result is deterministic
rather than a lucky single frame.

Region content is compared, not whole frames: unrelated live chrome (clock,
notification area, watermark) is expected to differ run to run and must not
fail a cell over pixels the criterion was never about.

No third-party dependencies; reads raw binary PPM (P6) directly.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


class Frame:
    __slots__ = ("width", "height", "pixels")

    def __init__(self, width: int, height: int, pixels: bytes):
        self.width = width
        self.height = height
        self.pixels = pixels

    @classmethod
    def load(cls, path: Path) -> "Frame":
        data = path.read_bytes()
        if not data.startswith(b"P6"):
            raise ValueError(f"{path}: not a binary PPM (P6)")
        fields = []
        offset = 2
        while len(fields) < 3:
            while offset < len(data) and data[offset:offset + 1].isspace():
                offset += 1
            if offset < len(data) and data[offset:offset + 1] == b"#":
                offset = data.index(b"\n", offset) + 1
                continue
            start = offset
            while offset < len(data) and not data[offset:offset + 1].isspace():
                offset += 1
            fields.append(int(data[start:offset]))
        width, height, maxval = fields
        if maxval != 255:
            raise ValueError(f"{path}: unsupported maxval {maxval}")
        offset += 1
        pixels = data[offset:offset + width * height * 3]
        if len(pixels) != width * height * 3:
            raise ValueError(f"{path}: truncated pixel data")
        return cls(width, height, pixels)

    def crop(self, x: int, y: int, w: int, h: int) -> bytes:
        if x < 0 or y < 0 or x + w > self.width or y + h > self.height:
            raise ValueError(
                f"region ({x},{y},{w}x{h}) exceeds frame {self.width}x{self.height}"
            )
        rows = []
        for row in range(y, y + h):
            start = (row * self.width + x) * 3
            rows.append(self.pixels[start:start + w * 3])
        return b"".join(rows)


def ink_ratio(region: bytes) -> float:
    """Fraction of pixels that differ from the region's single most common
    color. Real text over a flat background always has some minority-color
    pixels; a blank region rendered as pure background does not, regardless
    of whether that background is light or dark."""
    if not region:
        return 0.0
    pixel_count = len(region) // 3
    counts = Counter(region[i:i + 3] for i in range(0, len(region), 3))
    background_count = counts.most_common(1)[0][1]
    return (pixel_count - background_count) / pixel_count


def verify_cell(
    regions: dict[str, tuple[int, int, int, int]],
    captures: list[Path],
    min_ink_ratio: float,
) -> dict:
    if len(captures) < 3:
        raise ValueError("at least 3 repeated captures are required per cell")
    frames = [Frame.load(path) for path in captures]
    result: dict = {"regions": {}, "pass": True}
    for name, (x, y, w, h) in regions.items():
        crops = [frame.crop(x, y, w, h) for frame in frames]
        ratios = [ink_ratio(crop) for crop in crops]
        identical = all(crop == crops[0] for crop in crops[1:])
        has_ink = all(ratio >= min_ink_ratio for ratio in ratios)
        region_pass = identical and has_ink
        result["regions"][name] = {
            "ink_ratios": ratios,
            "identical_across_runs": identical,
            "min_ink_ratio_met": has_ink,
            "pass": region_pass,
        }
        result["pass"] = result["pass"] and region_pass
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--regions", type=Path, help="JSON: {name: [x,y,w,h]}")
    parser.add_argument("--captures", nargs="+", type=Path, help=">=3 PPM paths, same cell")
    parser.add_argument("--min-ink-ratio", type=float, default=0.001)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not args.regions or not args.captures:
        parser.error("--regions and --captures are required unless --self-test")
    regions = {name: tuple(box) for name, box in json.loads(args.regions.read_text()).items()}
    result = verify_cell(regions, args.captures, args.min_ink_ratio)
    print(json.dumps(result, indent=2))
    print(f"glyph pixel mask: {'PASS' if result['pass'] else 'FAIL'}")
    return 0 if result["pass"] else 1


def self_test() -> int:
    import tempfile

    def write_ppm(path: Path, width: int, height: int, pixel_fn) -> None:
        rows = bytearray()
        for y in range(height):
            for x in range(width):
                rows.extend(pixel_fn(x, y))
        path.write_bytes(f"P6\n{width} {height}\n255\n".encode() + bytes(rows))

    checks = 0

    def check(label: str, condition: bool) -> None:
        nonlocal checks
        if not condition:
            print(f"FAIL: {label}", file=sys.stderr)
            raise SystemExit(1)
        checks += 1

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)

        # A region with real, identical text ink across 3 runs passes.
        def with_text(x: int, y: int) -> bytes:
            return (0, 0, 0) if (x + y) % 5 == 0 else (255, 255, 255)

        good = [root / f"good{i}.ppm" for i in range(3)]
        for path in good:
            write_ppm(path, 20, 10, with_text)
        result = verify_cell({"caption": (0, 0, 20, 10)}, good, 0.001)
        check("legible identical region passes", result["pass"])
        check(
            "ink ratio measured nonzero",
            result["regions"]["caption"]["ink_ratios"][0] > 0,
        )

        # A blank region (the exact defect under test) fails on ink alone.
        def blank(x: int, y: int) -> bytes:
            return (255, 255, 255)

        blanked = [root / f"blank{i}.ppm" for i in range(3)]
        for path in blanked:
            write_ppm(path, 20, 10, blank)
        result = verify_cell({"caption": (0, 0, 20, 10)}, blanked, 0.001)
        check("blank region fails", not result["pass"])
        check(
            "blank region reports zero ink",
            result["regions"]["caption"]["ink_ratios"][0] == 0.0,
        )

        # Flaky rendering: same ink ratio, different pixels, must still fail.
        def with_text_shifted(x: int, y: int) -> bytes:
            return (0, 0, 0) if (x + y + 1) % 5 == 0 else (255, 255, 255)

        flaky = [root / "flaky0.ppm", root / "flaky1.ppm", root / "flaky2.ppm"]
        write_ppm(flaky[0], 20, 10, with_text)
        write_ppm(flaky[1], 20, 10, with_text_shifted)
        write_ppm(flaky[2], 20, 10, with_text)
        result = verify_cell({"caption": (0, 0, 20, 10)}, flaky, 0.001)
        check("non-identical runs fail despite nonzero ink", not result["pass"])
        check(
            "non-identical runs are reported as such",
            not result["regions"]["caption"]["identical_across_runs"],
        )

        # Independent regions are judged independently: one bad region fails
        # the cell without needing to also break the good one.
        mixed = [root / f"mixed{i}.ppm" for i in range(3)]

        def two_region(x: int, y: int) -> bytes:
            if y < 5:
                return with_text(x, y)
            return blank(x, y)

        for path in mixed:
            write_ppm(path, 20, 10, two_region)
        result = verify_cell(
            {"caption": (0, 0, 20, 5), "menu": (0, 5, 20, 5)}, mixed, 0.001
        )
        check("mixed cell fails overall", not result["pass"])
        check("good region still reports its own pass", result["regions"]["caption"]["pass"])
        check("bad region reports its own failure", not result["regions"]["menu"]["pass"])

        # Fewer than 3 captures must be refused, not silently under-checked.
        try:
            verify_cell({"caption": (0, 0, 20, 10)}, good[:2], 0.001)
            check("too few captures raises", False)
        except ValueError:
            checks += 1

    print(f"glyph pixel mask self-test: PASS ({checks} checks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
