#!/usr/bin/env python3
"""Inject one B4 press/release and timestamp active FbSink pixel change."""
import argparse, hashlib, struct, threading, time
from pathlib import Path

parser = argparse.ArgumentParser()
for option in ("fb", "input-control", "out"):
    parser.add_argument(f"--{option}", required=True, type=Path)
parser.add_argument("--timeout-ms", type=int, default=1000)
parser.add_argument("--hold-ms", type=int, default=200)
args = parser.parse_args()
HEADER = struct.Struct("<6IQ"); MAGIC = 0x42564642
def frame(with_full: bool = False) -> tuple[bytes, bytes, int, int]:
    for _ in range(100):
        with args.fb.open("rb", buffering=0) as fb:
            header = fb.read(HEADER.size)
            if len(header) != HEADER.size: time.sleep(0.005); continue
            magic, version, width, height, stride, _fourcc, seq1 = HEADER.unpack(header)
            if magic != MAGIC or version != 1 or width != 1600 or height != 900 or seq1 & 1:
                time.sleep(0.005); continue
            x0, y0, side = width // 2 - 64, height // 2 - 64, 128
            region = bytearray()
            for y in range(y0, y0 + side):
                fb.seek(64 + y * stride + x0 * 4); region += fb.read(side * 4)
            full = b""
            if with_full: fb.seek(64); full = fb.read(height * stride)
            fb.seek(24); seq2 = struct.unpack("<Q", fb.read(8))[0]
            if seq1 == seq2 and seq2 & 1 == 0 and (not with_full or len(full) == height * stride):
                return bytes(region), full, width, height
        time.sleep(0.005)
    raise RuntimeError("no stable active framebuffer")

before_region, before_frame, width, height = frame(True)
baseline_hash = hashlib.sha256(before_region).hexdigest()
args.out.mkdir(parents=True, exist_ok=True)
(args.out / "active-scanout-before.xrgb8888").write_bytes(before_frame)
started = time.monotonic_ns()
with args.input_control.open("ab", buffering=0) as control: control.write(b"POINTER press:16384x16384\n")

def release() -> None:
    deadline = started + args.hold_ms * 1_000_000
    remaining = deadline - time.monotonic_ns()
    if remaining > 0: time.sleep(remaining / 1_000_000_000)
    with args.input_control.open("ab", buffering=0) as control: control.write(b"POINTER release:16384x16384\n")

thread = threading.Thread(target=release); thread.start()
changed_hash = "none"; first_ms = "none"; changed_frame = b""
deadline = started + args.timeout_ms * 1_000_000
while time.monotonic_ns() <= deadline:
    region, _, _, _ = frame(); candidate_hash = hashlib.sha256(region).hexdigest()
    if candidate_hash != baseline_hash:
        first_ms = str((time.monotonic_ns() - started + 999_999) // 1_000_000)
        changed_hash = candidate_hash; _, changed_frame, _, _ = frame(True); break
    time.sleep(0.005)
thread.join()
if changed_frame: (args.out / "active-scanout-changed.xrgb8888").write_bytes(changed_frame)
(args.out / "visible.env").write_text(
    f"source=active-virtio-gpu-framebuffer\nwidth={width}\nheight={height}\n"
    f"baseline_region_sha256={baseline_hash}\nchanged_region_sha256={changed_hash}\n"
    f"first_changed_ms={first_ms}\n", encoding="ascii")
raise SystemExit(0 if first_ms != "none" else 1)
