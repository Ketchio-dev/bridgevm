#!/usr/bin/env python3
"""Verify ten audio summaries and write the flat t9 receipt."""
import argparse,json,platform,subprocess
from datetime import datetime,timezone
from pathlib import Path
from audio_teardown_summary import passes,read
from audio_teardown_receipt_self_test import run as run_self_test
def assess(out):
    rows=[read(out/f"runs/run{n}/summary.txt") for n in range(1,11)]
    good=[row for row in rows if passes(row)]
    return rows,good,sum(int(r.get("callback_errors",1)) for r in rows),sum(int(r.get("frames_rendered",0)) for r in rows)
def write(out,job,commit,image,vars_hash):
    rows,good,callback,frames=assess(out); passed=len(good)==10
    receipt={"tier":"t9-audio-teardown","gate_id":"a5-audio-teardown-quality","criterion":"A5-quality","job_id":job,"commit":commit,"image_sha256":image,"vars_sha256":vars_hash,"host_model":subprocess.check_output(["sysctl","-n","hw.model"],text=True).strip(),"macos_version":platform.mac_ver()[0],"sample_count":len(rows),"required_run_count":10,"passes":len(good),"failures":10-len(good),"callback_errors":callback,"frames_rendered":frames,"finished_at":datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),"outcome":"completed" if passed else "failed","pass":passed}
    (out/"receipt.json").write_text(json.dumps(receipt,indent=2)+"\n"); return 0 if passed else 1

def main():
    p=argparse.ArgumentParser(); p.add_argument("--self-test",action="store_true")
    for name in ("out","job","commit","image","vars"): p.add_argument(f"--{name}")
    a=p.parse_args()
    if a.self_test: run_self_test(assess); return
    if not all((a.out,a.job,a.commit,a.image,a.vars)): p.error("receipt fields are required")
    raise SystemExit(write(Path(a.out),a.job,a.commit,a.image,a.vars))
if __name__=="__main__": main()
