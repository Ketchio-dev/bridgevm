#!/usr/bin/env python3
"""Prove the packaged WinPE installer owns and guards its byte comparator."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
read = lambda path: (root / path).read_text()
guest = read("scripts/win-assets/bvinstall.cmd")
builder = read("scripts/build-hvf-windows-scripted-source.sh")
package = read("apps/macos/scripts/package-hvf-control-app.sh")
plan = read("apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstall.swift")
cache = read("apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallCacheIdentity.swift")
compiler = read("scripts/build-winpe-file-compare.sh")
assert "fc /b" not in guest.lower()
call = "bv-file-compare.exe %PROVISION%\\payload-receipt.tsv W:\\BridgeVM\\provisioning\\payload-receipt.tsv >nul"
assert guest.count(call) == 1
index = guest.index(call)
guard = "\nif errorlevel 1 (\n  echo BVINSTALL ERROR: guest provisioning receipt copy mismatch\n  goto :end\n)"
assert guest[index + len(call):].startswith(guard)
assert index < guest.index("echo BVINSTALL BCDBOOT")
assert "zig cc" not in builder
for required in ('FILE_COMPARE="${WINDOWS_FILE_COMPARE:-}"', 'add "$FILE_COMPARE" /Windows/System32/bv-file-compare.exe',
                 "for f in winpeshl.ini bvinstall.cmd bvdiskpart.txt bv-file-compare.exe"):
    assert required in builder
assert 'build-winpe-file-compare.sh" "$stage_app/Contents/Resources/helpers/bv-file-compare.exe"' in package
assert '"helpers/bv-file-compare.exe"' in plan and '"WINDOWS_FILE_COMPARE": fileComparePath' in plan
assert cache.count('"helpers/bv-file-compare.exe"') == 1
assert "zig cc -target aarch64-windows-gnu" in compiler
print("PASS: packaged installer owns a fail-closed ARM64 byte comparator")
