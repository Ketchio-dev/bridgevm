"""Synthetic shared-journey raw evidence for installed-disk import tests."""
import hashlib
import json
import pathlib
import shutil


def write_guest_evidence(r):
    share = pathlib.Path(r["share_path"])
    bundle = pathlib.Path(r["disk_path"]).parent.parent
    nonce=r["nonce"]; prefix=nonce[:12]; clipboard=f"브리지VM T17 클립보드 왕복 v1\n{nonce}\n".encode()
    raw={"keyboard_pointer_challenge_sha256":(f"t17-keyboard-pointer-{prefix}.txt",f"bridgevm-t17-keyboard-pointer-v1\n{nonce}\n".encode()),"clipboard_roundtrip_sha256":(f"t17-clipboard-guest-{prefix}.txt",clipboard),"share_host_to_guest_sha256":(f"t17-{prefix}.txt",f"bridgevm-t17-share-v1\n{nonce}\n".encode()),"share_guest_to_host_sha256":(f"t17-guest-{prefix}.txt",f"bridgevm-t17-guest-share-v1\n{nonce}\n".encode()),"network_result_sha256":(f"t17-network-{prefix}.txt",f"bridgevm-t17-network-ok-v1\n{nonce}\n".encode()),"audio_result_sha256":(f"t17-audio-{prefix}.txt",f"bridgevm-t17-audio-ok-v1\n{nonce}\n".encode()),"snapshot_marker_a_sha256":(f"t17-snapshot-a-{prefix}.txt",f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode()),"snapshot_marker_b_sha256":(f"t17-snapshot-b-{prefix}.txt",f"bridgevm-t17-snapshot-b-v1\n{nonce}\n".encode()),"snapshot_marker_restored_a_sha256":(f"t17-snapshot-restored-a-{prefix}.txt",f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode())}
    [(share/name).write_bytes(body) for name,body in raw.values()]; (share/f"t17-clipboard-host-{prefix}.txt").write_bytes(clipboard)
    frames=0 if "bad-audio" in r["job_id"] else 480; live="BVAGENT PONG (proactive)" if "proactive-pong" in r["job_id"] else "BVAGENT READY"; stop="stop: PSCI SYSTEM_OFF" if "retired-stop" in r["job_id"] else "stop: PSCI 0x84000008 (system off)"; first_lines=[f"{live} FIRST",stop]; mutation_lines=[f"{live} MUTATION",stop]; final_lines=[f"{live} RESTORED",stop]
    def log_body(lines,tail=""):
        offsets=[]; body=b""
        for line in lines: body+=b"=== EDK2 boot probe (with Apple hv_gic) ===\n" if line.startswith("stop: ") else b""; offsets.append(len(body)); body+=(line+"\n").encode()
        return offsets,body+b"serial raw bytes: 0 output bytes: 0\n--- serial (tail) ---\n\n--- end ---\n"+tail.encode()
    first_offsets,first_body=log_body(first_lines,f"hda CoreAudio stats: frames_rendered={frames} drops=0 dropped_bytes=0 format_drops=0 ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 callback_errors=3 callback_active_errors=0 callback_stopping_errors=3 callback_expected_stopping_errors=3 callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=3 callback_stopping_disposal_pending=0 callback_stopping_unclassified=0\n"); mutation_offsets,mutation_body=log_body(mutation_lines); final_offsets,final_body=log_body(final_lines)
    first_log=bundle/"metadata/product-e2e/first-run.log"; first_log.parent.mkdir(parents=True,exist_ok=True); first_log.write_bytes(first_body); mutation_log=bundle/"metadata/product-e2e/mutation-run.log"; mutation_log.write_bytes(mutation_body)
    final_log=bundle/"logs/hvf/run.log"; final_log.parent.mkdir(parents=True,exist_ok=True); final_log.write_bytes(final_body)
    observations={key:hashlib.sha256(body).hexdigest() for key,(_,body) in raw.items()}; observations.update(audio_playback_count=1,audio_error_count=0)
    identity={"job_id":r["job_id"],"commit":r["commit"],"lane":r["lane"],"nonce":nonce,"vm_slug":r["vm_slug"]}
    agent={"schema_version":"bridgevm.windows-product-e2e-agent-result.v2",**identity,**observations}
    guest_agent=share/f"t17-agent-result-{nonce[:12]}.json"; guest_agent.write_text(json.dumps(agent)+"\n")
    agent_path=bundle/"metadata/product-e2e/agent-result.json"; shutil.copyfile(guest_agent,agent_path)
    events=(("first-ready",first_lines[0]),("first-shutdown",first_lines[1]),("mutation-ready",mutation_lines[0]),("mutation-shutdown",mutation_lines[1]),("final-ready",final_lines[0]),("second-shutdown",final_lines[1]))
    line_hashes=[hashlib.sha256(f"bridgevm-t17-{event}-v1\n{nonce}\n{line}\n".encode()).hexdigest() for event,line in events]
    guest={"schema_version":"bridgevm.windows-product-e2e-guest-evidence.v2",**identity,**observations,"first_run_log_sha256":hashlib.sha256(first_body).hexdigest(),"mutation_run_log_sha256":hashlib.sha256(mutation_body).hexdigest(),"final_run_log_sha256":hashlib.sha256(final_body).hexdigest(),"agent_result_sha256":hashlib.sha256(agent_path.read_bytes()).hexdigest(),"first_ready_offset":first_offsets[0],"first_ready_line_nonce_sha256":line_hashes[0],"first_shutdown_offset":first_offsets[1],"first_shutdown_line_nonce_sha256":line_hashes[1],"mutation_ready_offset":mutation_offsets[0],"mutation_ready_line_nonce_sha256":line_hashes[2],"mutation_shutdown_offset":mutation_offsets[1],"mutation_shutdown_line_nonce_sha256":line_hashes[3],"final_ready_offset":final_offsets[0],"final_ready_line_nonce_sha256":line_hashes[4],"second_shutdown_offset":final_offsets[1],"second_shutdown_line_nonce_sha256":line_hashes[5]}
    guest["audio_error_count"]=1 if "bad-guest" in r["job_id"] else guest["audio_error_count"]; guest["mutation_ready_line_nonce_sha256"]="0"*64 if "bad-mutation" in r["job_id"] else guest["mutation_ready_line_nonce_sha256"]
    guest_path=pathlib.Path(r["guest_evidence_path"]); guest_path.write_text(json.dumps(guest)+"\n"); "bad-raw" in r["job_id"] and (share/f"t17-network-{prefix}.txt").write_bytes(b"forged")
