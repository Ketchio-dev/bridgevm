#!/usr/bin/env python3
import sys,tempfile
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"scripts"))
from glyph_region_analysis import analyze
from iosurface_settle import wait

class Clock:
    def __init__(self): self.now=0
    def clock(self): self.now+=1_000_000; return self.now
    def pause(self,_): pass
clock=Clock(); samples=iter([(1,"a",b"a"),(2,"b",b"b"),(3,"b",b"b")]+[(4,"b",b"b")]*300)
assert wait(lambda:next(samples),1,1000,250,clock.clock,clock.pause)==(4,"b",b"b")
clock=Clock()
try: wait(lambda:(1,"a",b"a"),1,5,2,clock.clock,clock.pause)
except RuntimeError: pass
else: raise AssertionError("never-advanced seed accepted")
with tempfile.TemporaryDirectory() as tmp:
    path=Path(tmp)/"scene.ppm"; width,height=800,220; pixels=bytearray(b"\x20\x20\x20"*(width*height))
    for y in range(70,80):
        for x in range(60,80): pixels[(y*width+x)*3:(y*width+x+1)*3]=b"\xff\xff\xff"
    path.write_bytes(f"P6\n{width} {height}\n255\n".encode()+pixels); result=analyze(path)
    assert result["regions"]["caption"]["components"]==1 and result["regions"]["caption"]["foreground_pixels"]==200
    assert sum(result["regions"]["caption"]["luminance_16"])==700*32
    assert result["regions"]["tabs"]["components"]==0 and result["regions"]["menu"]["components"]==0
print("PASS: settled glyph scene and region observations")
