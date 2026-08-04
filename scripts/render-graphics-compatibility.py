#!/usr/bin/env python3
"""Render the graphics compatibility matrix from its registry.

The matrix is generated rather than written because a hand-maintained
compatibility table drifts from what the code actually does, and the direction
it drifts in is always optimistic.

The renderer also refuses to emit a full-FL11_0 conformance claim while any
Vulkan feature is listed as relaxed. That check is the point: DXVK asks for
five features the Venus path does not provide, so the honest claim is an
experimental subset, and the tool will not let a future edit quietly upgrade
the wording without also removing the relaxations.

Usage:
  render-graphics-compatibility.py            # write the generated block
  render-graphics-compatibility.py --check    # fail if it is out of date
  render-graphics-compatibility.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "docs/windows-arm/graphics-compatibility.json"
DOCUMENT = ROOT / "docs/windows-arm/graphics-compatibility.md"
BEGIN = "<!-- BEGIN GENERATED: graphics-compatibility -->"
END = "<!-- END GENERATED: graphics-compatibility -->"


class ClaimError(ValueError):
    """The registry asks for a claim its own contents contradict."""


def validate(registry: dict) -> None:
    """Refuse a registry whose conformance claim outruns its feature list."""
    relaxed = registry.get("relaxed_features", [])
    conformance = registry.get("conformance", {})

    unprovided = [f for f in relaxed if not f.get("provided", False)]
    if conformance.get("full_fl11_0") and unprovided:
        names = ", ".join(f["feature"] for f in unprovided)
        raise ClaimError(
            "cannot claim full FL11_0 conformance while these features are "
            f"relaxed: {names}"
        )

    claim = conformance.get("claim", "")
    if unprovided and "experimental" not in claim.lower():
        raise ClaimError(
            f"claim {claim!r} does not say the subset is experimental, but "
            f"{len(unprovided)} feature(s) are relaxed"
        )

    for title in registry.get("titles", []):
        # A title cannot meet a gate it has no samples for. This is the shape
        # of mistake that turns "we did not measure" into "it passed".
        if title.get("meets_gate") and not title.get("samples"):
            raise ClaimError(
                f"title {title['title']!r} claims to meet its gate with no samples"
            )
        p50 = title.get("fps_p50")
        gate = title.get("fps_gate")
        if title.get("meets_gate") and p50 is not None and gate is not None and p50 < gate:
            raise ClaimError(
                f"title {title['title']!r} claims to meet a {gate} FPS gate at {p50} FPS"
            )


def render(registry: dict) -> str:
    stack = registry["stack"]
    lines: list[str] = []

    lines.append(f"_Generated from `docs/windows-arm/graphics-compatibility.json`"
                 f" on {registry['updated']}. Do not edit this block._")
    lines.append("")
    lines.append("## Stack")
    lines.append("")
    lines.append("| Layer | Component |")
    lines.append("| --- | --- |")
    lines.append(f"| Guest API | {stack['guest_api']} |")
    lines.append(f"| Translation | {stack['translation']} |")
    lines.append(f"| Guest driver | {stack['guest_driver']} |")
    lines.append(f"| Host backend | {stack['host_backend']} |")
    lines.append("")
    lines.append("Driver provenance is keyed by DriverStore hash, because a rebuilt "
                 "package with the same file name is a different driver:")
    lines.append("")
    lines.append(f"- fixed: `{stack['driverstore_fixed']}`")
    lines.append(f"- shipped 120.41: `{stack['driverstore_shipped_12041']}`")
    lines.append("")

    conformance = registry["conformance"]
    lines.append("## Conformance")
    lines.append("")
    lines.append(f"**{conformance['claim']}.** {conformance['rationale']}")
    lines.append("")

    lines.append("## Relaxed features")
    lines.append("")
    lines.append("DXVK requests these for feature level 11_0. They are not provided, "
                 "and each has a guest-visible consequence:")
    lines.append("")
    lines.append("| Feature | Layer | Consequence |")
    lines.append("| --- | --- | --- |")
    for feature in registry["relaxed_features"]:
        lines.append(
            f"| `{feature['feature']}` | {feature['layer']} | {feature['consequence']} |"
        )
    lines.append("")

    lines.append("## Titles")
    lines.append("")
    lines.append("| Title | API | Renders | Present mode | p50 FPS | Gate | Meets gate | Samples |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- | --- |")
    for title in registry["titles"]:
        p50 = "not measured" if title["fps_p50"] is None else f"{title['fps_p50']:.1f}"
        lines.append(
            f"| {title['title']} | {title['api']} | {'yes' if title['renders'] else 'no'} "
            f"| {title['present_mode']} | {p50} | {title['fps_gate']:.0f} "
            f"| {'yes' if title['meets_gate'] else '**no**'} | {title['samples']} |"
        )
    lines.append("")
    for title in registry["titles"]:
        if title.get("notes"):
            lines.append(f"- **{title['title']}**: {title['notes']}")
    lines.append("")
    return "\n".join(lines)


def apply(document: Path, body: str) -> str:
    text = document.read_text() if document.exists() else ""
    block = f"{BEGIN}\n{body}\n{END}"
    pattern = re.compile(re.escape(BEGIN) + r"\n(?:.*?\n)?" + re.escape(END), re.S)
    if pattern.search(text):
        return pattern.sub(lambda _: block, text)
    header = (
        "# Windows ARM graphics compatibility\n\n"
        "What the D3D11 and Vulkan paths actually do today, generated from a\n"
        "registry so the table cannot drift from the code.\n\n"
    )
    return header + block + "\n"


def _self_test() -> int:
    checks = 0

    def expect_error(registry: dict, fragment: str, description: str) -> None:
        nonlocal checks
        checks += 1
        try:
            validate(registry)
        except ClaimError as error:
            if fragment not in str(error):
                print(f"FAIL: {description}: wrong message {error}", file=sys.stderr)
                raise SystemExit(1)
        else:
            print(f"FAIL: {description}", file=sys.stderr)
            raise SystemExit(1)

    relaxed = [{"feature": "geometryShader", "provided": False}]

    expect_error(
        {"relaxed_features": relaxed,
         "conformance": {"claim": "Full D3D11", "full_fl11_0": True}},
        "cannot claim full FL11_0",
        "a full-FL11_0 claim with a relaxed feature must be refused",
    )
    expect_error(
        {"relaxed_features": relaxed,
         "conformance": {"claim": "D3D11 compatible", "full_fl11_0": False}},
        "does not say the subset is experimental",
        "a confident claim with a relaxed feature must be refused",
    )
    expect_error(
        {"relaxed_features": [], "conformance": {"claim": "x", "full_fl11_0": True},
         "titles": [{"title": "T", "meets_gate": True, "samples": 0}]},
        "no samples",
        "meeting a gate with no samples must be refused",
    )
    expect_error(
        {"relaxed_features": [], "conformance": {"claim": "x", "full_fl11_0": True},
         "titles": [{"title": "T", "meets_gate": True, "samples": 10,
                     "fps_p50": 20.0, "fps_gate": 30.0}]},
        "claims to meet a 30.0 FPS gate at 20.0",
        "meeting a gate below its threshold must be refused",
    )

    # The honest shape passes.
    checks += 1
    validate({
        "relaxed_features": relaxed,
        "conformance": {"claim": "Experimental D3D11-compatible subset",
                        "full_fl11_0": False},
        "titles": [{"title": "T", "meets_gate": False, "samples": 0,
                    "fps_p50": None, "fps_gate": 30.0}],
    })

    # A full claim is allowed once nothing is relaxed.
    checks += 1
    validate({"relaxed_features": [], "conformance": {"claim": "Full", "full_fl11_0": True}})

    print(f"PASS: render-graphics-compatibility self-test ({checks} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return _self_test()

    registry = json.loads(REGISTRY.read_text())
    try:
        validate(registry)
    except ClaimError as error:
        print(f"graphics compatibility: FAIL ({error})", file=sys.stderr)
        return 1

    rendered = apply(DOCUMENT, render(registry))
    if args.check:
        current = DOCUMENT.read_text() if DOCUMENT.exists() else ""
        if current != rendered:
            print("graphics compatibility: FAIL (generated block is out of date)",
                  file=sys.stderr)
            return 1
        print("graphics compatibility: PASS")
        return 0

    DOCUMENT.parent.mkdir(parents=True, exist_ok=True)
    DOCUMENT.write_text(rendered)
    print(f"graphics compatibility: PASS (rendered {len(registry['titles'])} titles, "
          f"{len(registry['relaxed_features'])} relaxed features)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
