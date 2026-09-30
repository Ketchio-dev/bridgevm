#!/bin/bash
# The final HVF report's stop record, bound to host framing rather than text.
# The packaged installed-boot runner has no Python, so this applies
# scripts/live-gates/hvf_terminal_report.py's grammar with grep byte offsets:
# only host teardown records follow the last `--- end ---`, exactly one
# `serial raw bytes: R output bytes: M` count owns the M-byte tail ending at
# it, and the stop record follows the last banner before that count.
#   hvf-terminal-report.sh --require-system-off RUN_LOG
# exits 0 only when the whole RUN_LOG, read after the helper exited, binds
# `stop: PSCI 0x84000008 (system off)`; 1 otherwise and 2 on bad usage.
# No pipefail: a reader that stops early must not turn a mismatch into success.
set -u
export LC_ALL=C
[[ $# -eq 2 && "$1" == --require-system-off ]] \
  || { echo 'usage: hvf-terminal-report.sh --require-system-off RUN_LOG' >&2; exit 2; }
log=$2 banner='=== EDK2 boot probe (with Apple hv_gic) ===' serial='--- serial (tail) ---'
[[ -f "$log" && ! -L "$log" ]] || exit 1
size=$(wc -c < "$log") && size=$((size)) && (( size < 100000000 )) || exit 1
line_at() { tail -c "+$(($1 + 1))" "$log" | head -n 1 | grep -a -q -x -F -- "$2"; }
footer=$(grep -a -b -x -F -- '--- end ---' "$log" | tail -n 1) && footer=${footer%%:*}
[[ -n "$footer" ]] && (( footer >= 1 && footer + 12 <= size && size - footer - 12 <= 4096 )) || exit 1
if (( size > footer + 12 )); then
  [[ -z "$(tail -c 1 "$log")" ]] && (( $(tail -c "$((size - footer - 12))" "$log" | grep -a -c '') <= 16 )) || exit 1
  ! tail -c "$((size - footer - 12))" "$log" | grep -a -q -v -x -E \
    -e 'hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)' \
    -e 'hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)' \
    -e 'hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*' \
    -e '[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  virgl_render_server\[[0-9]+\] <Debug>: socket disconnected' \
    || exit 1
fi
count="" counts=0
while IFS=: read -r offset text; do
  [[ "$text" =~ ^serial\ raw\ bytes:\ ([0-9]+)\ output\ bytes:\ ([0-9]+)$ ]] || exit 1
  marker=$((offset + ${#text} + 1)) raw=$((10#${BASH_REMATCH[1]})) output=$((10#${BASH_REMATCH[2]}))
  (( raw <= output && output == footer - marker - ${#serial} - 2 )) && line_at "$marker" "$serial" || continue
  count=$offset counts=$((counts + 1))
done < <(grep -a -b -x -E 'serial raw bytes: [0-9]{1,8} output bytes: [0-9]{1,8}' "$log")
(( counts == 1 )) || exit 1
found=$(head -c "$count" "$log" | grep -a -b -x -F -- "$banner" | tail -n 1) && found=${found%%:*}
[[ -n "$found" ]] && (( found >= 1 && found + ${#banner} + 36 <= count )) || exit 1
line_at "$((found + ${#banner} + 1))" 'stop: PSCI 0x84000008 (system off)'
