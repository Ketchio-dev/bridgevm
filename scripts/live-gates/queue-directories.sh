#!/usr/bin/env bash
# Create private queue directories regardless of caller umask; refuse leaf aliases.
set -euo pipefail
root="$(python3 -I -B "$(dirname "$0")/queue-root-path.py" "${1:?queue root required}")"
for dir in "$root" "$root/queued" "$root/running" "$root/done" "$root/job-ledger"; do
    if [[ ! -e "$dir" && ! -L "$dir" ]]; then
        (umask 077; mkdir -p "$dir")
    fi
    [[ -d "$dir" && ! -L "$dir" && -O "$dir" ]] || {
        echo "queue directory must be a real owned directory" >&2; exit 1;
    }
    chmod 700 "$dir"
done
