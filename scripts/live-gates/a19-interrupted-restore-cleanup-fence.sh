#!/usr/bin/env bash
# Keep private T22 media and its worktree when cleanup or the seal is uncertain.

bridgevm_t22_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6" verifier
    [[ "$tier" == t22-a19-interrupted-restore ]] || return 0
    verifier="$worktree/scripts/live-gates/a19_interrupted_restore_receipt.py"
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
spec = importlib.util.spec_from_file_location("a19_interrupt_receipt", verifier)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
value = module.validate(module.load_receipt(root / "receipt.json"), commit)
module.validate_seal(value, root)
clean = value["job_id"] == job_id and value["worker_cleanup_verified"] is True
raise SystemExit(0 if clean and not any(os.path.lexists(root / name)
                                      for name in ("prepared-inputs", "live")) else 1)
PY
    then
        return 0
    fi
    printf 'T22 job %s has unverified private-media cleanup\n' "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
