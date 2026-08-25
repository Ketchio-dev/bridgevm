"""Strict per-run audio teardown summary contract."""
KEYS={"out_dir","a5_audio","frames_rendered","drops","callback_errors","unexpected_callback_errors","teardown_reenqueue_refusals","stop_errors","dispose_errors","guest_sound_device_status"}
ERROR_KEYS=("unexpected_callback_errors","stop_errors","dispose_errors")

def read(path):
    pairs=[line.split("=",1) for line in path.read_text().splitlines() if "=" in line]
    if len(pairs)!=len(KEYS) or {key for key,_ in pairs}!=KEYS: raise ValueError("audio summary schema mismatch")
    return dict(pairs)

def passes(row):
    callback=int(row["callback_errors"])
    return row["a5_audio"]=="pass" and int(row["frames_rendered"])>0 and row["drops"]=="0" and callback==0 and callback==sum(int(row[key]) for key in ERROR_KEYS)
