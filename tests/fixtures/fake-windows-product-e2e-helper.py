#!/usr/bin/env python3
import argparse, hashlib, json, os, pathlib, shutil, subprocess, sys
p=argparse.ArgumentParser(); p.add_argument("--windows-product-e2e", action="store_true"); p.add_argument("--request", required=True); p.add_argument("--result", required=True); a=p.parse_args()
r=json.load(open(a.request)); print("lane_root="+r["lane_root"])
assert r["schema_version"] == "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"
assert r["vm_slug"] == f"bridgevm-t17-lane-{r['lane']}-{r['nonce'][:12]}"
if "noresult" in r["job_id"]: raise SystemExit(0)
if "survivor" in r["job_id"]:
    subprocess.Popen([sys.executable, "-c", "import time; time.sleep(20)", r["lane_root"]], start_new_session=True)
    raise SystemExit(1)
pathlib.Path(r["library_root_path"]).mkdir(); share=pathlib.Path(r["share_path"]); share.mkdir()
(share/f"t17-{r['nonce'][:12]}.txt").write_text(f"bridgevm-t17-share-v1\n{r['nonce']}\n")
vmroot=pathlib.Path(r["library_root_path"])/r["vm_slug"]; vmroot.mkdir()
(vmroot/"vm.json").write_text(json.dumps({"id":r["vm_slug"],"name":r["vm_name"],"bundlePath":str(vmroot/"bundle.vmbridge"),"installPending":False,"experimental3DAllowed":False})+"\n")
for field,data in {"disk_path":b"disk", "vars_path":b"vars"}.items():
    path=pathlib.Path(r[field]); path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(data+str(r["lane"]).encode())
