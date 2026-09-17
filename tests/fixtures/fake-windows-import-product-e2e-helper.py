#!/usr/bin/env python3
import argparse, hashlib, json, pathlib, shutil, stat

parser=argparse.ArgumentParser(); parser.add_argument("--windows-import-product-e2e",action="store_true")
parser.add_argument("--request",required=True); parser.add_argument("--result",required=True); args=parser.parse_args()
request=json.load(open(args.request)); root=pathlib.Path(request["lane_root"]); print("lane_root="+str(root))
assert request["schema_version"]=="bridgevm.windows-hvf-import-product-e2e-request.v1" and request["three_d_injection"] is False
if "noresult" in request["job_id"]: raise SystemExit(0)

def file_hash(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def tree_hash(root):
    root=pathlib.Path(root); records=[]
    for item in sorted(root.rglob("*"),key=lambda value:value.relative_to(root).as_posix()):
        relative=item.relative_to(root).as_posix(); mode=item.lstat().st_mode
        if relative==".lock": continue
        if stat.S_ISDIR(mode): records.append(f"D\t{relative}\n")
        elif stat.S_ISREG(mode): records.append(f"F\t{relative}\t{file_hash(item)}\n")
        else: raise AssertionError("unsafe tree")
    return hashlib.sha256("".join(sorted(records)).encode()).hexdigest()

library=pathlib.Path(request["library_root_path"]); library.mkdir(); share=pathlib.Path(request["share_path"]); share.mkdir()
bundle=pathlib.Path(request["disk_path"]).parents[1]; pathlib.Path(request["disk_path"]).parent.mkdir(parents=True)
pathlib.Path(request["vars_path"]).parent.mkdir(parents=True); pathlib.Path(request["vtpm_state_path"]).mkdir(parents=True)
shutil.copyfile(request["source_disk_path"],request["disk_path"]); shutil.copyfile(request["source_vars_path"],request["vars_path"])
for source in pathlib.Path(request["source_vtpm_path"]).iterdir():
    if source.name != ".lock": shutil.copy2(source,pathlib.Path(request["vtpm_state_path"])/source.name)
vmroot=library/request["vm_slug"]; (vmroot/"vm.json").write_text(json.dumps({"id":request["vm_slug"],"name":request["vm_name"],"backendKind":"hvf-engine","bundlePath":str(bundle),"diskPath":request["disk_path"]})+"\n")
guest=pathlib.Path(request["guest_evidence_path"]); guest.write_text(json.dumps({"nonce":request["nonce"],"status":"fixture"})+"\n")
stages=("artifact_preflight","source_authenticated","ui_imported","imported_media_authenticated","first_ready","keyboard_pointer","clipboard","folder_share","network","audio","first_shutdown","snapshot_restore","second_ready","second_shutdown")
source_disk=file_hash(request["source_disk_path"]); source_vars=file_hash(request["source_vars_path"]); source_vtpm=tree_hash(request["source_vtpm_path"])
result={"schema_version":"bridgevm.windows-hvf-import-product-e2e-lane.v1","job_id":request["job_id"],"commit":request["commit"],"campaign_mode":request["campaign_mode"],"lane":request["lane"],"nonce":request["nonce"],"three_d_injection":False,"ui_frontend_automated":True,"failure_code":"none","failure_detail":"","cleanup_verified":True}
result.update({stage:True for stage in stages}); result.update({"source_disk_sha256":source_disk,"source_vars_sha256":source_vars,"source_vtpm_tree_sha256":source_vtpm,"imported_initial_disk_sha256":source_disk,"imported_initial_vars_sha256":source_vars,"imported_initial_vtpm_tree_sha256":source_vtpm,"final_disk_sha256":file_hash(request["disk_path"]),"final_vars_sha256":file_hash(request["vars_path"]),"final_vtpm_tree_sha256":tree_hash(request["vtpm_state_path"]),"guest_evidence_sha256":file_hash(guest)})
if "bad-hash" in request["job_id"]: result["final_disk_sha256"]="0"*64
with open(args.result,"x") as output: json.dump(result,output,sort_keys=True); output.write("\n")
