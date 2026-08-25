#!/usr/bin/env python3
"""Record deterministic pixel observations for fixed Notepad chrome regions."""
import argparse,collections,hashlib,json
from pathlib import Path
REGIONS={"caption":(50,60,750,92),"tabs":(50,92,750,140),"menu":(50,140,750,190)}

def ppm(path):
    raw=path.read_bytes(); magic,dims,maximum,pixels=raw.split(b"\n",3)
    width,height=map(int,dims.split())
    if magic!=b"P6" or maximum!=b"255" or len(pixels)!=width*height*3: raise ValueError("invalid P6 frame")
    return width,height,pixels
def observe(pixels,width,height,rect):
    x0,y0,x1,y1=rect
    if not (0<=x0<x1<=width and 0<=y0<y1<=height): raise ValueError("region outside frame")
    values=[]
    for y in range(y0,y1):
        start=(y*width+x0)*3; values.extend(tuple(pixels[i:i+3]) for i in range(start,start+(x1-x0)*3,3))
    background=collections.Counter(values).most_common(1)[0][0]
    foreground={n for n,color in enumerate(values) if max(abs(color[i]-background[i]) for i in range(3))>=24}; foreground_pixels=len(foreground)
    luminance=[0]*16
    for red,green,blue in values: luminance[min(15,(54*red+183*green+19*blue)//4096)]+=1
    components=0
    while foreground:
        components+=1; stack=[foreground.pop()]
        while stack:
            n=stack.pop(); x,y=n%(x1-x0),n//(x1-x0)
            for yy in range(max(0,y-1),min(y1-y0,y+2)):
                for xx in range(max(0,x-1),min(x1-x0,x+2)):
                    q=yy*(x1-x0)+xx
                    if q in foreground: foreground.remove(q); stack.append(q)
    packed=b"".join(bytes(value) for value in values)
    return {"rect":rect,"background_rgb":background,"foreground_pixels":foreground_pixels,"components":components,"luminance_16":luminance,"sha256":hashlib.sha256(packed).hexdigest()}
def analyze(path):
    width,height,pixels=ppm(path)
    return {"width":width,"height":height,"regions":{name:observe(pixels,width,height,rect) for name,rect in REGIONS.items()}}
def main():
    p=argparse.ArgumentParser(); p.add_argument("--ppm",required=True,type=Path); p.add_argument("--out",type=Path); a=p.parse_args(); result=analyze(a.ppm); rendered=json.dumps(result,indent=2)+"\n"
    a.out.write_text(rendered) if a.out else print(rendered,end="")
if __name__=="__main__": main()
