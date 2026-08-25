#!/usr/bin/env python3
"""Pure conversion and fail-closed geometry tests; no live IOSurface required."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"scripts"))
from iosurface_capture import nonblack_pixels, ppm_from_bgra, validate, validate_presented

frame=bytes.fromhex("a03010ff ffffffff 000000ff 2040c0ff")
ppm=ppm_from_bgra(frame,2,2)
assert ppm==b"P6\n2 2\n255\n"+bytes.fromhex("1030a0 ffffff 000000 c04020")
assert nonblack_pixels(frame)==3
assert validate_presented(4,5,frame)==3
for initial,captured,pixels in [(4,4,frame),(4,5,b"\0\0\0\xff")]:
    try: validate_presented(initial,captured,pixels)
    except RuntimeError: pass
    else: raise AssertionError("stale or all-black presentation accepted")
for values in [(0,2,8,4,1),(2,0,8,4,1),(2,2,7,4,1),(2,2,8,3,1),(2,2,8,4,0)]:
    try: validate(*values)
    except RuntimeError: pass
    else: raise AssertionError(f"unsafe geometry accepted: {values}")
for bad in (frame[:-1],frame+b"x"):
    try: ppm_from_bgra(bad,2,2)
    except RuntimeError: pass
    else: raise AssertionError("wrong-sized frame accepted")
print("PASS: BGRA active-IOSurface conversion and fail-closed geometry")
