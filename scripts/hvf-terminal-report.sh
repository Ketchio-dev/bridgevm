#!/bin/bash
# The final HVF report's host records, bound to host framing rather than text. The packaged
# installed-boot runner has no Python, so this applies the grammar of live-gates/hvf_terminal_report.py
# and hvf_host_media.py with grep byte offsets: only hvf_host_tail.py's host records follow the last
# `--- end ---`, one `serial raw bytes: R output bytes: M` count owns the M-byte tail ending there, the
# stop record follows the last banner before it, and `host media: ` records follow the stop. Exits 0
# only when the whole RUN_LOG, read after the helper exited, binds SYSTEM_OFF or one NVMe disk
# write-back after an ASCII stop. No pipefail: a reader that stops early must not turn a mismatch into success.
set -u; export LC_ALL=C
[[ $# -eq 2 && "$1" =~ ^--require-(system-off|nvme-write-back)$ ]] \
  || { echo 'usage: hvf-terminal-report.sh --require-system-off|--require-nvme-write-back RUN_LOG' >&2; exit 2; }
log=$2 banner='=== EDK2 boot probe (with Apple hv_gic) ===' serial='--- serial (tail) ---' ms='(0|[1-9][0-9]{0,19})\.[0-9]{3}' n='(0|[1-9][0-9]{0,19})'
[[ -f "$log" && ! -L "$log" ]] || exit 1
size=$(wc -c < "$log") && size=$((size)) && (( size < 100000000 )) || exit 1
line_at() { tail -c "+$(($1 + 1))" "$log" | head -n 1 | grep -a -q -x "${3:--F}" -- "$2"; }
footer=$(grep -a -b -x -F -- '--- end ---' "$log" | tail -n 1) && footer=${footer%%:*}
[[ -n "$footer" ]] && (( footer >= 1 && footer + 12 <= size && size - footer - 12 <= 4096 )) || exit 1
[[ -z "$(tail -c 1 "$log")" ]] && (( $(tail -c "$((size - footer - 12))" "$log" | grep -a -c '') <= 16 )) || exit 1
! tail -c "$((size - footer - 12))" "$log" | grep -a -q -v -x -E \
  -e 'hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)' -e 'hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)' \
  -e 'hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*' \
  -e "hda CoreAudio continuity: active_callbacks=$n underrun_callbacks=$n underrun_frames=$n contention_callbacks=$n gaps=$n max_gap_frames=$n stream_stops=$n callback_frames=$n" \
  -e "hda poll: polls=$n gap_le_10ms=$n gap_le_20ms=$n gap_le_50ms=$n gap_le_100ms=$n gap_gt_100ms=$n max_gap_ms=$n capped_polls=$n capped_lost_ms=$n ioc_completions=$n max_iocs_per_poll=$n msis_sent=$n" \
  -e "hda stream: runs=$n rate_hz=$n channels=$n bits=$n cbl_bytes=$n cbl_ms=$ms entries=$n entry_bytes_min=$n entry_bytes_max=$n entry_ms_min=$ms entry_ms_max=$ms ioc_entries=$n position_buffer=([01]) lpib_reads=$n running_ms=$n" \
  -e "hda repeat: passes=$n silent_passes=$n compared_passes=$n repeat_passes=$n max_repeat_run=$n" \
  -e "hda CoreAudio timing: callbacks=$n interval_le_10ms=$n interval_le_15ms=$n interval_le_20ms=$n interval_le_30ms=$n interval_le_50ms=$n interval_gt_50ms=$n max_interval_ms=$n late_callbacks=$n trylock_misses=$n ring_samples=$n ring_min_ms=$n ring_max_ms=$n ring_final_ms=$n" \
  -e '[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  virgl_render_server\[[0-9]+\] <Debug>: socket disconnected' || exit 1
[[ "$(tail -c "$((size - footer - 12))" "$log" | sed -n -E 's/^hda CoreAudio stats:.*/S/p;s/^hda CoreAudio continuity:.*/C/p;s/^hda poll:.*/P/p;s/^hda stream:.*/T/p;s/^hda repeat:.*/R/p;s/^hda CoreAudio timing:.*/M/p' | tr -d '\n')" =~ ^P?T?R?M?(S+C)?S*$ ]] || exit 1
count="" counts=0
while IFS=: read -r offset text; do
  [[ "$text" =~ ^serial\ raw\ bytes:\ ([0-9]+)\ output\ bytes:\ ([0-9]+)$ ]] || exit 1
  marker=$((offset + ${#text} + 1)) raw=$((10#${BASH_REMATCH[1]})) output=$((10#${BASH_REMATCH[2]}))
  (( raw <= output && output == footer - marker - ${#serial} - 2 )) && line_at "$marker" "$serial" || continue
  count=$offset counts=$((counts + 1))
done < <(grep -a -b -x -E 'serial raw bytes: [0-9]{1,8} output bytes: [0-9]{1,8}' "$log")
(( counts == 1 )) && found=$(head -c "$count" "$log" | grep -a -b -x -F -- "$banner" | tail -n 1) && found=${found%%:*} || exit 1
stop=$((found + ${#banner} + 1)) && [[ -n "$found" ]] && (( found >= 1 && stop < count )) || exit 1
[[ "$1" == --require-system-off ]] && { line_at "$stop" 'stop: PSCI 0x84000008 (system off)'; exit; }
line_at "$stop" 'stop: [ -~]*' -E && (( $(tail -c "+$((stop + 1))" "$log" | head -c "$((count - stop))" | sed -n -E \
  '1d;/^host media: (UEFI vars|NVMe disk|NVMe target namespace \(NSID 2\)) (written back|snapshot written): [ -~]+ \([0-9]+ bytes\)$/!q;p' \
  | grep -a -c -x -E 'host media: NVMe disk written back: [ -~]+ \([0-9]+ bytes\)') == 1 ))
