#!/usr/bin/env python3
"""Verify one B6 cell: every scene matches, and the three glyph classes are covered.

B6 asks for caption, menu and tab glyphs. A 2026-09-08 spike asked whether one
scene could supply all three and answered no -- packaged Notepad's tab row is
not a classic caption bar, so the classic caption/menu scene is retained
alongside the packaged tab/menu one. verify-glyph-pixel-mask.py therefore
checks one scene against one reference, and this checks the cell those scenes
add up to: each scene must match its reviewed mask, and the union of their
regions must be exactly caption, menu and tab. A cell that verifies two scenes
covering only caption and menu has proven nothing about tab glyphs, and says so
rather than passing.
"""
from __future__ import annotations
import argparse
import importlib.util
import json
from pathlib import Path

REQUIRED = {"caption", "menu", "tab"}


def load_verifier():
    path = Path(__file__).resolve().parent / "verify-glyph-pixel-mask.py"
    spec = importlib.util.spec_from_file_location("glyph_mask", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_scene(verifier, scene):
    mask = json.loads(Path(scene["mask"]).read_text())
    reference = Path(scene["reference"])
    captures = [Path(c) for c in scene["captures"]]
    result = verifier.verify_cell(mask, reference, captures)
    return set(mask["regions"]), result


def verify_cell(scenes, verifier=None):
    verifier = verifier or load_verifier()
    if not scenes:
        raise ValueError("a cell needs at least one scene")
    covered, results = set(), {}
    for name, scene in scenes.items():
        regions, result = verify_scene(verifier, scene)
        results[name] = result
        if result["matches_reference"]:
            # Only a matching scene contributes coverage. A scene that failed
            # its mask has not shown its glyph classes render correctly.
            covered |= regions
    missing = sorted(REQUIRED - covered)
    return {
        "scenes": results,
        "covered": sorted(covered),
        "missing": missing,
        "matches_reference": not missing and all(r["matches_reference"] for r in results.values()),
    }


def self_test():
    import hashlib
    import tempfile

    class FakeVerifier:
        def __init__(self, verdicts):
            self.verdicts = verdicts

        def verify_cell(self, mask, reference, captures):
            return {"matches_reference": self.verdicts[mask["scene"]]}

    def scenes(root, layout):
        out = {}
        for name, regions in layout.items():
            mask = root / f"{name}.json"
            mask.write_text(json.dumps({"scene": name, "regions": {r: {} for r in regions}}))
            out[name] = {"mask": str(mask), "reference": str(root / "r.ppm"), "captures": []}
        return out

    with tempfile.TemporaryDirectory() as root:
        root = Path(root)
        both = {"classic": ("caption", "menu"), "packaged": ("tab", "menu")}
        ok = verify_cell(scenes(root, both), FakeVerifier({"classic": True, "packaged": True}))
        assert ok["matches_reference"] and not ok["missing"], ok
        assert ok["covered"] == ["caption", "menu", "tab"]

        # A scene that fails its mask cannot lend its coverage to the cell.
        bad = verify_cell(scenes(root, both), FakeVerifier({"classic": True, "packaged": False}))
        assert not bad["matches_reference"] and bad["missing"] == ["tab"], bad

        # Two matching scenes that never look at tab glyphs are not a pass.
        partial = {"classic": ("caption", "menu"), "second": ("caption", "menu")}
        thin = verify_cell(scenes(root, partial), FakeVerifier({"classic": True, "second": True}))
        assert not thin["matches_reference"] and thin["missing"] == ["tab"], thin

        try:
            verify_cell({}, FakeVerifier({}))
        except ValueError:
            pass
        else:
            raise AssertionError("an empty cell was accepted")
        assert hashlib.sha256(b"").hexdigest()
    print("b6 cell verification self-test: PASS")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cell", type=Path, help="JSON: {scene: {mask, reference, captures}}")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.cell:
        parser.error("--cell is required")
    result = verify_cell(json.loads(args.cell.read_text()))
    print(json.dumps(result, indent=2))
    return 0 if result["matches_reference"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