if "alias" in r["job_id"] or ("crosslane" in r["job_id"] and r["lane"] > 1): target=pathlib.Path(r["vars_path"] if "alias" in r["job_id"] else r["disk_path"]); source=pathlib.Path(r["disk_path"]) if "alias" in r["job_id"] else next((pathlib.Path(r["lane_root"]).parent/"lane-1/library").glob("*/bundle.vmbridge/disks/hvf-target.raw")); target.unlink(); os.link(source,target)
if "guest-alias" in r["job_id"]: pathlib.Path(r["disk_path"]).unlink(); os.link(pathlib.Path(r["guest_payload_path"])/"agent.bin",r["disk_path"])
vtpm=pathlib.Path(r["vtpm_state_path"]); vtpm.mkdir(); (vtpm/"state.bin").write_bytes(b"vtpm")
source=pathlib.Path(r["library_root_path"])/"Derived/WindowsInstallSources"/("win11-"+"1"*64+".raw")
source.parent.mkdir(parents=True); source.write_bytes(b"prepared source")
source_hash=hashlib.sha256(source.read_bytes()).hexdigest(); pathlib.Path(str(source)+".sha256").write_text(source_hash+"\n")
if "bad-source-receipt" in r["job_id"]: pathlib.Path(str(source)+".sha256").write_text("0"*64+"\n")
policy=json.load(open(r["secure_boot_policy_path"])); secure={"schemaVersion":1,"policy":policy["policy"],"sourceTag":policy["source"]["tag"],"sourceCommit":policy["source"]["commit"],"sourceAssetSha256":policy["source"]["assetSha256"],"firmwareFileName":policy["firmware"]["fileName"],"firmwareSha256":policy["firmware"]["sha256"],"firmwareEdk2Commit":policy["firmware"]["edk2Commit"],"provisionedAt":"2026-09-01T00:00:00Z","variables":[{"name":v["name"],"vendorGuid":v["vendorGuid"],"attributes":v["attributes"],"payloadSha256":v["sha256"]} for v in policy["variables"]]}
secure_path=pathlib.Path(r["secure_boot_receipt_path"]); secure_path.write_text(json.dumps(secure)+"\n")
if "bad-secure" in r["job_id"]: secure["policy"]="invented"; secure_path.write_text(json.dumps(secure)+"\n")
snapshot=pathlib.Path(r["snapshot_path"]); snapshot.mkdir(parents=True)
snapshot_disk=snapshot/"disk.raw"; snapshot_vars=snapshot/"vars.fd"
shutil.copyfile(r["disk_path"],snapshot_disk); shutil.copyfile(r["vars_path"],snapshot_vars)
snapshot_manifest={"format_version":1,"vm_id":r["vm_slug"],"disk_bytes":snapshot_disk.stat().st_size,"disk_sha256":hashlib.sha256(snapshot_disk.read_bytes()).hexdigest(),"vars_bytes":snapshot_vars.stat().st_size,"vars_sha256":hashlib.sha256(snapshot_vars.read_bytes()).hexdigest()}
(snapshot/"manifest.json").write_text(json.dumps(snapshot_manifest)+"\n")
if "bad-snapshot" in r["job_id"]: snapshot_manifest["disk_sha256"]="0"*64; (snapshot/"manifest.json").write_text(json.dumps(snapshot_manifest)+"\n")
stages=("artifact_preflight","vm_created","source_prepared","windows_installed","secure_boot_provisioned","first_ready","keyboard_pointer","clipboard","folder_share","network","audio","first_shutdown","snapshot_restore","second_ready","second_shutdown")
nonce=r["nonce"]; prefix=nonce[:12]; clipboard=f"브리지VM T17 클립보드 왕복 v1\n{nonce}\n".encode()
raw={"keyboard_pointer_challenge_sha256":(f"t17-keyboard-pointer-{prefix}.txt",f"bridgevm-t17-keyboard-pointer-v1\n{nonce}\n".encode()),"clipboard_roundtrip_sha256":(f"t17-clipboard-guest-{prefix}.txt",clipboard),"share_host_to_guest_sha256":(f"t17-{prefix}.txt",f"bridgevm-t17-share-v1\n{nonce}\n".encode()),"share_guest_to_host_sha256":(f"t17-guest-{prefix}.txt",f"bridgevm-t17-guest-share-v1\n{nonce}\n".encode()),"network_result_sha256":(f"t17-network-{prefix}.txt",f"bridgevm-t17-network-ok-v1\n{nonce}\n".encode()),"audio_result_sha256":(f"t17-audio-{prefix}.txt",f"bridgevm-t17-audio-ok-v1\n{nonce}\n".encode()),"snapshot_marker_a_sha256":(f"t17-snapshot-a-{prefix}.txt",f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode()),"snapshot_marker_b_sha256":(f"t17-snapshot-b-{prefix}.txt",f"bridgevm-t17-snapshot-b-v1\n{nonce}\n".encode()),"snapshot_marker_restored_a_sha256":(f"t17-snapshot-restored-a-{prefix}.txt",f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode())}
[(share/name).write_bytes(body) for name,body in raw.values()]; (share/f"t17-clipboard-host-{prefix}.txt").write_bytes(clipboard)
frames=0 if "bad-audio" in r["job_id"] else 480; first_lines=["BVAGENT READY FIRST",f"hda CoreAudio stats: frames_rendered={frames} drops=0 callback_errors=0","stop: PSCI SYSTEM_OFF"]; mutation_lines=["BVAGENT READY MUTATION","stop: PSCI SYSTEM_OFF"]; final_lines=["BVAGENT READY RESTORED","stop: PSCI SYSTEM_OFF"]
def log_body(lines):
    offsets=[]; body=b""
    for line in lines: offsets.append(len(body)); body+=(line+"\n").encode()
    return offsets,body
