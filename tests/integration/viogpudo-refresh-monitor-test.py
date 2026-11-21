#!/usr/bin/env python3
"""Compile the patched driver's real monitor enumeration against WDDM callbacks."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

source = Path(sys.argv[1]).read_text()


def body(signature):
    start = source.index(signature + "(")
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


with tempfile.TemporaryDirectory(prefix="bridgevm-monitor-") as temporary:
    work = Path(temporary)
    (work / "monitor-body.inc").write_text("\n".join(body(signature) for signature in (
        "VOID VioGpuDod::BuildVideoSignalInfo",
        "NTSTATUS VioGpuDod::AddSingleMonitorMode",
        "NTSTATUS VioGpuDod::AddSingleTargetMode")))
    fixture = Path(__file__).with_suffix(".cpp")
    binary = work / "monitor-test"
    subprocess.run([os.environ.get("CXX", "c++"), "-std=c++17", "-Wall", "-Wextra", "-Werror",
                    "-I", str(work), str(fixture), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
print("PASS: existing monitor modes retain every newly advertised timing; exactly one 120Hz mode is "
      "preferred per set; callback errors survive")
