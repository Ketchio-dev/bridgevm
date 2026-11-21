#!/usr/bin/env python3
"""Compile the patched driver's real monitor/target enumeration against WDDM callbacks."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

source = Path(sys.argv[1]).read_text()


def body(name):
    start = source.index("NTSTATUS VioGpuDod::" + name + "(")
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


with tempfile.TemporaryDirectory(prefix="bridgevm-monitor-") as temporary:
    work = Path(temporary)
    (work / "monitor-body.inc").write_text(
        body("AddSingleMonitorMode") + "\n" + body("AddSingleTargetMode"))
    fixture = Path(__file__).with_suffix(".cpp")
    binary = work / "monitor-test"
    subprocess.run([os.environ.get("CXX", "c++"), "-std=c++17", "-Wall", "-Wextra", "-Werror",
                    "-I", str(work), str(fixture), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
print("PASS: per-rate targets are independent of the source size, each has a matching monitor mode, "
      "120Hz is preferred; callback errors survive")
