#!/usr/bin/env python3
"""Security and completion contracts for the packaged Windows installer."""
from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

from windows_catalog_test_support import build_catalog_verifier, invoke_stage

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "scripts/win-assets"

def main() -> int:
    unattended = (ASSETS / "unattend.xml").read_text()
    for forbidden in ("<AutoLogon>", "<UserAccounts>", "<PlainText>true</PlainText>",
                      "<LogonCount>", "<Value>bridge</Value>"):
        assert forbidden not in unattended, f"product unattend contains fixed credentials: {forbidden}"
    assert "bvagent-firstboot.ps1" in unattended

    install = (ASSETS / "bvinstall.cmd").read_text()
    marker = "> S:\\EFI\\BridgeVM\\install-success.txt echo bridgevm-windows-install-success-v1"
    assert install.count(marker) == 1
    assert install.index(marker) > install.index("bcdboot W:\\Windows /s S: /f UEFI")
    assert install.index(marker) > install.index("BVINSTALL ERROR: unattend copy failed")
    assert install.index(marker) < install.index("echo BVINSTALL DONE") < install.rindex("\n:end")

    runner = (ROOT / "scripts/run-hvf-windows-scripted-install.sh").read_text()
    assert "verify-hvf-windows-install-target.sh\" --target \"$TARGET\"" in runner
    verifier = (ROOT / "scripts/verify-hvf-windows-install-target.sh").read_text()
    for required in ("hdiutil attach -readonly", "bridgevm-windows-install-success-v1",
                     "payload-roles=storage,serial,network", "bcdboot=complete"):
        assert required in verifier

    with tempfile.TemporaryDirectory(prefix="bridgevm-special-payload.") as temporary:
        root = Path(temporary); payload = root / "payload"; manifest = root / "manifest.tsv"
        catalog_verifier = root / "bridgevm-catalog-verify"
        build_catalog_verifier(catalog_verifier)
        subprocess.run([str(ROOT / "tests/fixtures/make-synthetic-windows-guest-payload.py"),
                        str(payload), str(manifest)], check=True)
        fifo = payload / "network/unsupported.pipe"; os.mkfifo(fifo)
        result = invoke_stage(payload, manifest, ASSETS, root / "output", catalog_verifier)
        assert result.returncode != 0 and "BLOCKER[guest-payload-file-set]" in result.stderr
    print("PASS: Windows install credentials, completion marker, and payload entry types fail closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
