#!/usr/bin/env bash
# Keep a T20 job in running when its private-media cleanup cannot be proven.

bridgevm_t20_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6" verifier
    [[ "$tier" == t20-a19-native-snapshot-restore ]] || return 0
    verifier="$worktree/scripts/live-gates/native_snapshot_restore_receipt.py"
    if [[ -d "$worktree" && ! -L "$worktree" && -d "$dir" && ! -L "$dir" \
        && -f "$verifier" && ! -L "$verifier" \
        && -f "$dir/receipt.json" && ! -L "$dir/receipt.json" \
        && "$(git -C "$worktree" rev-parse HEAD 2>/dev/null)" == "$commit" ]] && \
        python3 - "$verifier" "$dir" "$commit" "$job_id" >/dev/null 2>&1 <<'PY'
import importlib.util
import os
from pathlib import Path
import sys
verifier, directory, commit, job_id = sys.argv[1:]
root = Path(directory)
sys.path.insert(0, str(Path(verifier).parent))
from native_snapshot_restore_public import load_receipt
from native_snapshot_restore_seal import validate_seal
spec = importlib.util.spec_from_file_location("native_snapshot_receipt", verifier)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
value = module.validate(load_receipt(root / "receipt.json"), commit)
validate_seal(value, root)
clean = value["job_id"] == job_id and value["worker_cleanup_verified"] is True
raise SystemExit(0 if clean and not any(os.path.lexists(root / name)
                                      for name in ("prepared-inputs", "live")) else 1)
PY
    then
        return 0
    fi
    printf 'T20 job %s has unverified private-media cleanup\n' "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
