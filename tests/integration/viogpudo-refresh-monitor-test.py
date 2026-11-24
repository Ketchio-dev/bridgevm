#!/usr/bin/env python3
"""Compile the patched driver's real monitor enumeration against WDDM callbacks."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

source = Path(sys.argv[1]).read_text()
rates_start = source.index("static const ULONG g_BridgeRefreshRates[] =")
rate_count_start = source.index("static const UINT g_BridgeRefreshRateCount =", rates_start)
rates_end = source.index(";", rate_count_start) + 1


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
        source[rates_start:rates_end] + "\n" +
        body("AddSingleMonitorMode") + "\n" + body("AddSingleTargetMode"))
    fixture = Path(__file__).with_suffix(".cpp")
    binary = work / "monitor-test"
    subprocess.run([os.environ.get("CXX", "c++"), "-std=c++17", "-Wall", "-Wextra", "-Werror",
                    "-I", str(work), str(fixture), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
print("PASS: existing monitor modes retain every newly advertised timing; callback errors survive")
