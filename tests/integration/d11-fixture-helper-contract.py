"""Inspect generated private XML in memory; never emit credential values."""
import os
from pathlib import Path
import stat
import subprocess
import sys
import xml.etree.ElementTree as ET

from d11_fixture_test_support import ROOT
from d11_fixture_executables import dependencies, system

helper, root = Path(sys.argv[1]), Path(sys.argv[2])
links = dependencies(subprocess.check_output(["/usr/bin/otool", "-L", str(helper)]))
if any(not system(path) for path in links): raise SystemExit("FAIL: helper has an ambient dependency")
nonce = "1" * 64
outputs = [root / "a.xml", root / "b.xml"]
for path in outputs:
    subprocess.run([str(helper), "unattend", str(path), nonce], check=True, stdout=subprocess.DEVNULL)
    if stat.S_IMODE(path.stat().st_mode) != 0o600: raise SystemExit("FAIL: private XML permissions")
    raw = path.read_bytes()
    if b"\n" in raw.replace(b"\r\n", b""): raise SystemExit("FAIL: private XML line endings")
values = []
for path in outputs:
    element = ET.fromstring(path.read_bytes())
    ns = {"u": "urn:schemas-microsoft-com:unattend"}
    values.append(element.find(".//u:LocalAccount/u:Password/u:Value", ns).text)
    if element.find(".//u:LocalAccount/u:Name", ns).text != "BVT17" + nonce[:12]:
        raise SystemExit("FAIL: fixture account binding")
if values[0] == values[1] or any(len(v) < 40 for v in values): raise SystemExit("FAIL: independent private credentials")
before = outputs[0].read_bytes()
result = subprocess.run([str(helper), "unattend", str(outputs[0]), nonce], capture_output=True)
if result.returncode == 0 or outputs[0].read_bytes() != before: raise SystemExit("FAIL: output overwrite")
invalid = root / "invalid.xml"
result = subprocess.run([str(helper), "unattend", str(invalid), "invalid"], capture_output=True)
if result.returncode == 0 or invalid.exists(): raise SystemExit("FAIL: invalid nonce accepted")
if any(v.encode() in result.stdout + result.stderr for v in values): raise SystemExit("FAIL: credential in diagnostics")
print("PASS: D11 helper private generation, binding and refusal")
