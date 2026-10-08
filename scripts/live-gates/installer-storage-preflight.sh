#!/usr/bin/env bash
# Missing prospective directories permit only a labelled ancestor estimate.
set -euo pipefail
repo="$1"; queue="$2"; work="$3"; minimum="$4"
status=0
available="$(python3 -I -B "$repo/scripts/live-gates/live_storage_capacity.py" --estimate --minimum "$minimum" "$queue" "$work")" || status=$?
[[ "$status" == 0 || "$status" == 3 ]] && [[ "$available" =~ ^(0|[1-9][0-9]*)$ ]] || {
    echo "preflight: storage capacity unavailable or configuration invalid" >&2; exit 1;
}
if [[ "$status" == 3 ]]; then
    echo "warning: ${available}GiB free, below the ${minimum}GiB job guard (provisional queue/work estimate)"
else
    echo "free space: ${available}GiB (guard ${minimum}GiB; provisional queue/work estimate)"
fi
