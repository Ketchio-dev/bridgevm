#!/usr/bin/env python3
"""Adversarial deterministic coverage for the Windows guest payload boundary."""
from __future__ import annotations

import hashlib
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from windows_catalog_test_support import build_catalog_verifier, invoke_stage


ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fixtures/make-synthetic-windows-guest-payload.py"
ASSETS = ROOT / "scripts/win-assets"
def invoke(
    payload: Path, manifest: Path, output: Path, verifier: Path
) -> subprocess.CompletedProcess[str]:
    return invoke_stage(payload, manifest, ASSETS, output, verifier)


def require_block(result: subprocess.CompletedProcess[str], code: str) -> None:
    assert result.returncode != 0, result.stdout
    assert f"BLOCKER[{code}]" in result.stderr, result.stderr


def replace_hash(manifest: Path, relative: str, path: Path) -> None:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    lines = manifest.read_text().splitlines()
    prefix = f"file\t{relative}\t"
    manifest.write_text("\n".join(
        prefix + digest if line.startswith(prefix) else line for line in lines
    ) + "\n")


def main() -> int:
    for name in (
        "bvagent.ps1", "bvagent-firstboot.ps1", "bvinstall.cmd", "bvdiskpart.txt",
        "winpeshl.ini", "unattend.xml", "windows-guest-payload-v1.example.tsv",
    ):
        data = (ASSETS / name).read_bytes()
        assert data.count(b"\n") == data.count(b"\r\n"), f"win-assets file is not CRLF: {name}"
    install = (ASSETS / "bvinstall.cmd").read_text()
    for contract in (
        "required signed ARM64 guest payload missing", "/add-driver",
        "roles=storage,serial,network", "bvagent-firstboot.ps1",
        "live-bind-not-yet-proven", "first-logon-ready-not-yet-proven",
    ):
        assert contract in install, contract
    assert "/forceunsigned" not in install.lower()
    with tempfile.TemporaryDirectory(prefix="bridgevm-guest-payload-test.") as temporary:
        root = Path(temporary)
        payload = root / "payload"
        manifest = root / "manifest.tsv"
        verifier = root / "bridgevm-catalog-verify"
        build_catalog_verifier(verifier)
        subprocess.run([str(FIXTURE), str(payload), str(manifest)], check=True)

        good = invoke(payload, manifest, root / "staged", verifier)
        assert good.returncode == 0, good.stderr
        receipt = (root / "staged/payload-receipt.tsv").read_text()
        for role in ("storage", "serial", "network"):
            assert f"driver\t{role}\t" in receipt
        assert (root / "staged/agent/bvagent.ps1").is_file()
        assert (root / "staged/agent/bvagent-firstboot.ps1").is_file()

        storage = payload / "storage/storage.sys"
        original = storage.read_bytes()
        storage.write_bytes(original + b"mutation")
        require_block(invoke(payload, manifest, root / "mutated", verifier), "guest-payload-hash")
        storage.write_bytes(original)

        altered = bytearray(original)
        struct.pack_into("<H", altered, 0x84, 0x8664)
        storage.write_bytes(altered)
        replace_hash(manifest, "storage/storage.sys", storage)
        require_block(invoke(payload, manifest, root / "x64", verifier), "guest-payload-architecture")
        storage.write_bytes(original)
        replace_hash(manifest, "storage/storage.sys", storage)

        catalog = payload / "serial/serial.cat"
        signed_catalog = catalog.read_bytes()
        content_offset = signed_catalog.index(b"synthetic BridgeVM catalog fixture")
        catalog.write_bytes(signed_catalog[:content_offset] + b"X" + signed_catalog[content_offset + 1:])
        replace_hash(manifest, "serial/serial.cat", catalog)
        require_block(invoke(payload, manifest, root / "tampered-catalog", verifier), "guest-payload-catalog-signature")
        catalog.write_bytes(b"unsigned catalog")
        replace_hash(manifest, "serial/serial.cat", catalog)
        require_block(invoke(payload, manifest, root / "unsigned", verifier), "guest-payload-catalog-signature")
        catalog.write_bytes(signed_catalog)
        replace_hash(manifest, "serial/serial.cat", catalog)

        extra = payload / "network/unmanifested.dll"
        extra.write_bytes(original)
        require_block(invoke(payload, manifest, root / "extra", verifier), "guest-payload-file-set")
        extra.unlink()

        missing_role = root / "missing-role.tsv"
        missing_role.write_text("\n".join(
            line for line in manifest.read_text().splitlines()
            if not line.startswith("driver\tnetwork\t")) + "\n")
        require_block(invoke(payload, missing_role, root / "missing-role", verifier), "guest-payload-roles")

        missing = invoke(root / "absent", manifest, root / "missing", verifier)
        require_block(missing, "guest-payload-missing")

        iso = root / "windows.iso"
        iso.write_bytes(b"synthetic ISO")
        source_output = root / "must-not-exist.raw"
        build_environment = {
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "ISO": str(iso),
            "ASSETS": str(ASSETS), "OUT": str(source_output), "WIMLIB": "/usr/bin/true",
            "WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER": str(verifier),
        }
        source = subprocess.run(
            [str(ROOT / "scripts/build-hvf-windows-scripted-source.sh")],
            env=build_environment, capture_output=True, text=True,
        )
        require_block(source, "guest-payload-missing")
        assert not source_output.exists(), "source output was mutated before payload preflight"

    print("PASS: Windows guest payload verifier rejects missing, mutated, unsigned and wrong-architecture inputs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
