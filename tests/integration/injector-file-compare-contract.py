#!/usr/bin/env python3
"""Native binary comparator contract; not proof of execution inside WinPE."""
import os, runpy
from pathlib import Path
import shlex
import subprocess
import tempfile
root = Path(__file__).resolve().parents[2]
runpy.run_path(str(root / "tests/integration/injector-file-compare-wiring-contract.py"))
with tempfile.TemporaryDirectory(prefix="bridgevm-file-compare-") as directory:
    work = Path(directory)
    exe = work / ("compare.exe" if os.name == "nt" else "compare")
    subprocess.run(shlex.split(os.environ.get("CC", "cc")) + [
        "-std=c11", "-Wall", "-Wextra", "-Werror",
        str(root / "scripts/win-assets/bv-file-compare.c"), "-o", str(exe),
    ], check=True, timeout=60)
    a, b = work / "left.bin", work / "right.bin"
    cases = 0
    def expect(paths, expected):
        global cases
        result = subprocess.run([str(exe), *map(str, paths)], capture_output=True, timeout=10)
        assert result.returncode == expected, (paths, expected, result.returncode)
        cases += 1
    for size in (0, 1, 65535, 65536, 65537, 131072):
        payload = (bytes(range(256)) * ((size + 255) // 256))[:size]
        a.write_bytes(payload)
        b.write_bytes(payload)
        expect([a, b], 0)
        if size:
            b.write_bytes(payload[:-1] + bytes([payload[-1] ^ 1]))
            expect([a, b], 1)
        b.write_bytes(payload + b"x")
        expect([a, b], 1)
        expect([b, a], 1)
    expect([a, a], 0)
    expect([a, work / "missing"], 2)
    expect([work / "missing", b], 2)
    expect([], 2)
    print(f"PASS: {cases} native binary comparison cases; WinPE execution unproven")
