#!/usr/bin/env bash
# B4 pointer click reliability gate. Threshold (fixed 2026-08-20, before any
# fix attempt): across N=20 independent APFS clones, one host-injected click
# must land 20/20 -- press AND release consumed by the guest input stack
# (BVPTR lines from bv-pointer-capture.ps1), no stuck button, and a visible
# framebuffer reaction with first-change p95 <= 250 ms. Host 'fired' alone
# never counts. BVPTR separates input-stack consumption from active-scanout
# reaction: BVPTR without reaction indicts foreground/application routing.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

N=${N:-20}
P95_LIMIT_MS=${P95_LIMIT_MS:-250}
OUT=${OUT:-$HOME/BridgeVM/runs/pointer-reliability-$(date +%Y%m%d-%H%M%S)}
TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
DELAYS='5,15,30,60,120,250,500,1000'

parse_run_log() { # fired press release stuck first_ms; edge = press+release
  local log="$1" ptr="${2:-/dev/null}" click="${3:-/dev/null}" base sum first='' fired=false press=0 release=0 edges=0 stuck=1
  grep -q '^B4 pointer target ready: width=1600 height=900 center_x=800 center_y=450$' "$log" 2>/dev/null || { echo 'false 0 0 1 invalid-target'; return; }
  grep -q '^BVPTR begin .* session=[1-9][0-9]* input_desktop_open=1 foreground=[1-9][0-9]* cursor_x=800 cursor_y=450 ' "$ptr" 2>/dev/null || { echo 'false 0 0 1 invalid-desktop'; return; }
  [[ $(tr -d '\r' < "$click" 2>/dev/null | grep -c '^BVTARGET click ' || true) -eq 1 ]] || { echo 'false 0 0 1 invalid-click-count'; return; }
  grep -q '^xHCI pointer-input injection .* fired:' "$log" 2>/dev/null && fired=true
  press=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR press ' || true)
  release=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR release ' || true)
  edges=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR edge ' || true)
  press=$((press + edges)); release=$((release + edges))
  local summary
  summary=$(tr -d '\r' < "$ptr" | grep '^BVPTR summary ' | tail -1 || true)
  [[ "$summary" == *' stuck=0' ]] && stuck=0
  base=$(tr -d '\r' < "$log" | sed -n 's|.*label=pointer-input-before state=captured checksum64=\([^ ]*\) raw=.*/virtio-gpu-checkpoint-pointer-input-before-[^ ]* .*|\1|p' | tail -1)
  if [[ -n "$base" ]]; then
    local ms
    for ms in ${DELAYS//,/ }; do
      sum=$(tr -d '\r' < "$log" | sed -n "s|.*label=pointer-input-delay-${ms}ms state=captured checksum64=\([^ ]*\) raw=.*/virtio-gpu-checkpoint-pointer-input-delay-${ms}ms-[^ ]* .*|\1|p" | tail -1)
      [[ -n "$sum" && "$sum" != "$base" && -z "$first" ]] && first=$ms
    done
  fi
  echo "$fired $press $release $stuck ${first:-none}"
}

if [[ "${1:-}" == "--selftest" ]]; then
  t=$(mktemp); p=$(mktemp); c=$(mktemp); printf 'BVTARGET click count=1\r\n' > "$c"
  printf 'B4 pointer target ready: width=1600 height=900 center_x=800 center_y=450\nxHCI pointer-input injection 1 fired: ok\nramfb checkpoint: label=pointer-input-before state=captured checksum64=aa raw=/x/virtio-gpu-checkpoint-pointer-input-before-0000.xrgb8888 ppm=x\nramfb checkpoint: label=pointer-input-delay-5ms state=captured checksum64=aa raw=/x/virtio-gpu-checkpoint-pointer-input-delay-5ms-0000.xrgb8888 ppm=x\nramfb checkpoint: label=pointer-input-delay-250ms state=captured checksum64=bb raw=/x/virtio-gpu-checkpoint-pointer-input-delay-250ms-0000.xrgb8888 ppm=x\n' > "$t"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR press t_ms=3 x=1 y=2 fg=9\r\nBVPTR release t_ms=40 x=1 y=2 fg=9\r\nBVPTR summary presses=1 releases=1 edges=0 first_press_ms=3 first_release_ms=40 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 1 1 0 250" ]] || fail "selftest good-run parse: got '$r'"
  : > "$c"; r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "false 0 0 1 invalid-click-count" ]] || fail "selftest missing-click parse: got '$r'"
  printf 'BVTARGET click count=1\r\n' > "$c"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR edge t_ms=12\r\nBVPTR summary presses=0 releases=0 edges=1 first_press_ms=-1 first_release_ms=-1 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 1 1 0 250" ]] || fail "selftest edge-run parse: got '$r'"
  printf 'B4 pointer target ready: width=1600 height=900 center_x=800 center_y=450\nxHCI pointer-input injection 1 fired: ok\nramfb checkpoint: label=pointer-input-before state=captured checksum64=aa raw=/x/virtio-gpu-checkpoint-pointer-input-before-0000.xrgb8888 ppm=x\nramfb checkpoint: label=pointer-input-delay-1000ms state=captured checksum64=aa raw=/x/virtio-gpu-checkpoint-pointer-input-delay-1000ms-0000.xrgb8888 ppm=x\n' > "$t"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR summary presses=0 releases=0 edges=0 first_press_ms=-1 first_release_ms=-1 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 0 0 0 none" ]] || fail "selftest lost-click parse: got '$r'"
  rm -f "$t" "$p" "$c"; echo "pointer reliability parser: PASS (selftest)"; exit 0
fi

mkdir -p "$OUT"
trap 'rm -rf "$OUT"/work*' EXIT # cancelled clones trip the free-space guard
landed=0; declare -a firsts=()
for i in $(seq 1 "$N"); do
  run="$OUT/run$i"; work="$OUT/work$i"
  RUN="$run" WORK="$work" TARGET="$TARGET" VARS="$VARS" VIOGPU_DIR="$VIOGPU_DIR" DELAYS="$DELAYS" \
    bash scripts/run-pointer-click-reliability-case.sh || true
  read -r fired press release stuck first <<<"$(parse_run_log "$run/run.log" "$run/share/bvptr.log" "$run/share/bv-pointer-target-click.log")"
  ok=false
  [[ "$fired" == true && "$press" -ge 1 && "$release" -ge 1 && "$stuck" == 0 && "$first" != none ]] && { ok=true; landed=$((landed+1)); firsts+=("$first"); }
  echo "run $i: fired=$fired press=$press release=$release stuck=$stuck first_changed_ms=$first landed=$ok" | tee -a "$OUT/summary.txt"
  rm -rf "$work"
done

p95=none
if [[ ${#firsts[@]} -gt 0 ]]; then
  p95=$(printf '%s\n' "${firsts[@]}" | sort -n | awk -v n="${#firsts[@]}" 'NR==int((n*95+99)/100){print; exit}')
fi
echo "landed $landed/$N p95_first_changed_ms=$p95 (limit $P95_LIMIT_MS)" | tee -a "$OUT/summary.txt"
[[ "$landed" -eq "$N" && "$p95" != none && "$p95" -le "$P95_LIMIT_MS" ]] || fail "B4 gate not met (landed $landed/$N, p95=$p95)"
echo "B4 pointer reliability: PASS"
