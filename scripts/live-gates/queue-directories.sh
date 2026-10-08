#!/usr/bin/env bash
# Create private queue directories regardless of caller umask; refuse leaf aliases.
set -euo pipefail
root="${1:?queue root required}"
case "$root" in */|.|..|*/.|*/..) echo "queue root needs an unambiguous directory leaf" >&2; exit 1;; esac
for dir in "$root" "$root/queued" "$root/running" "$root/done" "$root/job-ledger"; do
    if [[ ! -e "$dir" && ! -L "$dir" ]]; then
        (umask 077; mkdir -p "$dir")
    fi
    [[ -d "$dir" && ! -L "$dir" && -O "$dir" ]] || {
        echo "queue directory must be a real owned directory" >&2; exit 1;
    }
    chmod 700 "$dir"
done
