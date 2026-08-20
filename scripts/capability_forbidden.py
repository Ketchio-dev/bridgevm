"""Retracted-claim scanning for capability wording surfaces.

Once the registry owns user-facing wording, superseded claims must never
reappear in code or binding prose. This module owns both the static list and
the state-dependent list: 'the product is Engineering Preview' survived the
1.0 promotion in AGENTS.md precisely because the scan only knew strings that
were forbidden in every state.
"""

from __future__ import annotations

from pathlib import Path

# Claims forbidden regardless of product state.
FORBIDDEN_CLAIMS = (
    ("Windows is not bootable yet", "contradicts the proven installed-desktop boot"),
    ("Windows beta", "product wording comes from the registry; 'beta' is not a sanctioned label"),
    ("bit-identical", "the guest contract has documented deviations"),
    ("Rust 1.76", "workspace MSRV is 1.85"),
)

MATRIX_BLOCK_BEGIN = "<!-- BEGIN GENERATED: capability-matrix -->"


def stale_state_claims(registry: dict) -> tuple[tuple[str, str], ...]:
    """Forbidden wording that depends on the current product state."""
    if registry["product_state"] == "ENGINEERING_PREVIEW":
        return ()
    label = registry["wording"]["product_state_label"]
    return (
        (
            "product is **Engineering Preview**",
            f"product state is {label}; preview wording is retracted",
        ),
        (
            "promoted past Engineering Preview",
            "state-dependent rule wording; describe the promotion rule state-agnostically",
        ),
    )


def scan_targets(root: Path) -> list[Path]:
    """Whole trees, not a file list: a list stops covering what moves."""
    targets = [root / "README.md", root / "STATUS.md", root / "AGENTS.md"]
    for tree, suffixes in (
        (root / "crates", (".rs",)),
        (root / "runners", (".rs",)),
        (root / "tools", (".rs",)),
        (root / "apps" / "macos" / "Sources", (".swift",)),
    ):
        targets.extend(
            path
            for path in sorted(tree.rglob("*"))
            if path.suffix in suffixes and "/target/" not in str(path)
        )
    targets.append(root / "docs" / "hvf-windows-engine-strategy.md")
    return targets


def check_forbidden(registry: dict, root: Path) -> list[str]:
    """Scan claim surfaces for retracted wording."""
    claims = FORBIDDEN_CLAIMS + stale_state_claims(registry)
    problems = []
    for target in scan_targets(root):
        if not target.exists():
            continue
        text = target.read_text()
        for claim, reason in claims:
            if claim in text:
                rel = target.relative_to(root)
                problems.append(f"{rel}: retracted claim {claim!r} ({reason})")
    # The capability matrix's hand-written prose (above its generated block)
    # states the promotion rule; only that prose is scanned, because generated
    # measured entries legitimately quote historical retracted strings.
    matrix = root / "docs" / "windows-arm" / "capability-matrix.md"
    if matrix.exists():
        prose = matrix.read_text().split(MATRIX_BLOCK_BEGIN)[0]
        for claim, reason in claims:
            if claim in prose:
                problems.append(
                    f"{matrix.relative_to(root)}: retracted claim {claim!r} ({reason})"
                )
    return problems
