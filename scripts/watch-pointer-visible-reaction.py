#!/usr/bin/env python3
"""Inject one B4 press/release and timestamp active-scanout PPM change."""
import argparse
import hashlib
from pathlib import Path
import shutil
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument("--ppm", required=True, type=Path)
parser.add_argument("--input-control", required=True, type=Path)
parser.add_argument("--out", required=True, type=Path)
parser.add_argument("--timeout-ms", type=int, default=1000)
parser.add_argument("--hold-ms", type=int, default=200)
args = parser.parse_args()

def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

baseline = args.ppm.read_bytes()
baseline_hash = digest(baseline)
args.out.mkdir(parents=True, exist_ok=True)
shutil.copyfile(args.ppm, args.out / "active-scanout-before.ppm")
initial_mtime = args.ppm.stat().st_mtime_ns
started = time.monotonic_ns()
with args.input_control.open("ab", buffering=0) as control:
    control.write(b"POINTER press:16384x16384\n")

def release() -> None:
    deadline = started + args.hold_ms * 1_000_000
    remaining = deadline - time.monotonic_ns()
    if remaining > 0:
        time.sleep(remaining / 1_000_000_000)
    with args.input_control.open("ab", buffering=0) as control:
        control.write(b"POINTER release:16384x16384\n")

release_thread = threading.Thread(target=release)
release_thread.start()
changed_hash = "none"
first_ms = "none"
deadline = started + args.timeout_ms * 1_000_000
while time.monotonic_ns() <= deadline:
    try:
        if args.ppm.stat().st_mtime_ns != initial_mtime:
            frame = args.ppm.read_bytes()
            candidate = digest(frame)
            if candidate != baseline_hash:
                first_ms = str((time.monotonic_ns() - started + 999_999) // 1_000_000)
                changed_hash = candidate
                (args.out / "active-scanout-changed.ppm").write_bytes(frame)
                break
    except FileNotFoundError:
        pass
    time.sleep(0.005)
release_thread.join()
(args.out / "visible.env").write_text(
    "source=active-virtio-gpu-scanout\n"
    f"baseline_sha256={baseline_hash}\nchanged_sha256={changed_hash}\n"
    f"first_changed_ms={first_ms}\n",
    encoding="ascii",
)
raise SystemExit(0 if first_ms != "none" else 1)
