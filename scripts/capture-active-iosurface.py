#!/usr/bin/env python3
"""Capture one newly presented, nonblack full CGL IOSurface frame."""
import argparse, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from iosurface_capture import lookup, ppm_from_bgra, seed, sha256, snapshot, validate_presented

p=argparse.ArgumentParser(); p.add_argument("--iosurface",required=True,type=Path)
p.add_argument("--out",required=True,type=Path); p.add_argument("--timeout-ms",type=int,default=5000)
a=p.parse_args(); ref,ident,width,height=lookup(a.iosurface); initial=seed(ref)
deadline=time.monotonic_ns()+a.timeout_ms*1_000_000
while seed(ref)==initial and time.monotonic_ns()<=deadline: time.sleep(0.001)
frame, locked_seed=snapshot(ref,width,height)
nonblack=validate_presented(initial,locked_seed,frame)
a.out.mkdir(parents=True,exist_ok=True); ppm=ppm_from_bgra(frame,width,height)
(a.out/"presented.bgra").write_bytes(frame); (a.out/"presented.ppm").write_bytes(ppm)
(a.out/"capture.env").write_text(f"source=active-cgl-iosurface\niosurface_id={ident}\nwidth={width}\nheight={height}\ninitial_seed={initial}\ncaptured_seed={locked_seed}\nnonblack_pixels={nonblack}\nbgra_sha256={sha256(frame)}\nppm_sha256={sha256(ppm)}\n",encoding="ascii")
