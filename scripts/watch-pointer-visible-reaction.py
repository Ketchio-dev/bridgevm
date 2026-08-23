#!/usr/bin/env python3
"""Inject B4 press/release and timestamp the real CGL IOSurface change."""
import argparse, ctypes, hashlib, struct, time
from pathlib import Path
parser = argparse.ArgumentParser()
for option in ("iosurface", "input-control", "out"):
    parser.add_argument(f"--{option}", required=True, type=Path)
parser.add_argument("--timeout-ms", type=int, default=1000); parser.add_argument("--hold-ms", type=int, default=200)
parser.add_argument("--hid-x", required=True, type=int); parser.add_argument("--hid-y", required=True, type=int)
args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
lib = ctypes.CDLL("/System/Library/Frameworks/IOSurface.framework/IOSurface")
lib.IOSurfaceLookup.argtypes = [ctypes.c_uint32]; lib.IOSurfaceLookup.restype = ctypes.c_void_p
lib.IOSurfaceLock.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32)]
lib.IOSurfaceUnlock.argtypes = lib.IOSurfaceLock.argtypes
for name in ("IOSurfaceGetBaseAddress",):
    getattr(lib, name).argtypes = [ctypes.c_void_p]; getattr(lib, name).restype = ctypes.c_void_p
for name in ("IOSurfaceGetBytesPerRow", "IOSurfaceGetWidth", "IOSurfaceGetHeight"):
    getattr(lib, name).argtypes = [ctypes.c_void_p]; getattr(lib, name).restype = ctypes.c_size_t
lib.IOSurfaceGetSeed.argtypes = [ctypes.c_void_p]; lib.IOSurfaceGetSeed.restype = ctypes.c_uint32
def surface():
    ident, width, height = map(int, args.iosurface.read_text().split())
    ref = lib.IOSurfaceLookup(ident)
    if not ref or (width, height) != (1600, 900): raise RuntimeError("invalid active IOSurface")
    return ref, ident
def frame(ref, full=False):
    seed = ctypes.c_uint32()
    if lib.IOSurfaceLock(ref, 1, ctypes.byref(seed)): raise RuntimeError("IOSurface lock failed")
    try:
        width, height = lib.IOSurfaceGetWidth(ref), lib.IOSurfaceGetHeight(ref)
        stride, base = lib.IOSurfaceGetBytesPerRow(ref), lib.IOSurfaceGetBaseAddress(ref)
        if (width, height) != (1600, 900) or not base: raise RuntimeError("bad IOSurface geometry")
        x0, y0, side = width // 2 - 64, height // 2 - 64, 128
        region = b"".join(ctypes.string_at(base + y * stride + x0 * 4, side * 4) for y in range(y0, y0 + side))
        pixels = ctypes.string_at(base, height * stride) if full else b""
        return region, pixels, seed.value
    finally: lib.IOSurfaceUnlock(ref, 1, ctypes.byref(seed))

ref, ident = surface(); baseline = ""; deadline = time.monotonic_ns() + 120_000_000_000
while time.monotonic_ns() <= deadline:
    before, _, _ = frame(ref); candidate = hashlib.sha256(before).hexdigest()
    fill = sum(before[i:i + 4] == b"\xff\xff\xff\xff" for i in range(0, len(before), 4))
    if fill >= 10_000 and candidate == baseline: break
    baseline = candidate; time.sleep(0.025)
else: raise RuntimeError("active target baseline not presented")
before, before_frame, seed = frame(ref, True); baseline = hashlib.sha256(before).hexdigest(); (args.out / "iosurface-before.xrgb8888").write_bytes(before_frame)
started = time.monotonic_ns(); start_unix = time.time_ns()
with args.input_control.open("a", buffering=1) as ctl: ctl.write(f"POINTER click:{args.hid_x}x{args.hid_y}\n")
press_unix = time.time_ns()
changed = "none"; first = "none"; changed_frame = b""; change_unix = 0; deadline = started + args.timeout_ms * 1_000_000
while time.monotonic_ns() <= deadline:
    observed_seed = lib.IOSurfaceGetSeed(ref)
    if observed_seed != seed:
        region, _, seed = frame(ref); candidate = hashlib.sha256(region).hexdigest()
        if candidate != baseline:
            first = str((time.monotonic_ns() - started + 999_999) // 1_000_000); change_unix = time.time_ns()
            changed = candidate; _, changed_frame, _ = frame(ref, True); break
    time.sleep(0.001)
if changed_frame: (args.out / "iosurface-changed.xrgb8888").write_bytes(changed_frame)
(args.out / "visible.env").write_text(f"source=active-cgl-iosurface\niosurface_id={ident}\nwidth=1600\nheight=900\nhid_x={args.hid_x}\nhid_y={args.hid_y}\nstart_unix_ns={start_unix}\npress_unix_ns={press_unix}\nchange_unix_ns={change_unix}\nbaseline_region_sha256={baseline}\nchanged_region_sha256={changed}\nfirst_changed_ms={first}\n", encoding="ascii")
raise SystemExit(0 if first != "none" else 1)
