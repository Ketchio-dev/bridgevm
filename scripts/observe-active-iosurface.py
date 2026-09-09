#!/usr/bin/env python3
"""OBSERVATION ONLY -- snapshot the active IOSurface without the freshness gate.

scripts/capture-active-iosurface.py refuses a frame whose seed did not advance,
which is correct: a stale frame is not evidence that the scene under test was
presented. This tool deliberately drops that check so a human can *look* at a
screen that is not changing, which is exactly the state the packaged-Notepad
scene fails in. Output of this tool must never be cited as a B6 capture; it
carries observation_only=true in its env file for that reason.
"""
import argparse, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from iosurface_capture import lookup, ppm_from_bgra, seed, sha256, snapshot, nonblack_pixels

p = argparse.ArgumentParser()
p.add_argument("--iosurface", type=pathlib.Path, required=True)
p.add_argument("--out", type=pathlib.Path, required=True)
a = p.parse_args()

ref, ident, width, height = lookup(a.iosurface)
observed_seed = seed(ref)
frame, locked_seed = snapshot(ref, width, height)
ppm = ppm_from_bgra(frame, width, height)
a.out.mkdir(parents=True, exist_ok=True)
(a.out / "observed.bgra").write_bytes(frame)
(a.out / "observed.ppm").write_bytes(ppm)
(a.out / "observe.env").write_text(
    "observation_only=true\nsource=active-cgl-iosurface\n"
    f"iosurface_id={ident}\nwidth={width}\nheight={height}\n"
    f"observed_seed={observed_seed}\nlocked_seed={locked_seed}\n"
    f"nonblack_pixels={nonblack_pixels(frame)}\n"
    f"bgra_sha256={sha256(frame)}\nppm_sha256={sha256(ppm)}\n",
    encoding="ascii")
print(f"OBSERVE id={ident} {width}x{height} seed={locked_seed} nonblack={nonblack_pixels(frame)}")
