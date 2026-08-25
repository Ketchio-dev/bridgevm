#!/usr/bin/env python3
"""Validate the sealed 20-real-workload input/result/frame-time contract."""
import argparse,csv,hashlib,io,math,os,re,stat,tempfile,unittest
from pathlib import Path

HEX=re.compile(r"[0-9a-f]{64}"); ID=re.compile(r"[a-z0-9][a-z0-9.-]{2,63}")
INPUT=["id","executable_sha256","payload_sha256","source","version","license","architecture","api","category","warmup_seconds","measurement_seconds"]
RESULT=["id","class","visible","crash_reset","clean_shutdown","samples","p50_ms","p95_ms","p99_ms","series_sha256"]
CLASSES={"pass","degraded","unsupported","crash","launch-fail"}; APIS={"vulkan","d3d11","d3d12","opengl","webgpu"}; ARCH={"arm64","x64"}
SYNTHETIC=("smoke","synthetic","triangle","bridgevm-d3d","vkcube")

def bounded(path,limit):
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    try:
        info=os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size>limit: raise ValueError(f"unsafe evidence file: {path.name}")
        raw=os.read(fd,limit+1)
    finally: os.close(fd)
    if len(raw)>limit: raise ValueError(f"oversized evidence file: {path.name}")
    return raw
def rows(path,fields):
    reader=csv.DictReader(io.StringIO(bounded(path,1_000_000).decode()),delimiter="\t")
    if reader.fieldnames!=fields: raise ValueError(f"bad columns in {path.name}")
    return list(reader)
def quantile(values,q): return values[int((len(values)-1)*q)]
def series(root,ident):
    raw=bounded(root/f"{ident}.frametimes-ms",8_000_000); values=[float(x) for x in raw.splitlines()]
    if not values or any(not math.isfinite(x) or x<=0 or x>60_000 for x in values): raise ValueError("invalid frame-time sample")
    return raw,sorted(values)
def validate(inputs,results,evidence):
    info=os.lstat(evidence)
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode): raise ValueError("unsafe evidence root")
    ins,outs=rows(inputs,INPUT),rows(results,RESULT)
    if len(ins)!=20 or len(outs)!=20: raise ValueError("matrix requires exactly 20 rows")
    ids=[x["id"] for x in ins]
    if len(set(ids))!=20 or any(not ID.fullmatch(x) or any(word in x for word in SYNTHETIC) for x in ids): raise ValueError("invalid, duplicate or synthetic id")
    for row in ins:
        if not HEX.fullmatch(row["executable_sha256"]) or not HEX.fullmatch(row["payload_sha256"]): raise ValueError("missing workload identity")
        if row["architecture"] not in ARCH or row["api"] not in APIS or not all(row[x].strip() for x in ("source","version","license","category")): raise ValueError("missing workload provenance")
        if int(row["warmup_seconds"])<0 or int(row["measurement_seconds"])<30: raise ValueError("invalid measurement window")
    if [x["id"] for x in outs]!=ids: raise ValueError("result ids/order differ from input")
    for row in outs:
        if row["class"] not in CLASSES or row["visible"] not in {"yes","no"} or row["crash_reset"] not in {"yes","no"} or row["clean_shutdown"] not in {"yes","no"}: raise ValueError("invalid result class")
        if row["class"]=="pass" and (row["visible"],row["crash_reset"],row["clean_shutdown"])!=("yes","no","yes"): raise ValueError("inconsistent pass class")
        raw,values=series(evidence,row["id"]); expected=[quantile(values,q) for q in (.5,.95,.99)]
        supplied=[float(row[x]) for x in ("p50_ms","p95_ms","p99_ms")]
        if int(row["samples"])!=len(values) or supplied!=expected or row["series_sha256"]!=hashlib.sha256(raw).hexdigest(): raise ValueError("frame-time evidence mismatch")
    return 20

def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp); a=root/"inputs.tsv"; b=root/"results.tsv"; ev=root/"evidence"; ev.mkdir()
        with a.open("w",newline="") as x, b.open("w",newline="") as y:
            wi,wr=csv.DictWriter(x,INPUT,delimiter="\t",lineterminator="\n"),csv.DictWriter(y,RESULT,delimiter="\t",lineterminator="\n"); wi.writeheader(); wr.writeheader()
            for n in range(20):
                ident=f"real-title-{n:02d}"; raw=b"10\n20\n30\n40\n"; (ev/f"{ident}.frametimes-ms").write_bytes(raw)
                wi.writerow(dict(id=ident,executable_sha256="a"*64,payload_sha256="b"*64,source="publisher",version="1",license="owned",architecture="arm64",api="vulkan",category="native-title",warmup_seconds=30,measurement_seconds=60))
                wr.writerow(dict(id=ident,**{"class":"pass"},visible="yes",crash_reset="no",clean_shutdown="yes",samples=4,p50_ms=20,p95_ms=30,p99_ms=30,series_sha256=hashlib.sha256(raw).hexdigest()))
        assert validate(a,b,ev)==20
        saved=a.read_text(); body=saved.splitlines()
        for changed,label in [("\n".join(body[:-1])+"\n","19"),(saved+body[-1]+"\n","21")]:
            a.write_text(changed)
            try: validate(a,b,ev)
            except ValueError: pass
            else: raise AssertionError(f"{label}-row matrix survived")
        a.write_text(saved)
        for path,old,new in [(a,"real-title-00","bridgevm-d3d11-smoke"),(a,"\t"+"a"*64+"\t","\tbad\t"),(b,"\t20\t30\t30\t","\t20\t30\t20\t")]:
            saved=path.read_text(); changed=saved.replace(old,new,1); assert changed!=saved; path.write_text(changed)
            try: validate(a,b,ev)
            except (ValueError,FileNotFoundError): pass
            else: raise AssertionError("matrix mutation survived")
            path.write_text(saved)
        target=ev/"real-title-19.frametimes-ms"; saved=target.read_bytes(); [(target.write_text(bad),unittest.TestCase().assertRaises(ValueError,series,ev,"real-title-19")) for bad in ("nan\n","inf\n","-inf\n")]; target.write_bytes(saved); target.unlink(); target.symlink_to(ev/"real-title-18.frametimes-ms")
        try: validate(a,b,ev)
        except OSError: pass
        else: raise AssertionError("symlink series survived")
        target.unlink(); target.write_bytes(saved)
    print("PASS: 20-real-workload compatibility contract and mutations")

def main():
    p=argparse.ArgumentParser(); p.add_argument("--inputs",type=Path); p.add_argument("--results",type=Path); p.add_argument("--evidence",type=Path); p.add_argument("--self-test",action="store_true"); a=p.parse_args()
    if a.self_test: return self_test()
    if not all((a.inputs,a.results,a.evidence)): p.error("inputs, results and evidence are required")
    print(f"PASS: compatibility matrix rows={validate(a.inputs,a.results,a.evidence)}")
if __name__=="__main__": main()
