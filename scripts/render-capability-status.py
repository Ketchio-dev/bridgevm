#!/usr/bin/env python3
"""Validate the capability registry and keep generated claim blocks in sync.

The registry at ``capabilities/windows-hvf.json`` owns capability state and the
exact user-facing wording. Human-written prose stays untouched: this script only
rewrites explicitly marked managed blocks, so historical evidence and analysis
can never be overwritten by generation.

    python3 scripts/render-capability-status.py            # write
    python3 scripts/render-capability-status.py --check    # fail on drift
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "capabilities" / "windows-hvf.json"
SCHEMA = ROOT / "schemas" / "bridgevm-capability-v1.json"

BEGIN = "<!-- BEGIN GENERATED: {name} -->"
END = "<!-- END GENERATED: {name} -->"

# Claims that must never reappear once the registry owns the wording.
FORBIDDEN_CLAIMS = (
    ("Windows is not bootable yet", "contradicts the proven installed-desktop boot"),
    ("Windows beta", "product state is Engineering Preview, not beta"),
    ("bit-identical", "the guest contract has documented deviations"),
    ("Rust 1.76", "workspace MSRV is 1.85"),
)

STATE_MARK = {"PROVEN": "proven", "OPEN": "open", "EXTERNAL": "external"}


def fail(message: str) -> None:
    print(f"capability registry: {message}", file=sys.stderr)
    sys.exit(1)


def load_registry() -> dict:
    if not REGISTRY.exists():
        fail(f"missing registry: {REGISTRY}")
    try:
        return json.loads(REGISTRY.read_text())
    except json.JSONDecodeError as exc:
        fail(f"registry is not valid JSON: {exc}")
    raise AssertionError("unreachable")


def validate(registry: dict) -> None:
    """Check the registry against the parts of the schema we depend on.

    A full JSON Schema validator is not a workspace dependency, so this asserts
    the required fields, enums and cross-field rules the repository relies on.
    """
    if not SCHEMA.exists():
        fail(f"missing schema: {SCHEMA}")
    schema = json.loads(SCHEMA.read_text())

    top_required = schema["required"]
    for key in top_required:
        if key not in registry:
            fail(f"registry is missing required field: {key}")

    if registry["schema_version"] != 1:
        fail("unsupported schema_version")

    if not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", registry["reviewed"]):
        fail("reviewed must be an ISO date")

    if not re.fullmatch(r"[0-9a-f]{7,40}", registry["tested_commit"]):
        fail("tested_commit must be a hex commit id")

    product_states = schema["properties"]["product_state"]["enum"]
    if registry["product_state"] not in product_states:
        fail(f"product_state must be one of {product_states}")

    wording_required = schema["properties"]["wording"]["required"]
    for key in wording_required:
        if not registry.get("wording", {}).get(key):
            fail(f"wording.{key} is required and must be non-empty")

    criterion_schema = schema["definitions"]["criterion"]
    states = criterion_schema["properties"]["state"]["enum"]
    seen: set[str] = set()
    for criterion in registry["criteria"]:
        for key in criterion_schema["required"]:
            if key not in criterion:
                fail(f"criterion {criterion.get('id', '?')} is missing {key}")
        cid = criterion["id"]
        if not re.fullmatch(r"[AB][0-9]{1,2}", cid):
            fail(f"invalid criterion id: {cid}")
        if cid in seen:
            fail(f"duplicate criterion id: {cid}")
        seen.add(cid)
        if criterion["state"] not in states:
            fail(f"criterion {cid} has invalid state {criterion['state']}")
        if criterion["state"] == "OPEN" and not criterion.get("measured"):
            fail(f"criterion {cid} is OPEN and must record its latest measurement")
        for path in criterion["evidence_paths"]:
            if not (ROOT / path).exists():
                fail(f"criterion {cid} references a missing evidence path: {path}")

    # The central consistency rule: a promoted product state requires proof.
    if registry["product_state"] != "ENGINEERING_PREVIEW":
        unproven = [
            c["id"]
            for c in registry["criteria"]
            if c["release_blocking"] and c["state"] != "PROVEN"
        ]
        if unproven:
            fail(
                "product_state may not be promoted while release-blocking "
                f"criteria are unproven: {', '.join(unproven)}"
            )


def render_summary(registry: dict) -> str:
    wording = registry["wording"]
    criteria = registry["criteria"]
    blocking = [c for c in criteria if c["release_blocking"]]
    proven = [c for c in blocking if c["state"] == "PROVEN"]
    open_ids = [c["id"] for c in blocking if c["state"] != "PROVEN"]

    lines = [
        f"**Product state: {wording['product_state_label']}.** "
        f"{wording['engine_summary']}",
        "",
        f"Release-blocking criteria proven: **{len(proven)} / {len(blocking)}**. "
        f"Open: {', '.join(open_ids) if open_ids else 'none'}.",
        "",
        f"- Graphics: {wording['graphics_vulkan']} and "
        f"{wording['graphics_d3d11']}.",
        f"- Guest platform: {wording['machine_contract']}.",
        "",
        f"State reviewed {registry['reviewed']} at commit "
        f"`{registry['tested_commit']}`. This block is generated from "
        "[`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by "
        "`scripts/render-capability-status.py`.",
    ]
    return "\n".join(lines)


def render_matrix(registry: dict) -> str:
    lines = [
        "| ID | Capability | State | Threshold | Latest measurement |",
        "| --- | --- | --- | --- | --- |",
    ]
    for criterion in registry["criteria"]:
        measured = criterion.get("measured") or "See evidence."
        state = STATE_MARK[criterion["state"]]
        blocking = "" if criterion["release_blocking"] else " (non-blocking)"
        lines.append(
            f"| {criterion['id']} | {criterion['title']}{blocking} | "
            f"`{state}` | {criterion['statement']} | {measured} |"
        )
    lines.append("")
    lines.append(
        "Generated from [`capabilities/windows-hvf.json`]"
        "(../capabilities/windows-hvf.json) by "
        "`scripts/render-capability-status.py`."
    )
    return "\n".join(lines)


def apply_block(path: Path, name: str, body: str, check: bool) -> bool:
    """Replace one managed block. Returns True when the file already matched."""
    if not path.exists():
        fail(f"managed block target is missing: {path}")
    text = path.read_text()
    begin = BEGIN.format(name=name)
    end = END.format(name=name)
    pattern = re.compile(
        re.escape(begin) + r"\n(?:.*?\n)?" + re.escape(end),
        re.DOTALL,
    )
    if not pattern.search(text):
        fail(f"{path.relative_to(ROOT)} has no managed block named {name}")
    replacement = f"{begin}\n{body}\n{end}"
    updated = pattern.sub(lambda _: replacement, text, count=1)
    if updated == text:
        return True
    if check:
        return False
    path.write_text(updated)
    return True


def check_forbidden() -> list[str]:
    """Scan claim surfaces for retracted wording."""
    targets = [
        ROOT / "README.md",
        ROOT / "STATUS.md",
        ROOT / "crates" / "bridgevm-core" / "src" / "lib.rs",
        ROOT / "crates" / "bridgevm-hvf" / "src" / "machine.rs",
        ROOT / "docs" / "hvf-windows-engine-strategy.md",
        ROOT / "apps" / "macos" / "Sources" / "BridgeVMApp" / "Services"
        / "VirtualMachineClient.swift",
    ]
    problems = []
    for target in targets:
        if not target.exists():
            continue
        text = target.read_text()
        for claim, reason in FORBIDDEN_CLAIMS:
            if claim in text:
                rel = target.relative_to(ROOT)
                problems.append(f"{rel}: retracted claim {claim!r} ({reason})")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of writing when a generated block is stale",
    )
    args = parser.parse_args()

    registry = load_registry()
    validate(registry)

    blocks = [
        (ROOT / "README.md", "capability-summary", render_summary(registry)),
        (ROOT / "STATUS.md", "capability-summary", render_summary(registry)),
        (
            ROOT / "docs" / "windows-arm" / "capability-matrix.md",
            "capability-matrix",
            render_matrix(registry),
        ),
    ]

    stale = [
        path.relative_to(ROOT)
        for path, name, body in blocks
        if not apply_block(path, name, body, args.check)
    ]

    problems = check_forbidden()

    if stale:
        for path in stale:
            print(
                f"capability drift: {path} generated block is out of date",
                file=sys.stderr,
            )
    for problem in problems:
        print(f"capability drift: {problem}", file=sys.stderr)

    if stale or problems:
        print(
            "capability registry: FAIL "
            "(run scripts/render-capability-status.py to regenerate)",
            file=sys.stderr,
        )
        return 1

    verb = "verified" if args.check else "rendered"
    print(
        f"capability registry: PASS ({verb} {len(blocks)} blocks, "
        f"{len(registry['criteria'])} criteria)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