first_offsets,first_body=log_body(first_lines); mutation_offsets,mutation_body=log_body(mutation_lines); final_offsets,final_body=log_body(final_lines)
first_log=vmroot/"bundle.vmbridge/metadata/product-e2e/first-run.log"; first_log.parent.mkdir(parents=True,exist_ok=True); first_log.write_bytes(first_body); mutation_log=vmroot/"bundle.vmbridge/metadata/product-e2e/mutation-run.log"; mutation_log.write_bytes(mutation_body)
final_log=vmroot/"bundle.vmbridge/logs/hvf/run.log"; final_log.parent.mkdir(parents=True,exist_ok=True); final_log.write_bytes(final_body)
observations={key:hashlib.sha256(body).hexdigest() for key,(_,body) in raw.items()}; observations.update(audio_playback_count=1,audio_error_count=0)
identity={"job_id":r["job_id"],"commit":r["commit"],"lane":r["lane"],"nonce":nonce,"vm_slug":r["vm_slug"]}
agent={"schema_version":"bridgevm.windows-product-e2e-agent-result.v2",**identity,**observations}
guest_agent=share/f"t17-agent-result-{nonce[:12]}.json"; guest_agent.write_text(json.dumps(agent)+"\n")
agent_path=vmroot/"bundle.vmbridge/metadata/product-e2e/agent-result.json"; shutil.copyfile(guest_agent,agent_path)
events=(("first-ready",first_lines[0]),("first-shutdown",first_lines[2]),("mutation-ready",mutation_lines[0]),("mutation-shutdown",mutation_lines[1]),("final-ready",final_lines[0]),("second-shutdown",final_lines[1]))
line_hashes=[hashlib.sha256(f"bridgevm-t17-{event}-v1\n{nonce}\n{line}\n".encode()).hexdigest() for event,line in events]
guest={"schema_version":"bridgevm.windows-product-e2e-guest-evidence.v2",**identity,**observations,"first_run_log_sha256":hashlib.sha256(first_body).hexdigest(),"mutation_run_log_sha256":hashlib.sha256(mutation_body).hexdigest(),"final_run_log_sha256":hashlib.sha256(final_body).hexdigest(),"agent_result_sha256":hashlib.sha256(agent_path.read_bytes()).hexdigest(),"first_ready_offset":first_offsets[0],"first_ready_line_nonce_sha256":line_hashes[0],"first_shutdown_offset":first_offsets[2],"first_shutdown_line_nonce_sha256":line_hashes[1],"mutation_ready_offset":mutation_offsets[0],"mutation_ready_line_nonce_sha256":line_hashes[2],"mutation_shutdown_offset":mutation_offsets[1],"mutation_shutdown_line_nonce_sha256":line_hashes[3],"final_ready_offset":final_offsets[0],"final_ready_line_nonce_sha256":line_hashes[4],"second_shutdown_offset":final_offsets[1],"second_shutdown_line_nonce_sha256":line_hashes[5]}
guest["audio_error_count"]=1 if "bad-guest" in r["job_id"] else guest["audio_error_count"]; guest["mutation_ready_line_nonce_sha256"]="0"*64 if "bad-mutation" in r["job_id"] else guest["mutation_ready_line_nonce_sha256"]
guest_path=pathlib.Path(r["guest_evidence_path"]); guest_path.write_text(json.dumps(guest)+"\n"); "bad-raw" in r["job_id"] and (share/f"t17-network-{prefix}.txt").write_bytes(b"forged")
result={"schema_version":"bridgevm.windows-hvf-3d-off-product-e2e-lane.v2", "job_id":r["job_id"], "commit":r["commit"], "campaign_mode":r["campaign_mode"], "lane":r["lane"], "nonce":r["nonce"], "three_d_injection":False, "ui_frontend_automated":True, "failure_code":"none", "cleanup_verified":True, "installer_source_path":str(source)}
result.update({stage:True for stage in stages}); result["installer_source_sha256"]=source_hash
for output,path_field in (("final_disk_sha256","disk_path"),("final_vars_sha256","vars_path"),("secure_boot_receipt_sha256","secure_boot_receipt_path"),("guest_evidence_sha256","guest_evidence_path")): result[output]=hashlib.sha256(pathlib.Path(r[path_field]).read_bytes()).hexdigest()
if "malformed" in r["job_id"]: result["future_unverified_field"]=True
if "bad-hash" in r["job_id"]: result["final_disk_sha256"]="0"*64
if "partial" in r["job_id"]: result["second_shutdown"]=False; result["failure_code"]="internal-error"
with open(a.result,"x") as out: json.dump(result,out,sort_keys=True); out.write("\n")
