#!/usr/bin/env python3
"""Verify ten audio summaries and write the flat t9 receipt."""
import argparse,json,platform,subprocess,tempfile
from datetime import datetime,timezone
from pathlib import Path

def values(path): return dict(line.split("=",1) for line in path.read_text().splitlines() if "=" in line)
def assess(out):
    rows=[values(out/f"runs/run{n}/summary.txt") for n in range(1,11)]
    good=[r for r in rows if int(r.get("frames_rendered",0))>0 and r.get("drops")=="0" and r.get("callback_errors")=="0"]
    return rows,good,sum(int(r.get("callback_errors",1)) for r in rows),sum(int(r.get("frames_rendered",0)) for r in rows)
def write(out,job,commit,image,vars_hash):
    rows,good,callback,frames=assess(out); passed=len(good)==10
    receipt={"tier":"t9-audio-teardown","gate_id":"a5-audio-teardown-quality","criterion":"A5-quality","job_id":job,"commit":commit,"image_sha256":image,"vars_sha256":vars_hash,"host_model":subprocess.check_output(["sysctl","-n","hw.model"],text=True).strip(),"macos_version":platform.mac_ver()[0],"sample_count":len(rows),"required_run_count":10,"passes":len(good),"failures":10-len(good),"callback_errors":callback,"frames_rendered":frames,"finished_at":datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),"outcome":"completed" if passed else "failed","pass":passed}
    (out/"receipt.json").write_text(json.dumps(receipt,indent=2)+"\n"); return 0 if passed else 1
def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp); (out/"runs").mkdir()
        for n in range(1,11):
            path=out/f"runs/run{n}"; path.mkdir(); (path/"summary.txt").write_text("frames_rendered=48000\ndrops=0\ncallback_errors=0\n")
        assert len(assess(out)[1])==10
        target=out/"runs/run7/summary.txt"
        for old,new in [("callback_errors=0","callback_errors=1"),("drops=0","drops=1"),("frames_rendered=48000","frames_rendered=0")]:
            saved=target.read_text(); target.write_text(saved.replace(old,new)); assert len(assess(out)[1])==9; target.write_text(saved)
    print("PASS: audio teardown receipt verifier and mutations")
def main():
    p=argparse.ArgumentParser(); p.add_argument("--self-test",action="store_true")
    for name in ("out","job","commit","image","vars"): p.add_argument(f"--{name}")
    a=p.parse_args()
    if a.self_test: self_test(); return
    if not all((a.out,a.job,a.commit,a.image,a.vars)): p.error("receipt fields are required")
    raise SystemExit(write(Path(a.out),a.job,a.commit,a.image,a.vars))
if __name__=="__main__": main()
