#!/usr/bin/env bash
# Explicit refusal: caller run_job is conditional, so its errexit is ineffective.
set -euo pipefail
repo="$1"; job="$2"; work="$3"; minimum="$4"
refuse() { printf 'result=%s\n' "$1" > "$job/result.env"; exit 1; }
status=0
available="$(python3 -I -B "$repo/scripts/live-gates/live_storage_capacity.py" --prepare-work --minimum "$minimum" "$job" "$work")" || status=$?
case "$status" in
    0) [[ "$available" =~ ^(0|[1-9][0-9]*)$ ]] || refuse refused-storage-unavailable ;;
    3) printf 'result=refused-free-space\navailable_gib=%s\n' "$available" > "$job/result.env"; exit 1 ;;
    *) refuse refused-storage-unavailable ;;
esac
