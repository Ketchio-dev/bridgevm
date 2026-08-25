#!/usr/bin/env python3
"""Capture one newly presented, nonblack full CGL IOSurface frame."""
import argparse, sys
from pathlib import Path; sys.path.insert(0, str(Path(__file__).resolve().parent))
from iosurface_capture import lookup, ppm_from_bgra, seed, sha256, snapshot, validate_presented
from iosurface_settle import wait

p=argparse.ArgumentParser(); p.add_argument("--iosurface",required=True,type=Path); p.add_argument("--out",required=True,type=Path)
p.add_argument("--timeout-ms",type=int,default=5000); p.add_argument("--settle-ms",type=int,default=250); p.add_argument("--ready",type=Path)
a=p.parse_args(); ref,ident,width,height=lookup(a.iosurface); initial=seed(ref); a.ready and a.ready.write_text(f"initial_seed={initial}\n",encoding="ascii")
def sample():
    frame,observed=snapshot(ref,width,height); return observed,sha256(frame),frame
locked_seed,_,frame=wait(sample,initial,a.timeout_ms,a.settle_ms)
nonblack=validate_presented(initial,locked_seed,frame)
a.out.mkdir(parents=True,exist_ok=True); ppm=ppm_from_bgra(frame,width,height)
(a.out/"presented.bgra").write_bytes(frame); (a.out/"presented.ppm").write_bytes(ppm)
(a.out/"capture.env").write_text(f"source=active-cgl-iosurface\niosurface_id={ident}\nwidth={width}\nheight={height}\ninitial_seed={initial}\ncaptured_seed={locked_seed}\nnonblack_pixels={nonblack}\nbgra_sha256={sha256(frame)}\nppm_sha256={sha256(ppm)}\n",encoding="ascii")
