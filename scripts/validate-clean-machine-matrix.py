#!/usr/bin/env python3
"""Validate four cross-generation release-only clean-machine receipts."""
import argparse,csv,hashlib,io,json,os,re,stat,tempfile
from pathlib import Path

FIELDS=["generation","host_model","macos_version","macos_build","release_sha256","image_sha256","vars_sha256","clean_user","checkout_absent","prior_app_absent","prior_data_absent","repo_override_unset","swtpm_override_unset","path_helper_absent","release_override_scan","install","boot","update","rollback","receipt_sha256"]
HEX=re.compile(r"[0-9a-f]{64}"); MODEL=re.compile(r"Mac[0-9]+,[0-9]+")

def safe_read(path,limit=1_000_000):
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    try:
        info=os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size>limit: raise ValueError("unsafe evidence file")
        raw=os.read(fd,limit+1)
    finally: os.close(fd)
    if len(raw)>limit: raise ValueError("oversized evidence file")
    return raw

def validate(path,receipts):
    info=os.lstat(receipts)
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode): raise ValueError("unsafe receipt root")
    reader=csv.DictReader(io.StringIO(safe_read(path).decode()),delimiter="\t"); rows=list(reader)
    if reader.fieldnames!=FIELDS or len(rows)!=4: raise ValueError("matrix needs fixed columns and four rows")
    if [r["generation"] for r in rows] != ["M1","M2","M3","M4"]: raise ValueError("matrix needs ordered M1 through M4")
    sealed=tuple(rows[0][key] for key in ("release_sha256","image_sha256","vars_sha256"))
    for row in rows:
        if not MODEL.fullmatch(row["host_model"]) or not row["macos_version"] or not row["macos_build"]: raise ValueError("missing host identity")
        hashes=tuple(row[key] for key in ("release_sha256","image_sha256","vars_sha256"))
        if hashes!=sealed or any(not HEX.fullmatch(x) for x in (*hashes,row["receipt_sha256"])): raise ValueError("unsealed or differing inputs")
        if any(row[key]!="yes" for key in FIELDS[7:19]): raise ValueError(f"clean release flow failed: {row['generation']}")
        if any("/" in value or "\\" in value or "secret" in value.lower() for value in row.values()): raise ValueError("receipt contains path or secret")
        raw=safe_read(receipts/f"{row['generation']}.json"); receipt=json.loads(raw)
        if hashlib.sha256(raw).hexdigest()!=row["receipt_sha256"] or not isinstance(receipt,dict) or any(isinstance(x,(dict,list)) for x in receipt.values()): raise ValueError("receipt identity or shape mismatch")
        if any(str(receipt.get(key,""))!=row[key] for key in FIELDS[:-1]): raise ValueError("receipt differs from matrix")
    if len({r["host_model"] for r in rows})!=4: raise ValueError("each generation needs a distinct host")
    return 4

def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp); path=root/"matrix.tsv"; receipts=root/"receipts"; receipts.mkdir(); rows=[]
        for n,generation in enumerate(("M1","M2","M3","M4"),1):
            row={key:"yes" for key in FIELDS[:-1]}; row.update(generation=generation,host_model=f"Mac{n},1",macos_version="26.5.2",macos_build="25F90",release_sha256="a"*64,image_sha256="b"*64,vars_sha256="c"*64)
            raw=(json.dumps(row,sort_keys=True)+"\n").encode(); (receipts/f"{generation}.json").write_bytes(raw); row["receipt_sha256"]=hashlib.sha256(raw).hexdigest(); rows.append(row)
        with path.open("w",newline="") as stream:
            writer=csv.DictWriter(stream,FIELDS,delimiter="\t",lineterminator="\n"); writer.writeheader(); writer.writerows(rows)
        assert validate(path,receipts)==4; saved=path.read_text()
        mutations=[("M3\t","M2\t"),("\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\t","\tyes\tyes\tyes\tno\tyes\tyes\tyes\tyes\tyes\tyes\tyes\tyes\t"),("\t"+"b"*64+"\t","\tbad\t")]
        for old,new in mutations:
            changed=saved.replace(old,new,1); assert changed!=saved; path.write_text(changed)
            try: validate(path,receipts)
            except ValueError: pass
            else: raise AssertionError("clean-machine mutation survived")
        path.write_text("\n".join(saved.splitlines()[:-1])+"\n")
        try: validate(path,receipts)
        except ValueError: pass
        else: raise AssertionError("three-cell matrix survived")
    print("PASS: four-generation clean-machine contract and mutations")

def main():
    p=argparse.ArgumentParser(); p.add_argument("--matrix",type=Path); p.add_argument("--receipts",type=Path); p.add_argument("--self-test",action="store_true"); a=p.parse_args()
    if a.self_test: self_test(); return
    if not a.matrix or not a.receipts: p.error("--matrix and --receipts are required")
    print(f"PASS: clean-machine cells={validate(a.matrix,a.receipts)}")
if __name__=="__main__": main()
