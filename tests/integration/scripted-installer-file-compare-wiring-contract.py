#!/usr/bin/env python3
"""Prove the scripted WinPE installer bundles and guards its byte comparator."""
from pathlib import Path

root = Path(__file__).resolve().parents[2]
guest = (root / "scripts/win-assets/bvinstall.cmd").read_text()
builder = (root / "scripts/build-hvf-windows-scripted-source.sh").read_text()

assert "fc /b" not in guest.lower()
call = (
    "bv-file-compare.exe %PROVISION%\\payload-receipt.tsv "
    "W:\\BridgeVM\\provisioning\\payload-receipt.tsv >nul"
)
assert guest.count(call) == 1
call_index = guest.index(call)
guard = (
    "\nif errorlevel 1 (\n"
    "  echo BVINSTALL ERROR: guest provisioning receipt copy mismatch\n"
    "  goto :end\n)"
)
assert guest[call_index + len(call):].startswith(guard)
assert call_index < guest.index("echo BVINSTALL BCDBOOT")
for required in (
    'zig cc -target aarch64-windows-gnu',
    '"$ASSETS/bv-file-compare.c" -o "$COMPARE_EXE"',
    'add "$COMPARE_EXE" /Windows/System32/bv-file-compare.exe',
    "for f in winpeshl.ini bvinstall.cmd bvdiskpart.txt bv-file-compare.exe",
):
    assert required in builder
print("PASS: scripted installer uses its bundled fail-closed byte comparator")
