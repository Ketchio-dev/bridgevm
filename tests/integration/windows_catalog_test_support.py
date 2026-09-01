"""Shared OpenSSL-backed Windows catalog test helpers."""
from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
STAGE = ROOT / "scripts/stage-hvf-windows-guest-payload.sh"


def build_catalog_verifier(output: Path) -> None:
    prefix = next(path for path in (
        Path("/opt/homebrew/opt/openssl@3"), Path("/usr/local/opt/openssl@3")
    ) if path.is_dir())
    subprocess.run([
        str(ROOT / "scripts/build-windows-catalog-verifier.sh"),
        "--output", str(output), "--openssl-prefix", str(prefix),
    ], check=True, stdout=subprocess.DEVNULL)


def invoke_stage(
    payload: Path, manifest: Path, assets: Path, output: Path, verifier: Path
) -> subprocess.CompletedProcess[str]:
    return subprocess.run([
        str(STAGE), "--payload-dir", str(payload), "--manifest", str(manifest),
        "--assets", str(assets), "--output", str(output),
        "--catalog-verifier", str(verifier),
    ], capture_output=True, text=True)
