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

parse_run_log() { # fired press release stuck first_ms; edge = press+release
  local log="$1" ptr="${2:-/dev/null}" click="${3:-/dev/null}" first='' fired=false press=0 release=0 edges=0 stuck=1 ready cx cy hid_x hid_y env
  ready=$(tr -d '\r' < "$(dirname "$log")/share/bv-pointer-target-ready.log" 2>/dev/null || true)
  [[ "$ready" =~ ^BVTARGET.ready.width=1600.height=900.screen_x=([-0-9]+).screen_y=([-0-9]+).center_x=([-0-9]+).center_y=([-0-9]+).virtual_x=([-0-9]+).virtual_y=([-0-9]+).virtual_w=([0-9]+).virtual_h=([0-9]+).hwnd=([1-9][0-9]*)$ ]] || { echo 'false 0 0 1 invalid-target'; return; }
  cx=${BASH_REMATCH[3]}; cy=${BASH_REMATCH[4]}; env="$(dirname "$log")/visible/visible.env"
  hid_x=$(awk -F= '$1=="hid_x"{print $2}' "$env" 2>/dev/null); hid_y=$(awk -F= '$1=="hid_y"{print $2}' "$env" 2>/dev/null)
  grep -q "^BVPTR begin .* session=[1-9][0-9]* input_desktop_open=1 foreground=[1-9][0-9]* cursor_x=$cx cursor_y=$cy " "$ptr" 2>/dev/null || { echo 'false 0 0 1 invalid-desktop'; return; }
  [[ $(tr -d '\r' < "$click" 2>/dev/null | grep -c '^BVTARGET click ' || true) -eq 1 ]] || { echo 'false 0 0 1 invalid-click-count'; return; }
  [[ -n "$hid_x" && -n "$hid_y" && $(grep -Fxc "live input accepted: command=Pointer(\"click:${hid_x}x${hid_y}\")" "$log" 2>/dev/null || true) -eq 1 ]] && fired=true
  press=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR press ' || true)
  release=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR release ' || true)
  edges=$(tr -d '\r' < "$ptr" | grep -c '^BVPTR edge ' || true)
  press=$((press + edges)); release=$((release + edges))
  local summary
  summary=$(tr -d '\r' < "$ptr" | grep '^BVPTR summary ' | tail -1 || true)
  [[ "$summary" == *' moves=0 '* && "$summary" == *' stuck=0' ]] && stuck=0
  tr -d '\r' < "$ptr" | grep -E '^BVPTR (press|release) ' | grep -qv " x=$cx y=$cy " && { echo 'false 0 0 1 invalid-coordinate'; return; }
  first=$(awk -F= '$1=="source"{s=$2} $1=="first_changed_ms"{m=$2} END{if(s=="active-cgl-iosurface")print m}' "$env" 2>/dev/null)
  echo "$fired $press $release $stuck ${first:-none}"
}

# Why a run did not land. A run whose target scene never reached the active
# surface never received an injected click at all, so counting it as a lost
# pointer report misattributes a presentation failure to input. This only
# labels an already-failed run; it never converts one into a pass.
classify_failure() { # run-dir -> cause
  local baseline="$1/visible/baseline.env" peak
  if [[ -f "$baseline" ]]; then
    peak=$(awk -F= '$1=="peak_white_px"{print $2}' "$baseline" 2>/dev/null)
    [[ "$peak" == 0 ]] && { echo scene-never-presented; return; }
    echo scene-unsettled; return
  fi
  [[ -f "$1/visible/visible.env" ]] || { echo scene-absent; return; }
  echo pointer-reaction
}

if [[ "${1:-}" == "--selftest" ]]; then
  d=$(mktemp -d); t="$d/run.log"; p="$d/bvptr.log"; c="$d/click.log"; mkdir "$d/visible" "$d/share"; printf 'BVTARGET click count=1\r\n' > "$c"
  printf 'BVTARGET ready width=1600 height=900 screen_x=0 screen_y=0 center_x=800 center_y=450 virtual_x=0 virtual_y=0 virtual_w=1600 virtual_h=900 hwnd=9\r\n' > "$d/share/bv-pointer-target-ready.log"
  printf 'source=active-cgl-iosurface\nhid_x=16384\nhid_y=16384\nfirst_changed_ms=250\n' > "$d/visible/visible.env"
  printf 'live input accepted: command=Pointer("click:16384x16384")\n' > "$t"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR press t_ms=3 x=800 y=450 fg=9\r\nBVPTR release t_ms=40 x=800 y=450 fg=9\r\nBVPTR summary presses=1 releases=1 edges=0 moves=0 first_press_ms=3 first_release_ms=40 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 1 1 0 250" ]] || fail "selftest good-run parse: got '$r'"
  : > "$c"; r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "false 0 0 1 invalid-click-count" ]] || fail "selftest missing-click parse: got '$r'"
  printf 'BVTARGET click count=1\r\n' > "$c"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR edge t_ms=12\r\nBVPTR summary presses=0 releases=0 edges=1 moves=0 first_press_ms=-1 first_release_ms=-1 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 1 1 0 250" ]] || fail "selftest edge-run parse: got '$r'"
  printf 'source=active-cgl-iosurface\nhid_x=16384\nhid_y=16384\nfirst_changed_ms=none\n' > "$d/visible/visible.env"
  printf 'live input accepted: command=Pointer("click:16384x16384")\n' > "$t"
  printf 'BVPTR begin duration_ms=20 poll_ms=4 session=1 input_desktop_open=1 foreground=9 cursor_x=800 cursor_y=450 utc=x\r\nBVPTR summary presses=0 releases=0 edges=0 moves=0 first_press_ms=-1 first_release_ms=-1 stuck=0\r\n' > "$p"
  r=$(parse_run_log "$t" "$p" "$c")
  [[ "$r" == "true 0 0 0 none" ]] || fail "selftest lost-click parse: got '$r'"
  c=$(classify_failure "$d"); [[ "$c" == pointer-reaction ]] || fail "selftest reaction cause: got '$c'"
  printf 'result=baseline-not-presented\nsamples=544\nfinal_white_px=0\npeak_white_px=0\n' > "$d/visible/baseline.env"
  c=$(classify_failure "$d"); [[ "$c" == scene-never-presented ]] || fail "selftest never-presented cause: got '$c'"
  printf 'result=baseline-not-presented\nsamples=544\nfinal_white_px=3\npeak_white_px=12000\n' > "$d/visible/baseline.env"
  c=$(classify_failure "$d"); [[ "$c" == scene-unsettled ]] || fail "selftest unsettled cause: got '$c'"
  rm -f "$d/visible/baseline.env" "$d/visible/visible.env"
  c=$(classify_failure "$d"); [[ "$c" == scene-absent ]] || fail "selftest absent cause: got '$c'"
  rm -rf "$d"; echo "pointer reliability parser: PASS (selftest)"; exit 0
fi

mkdir -p "$OUT"
trap 'rm -rf "$OUT"/work*' EXIT # cancelled clones trip the free-space guard
landed=0; declare -a firsts=()
for i in $(seq 1 "$N"); do
  run="$OUT/run$i"; work="$OUT/work$i"
  RUN="$run" WORK="$work" TARGET="$TARGET" VARS="$VARS" VIOGPU_DIR="$VIOGPU_DIR" \
    bash scripts/run-pointer-click-reliability-case.sh || true
  read -r fired press release stuck first <<<"$(parse_run_log "$run/run.log" "$run/share/bvptr.log" "$run/share/bv-pointer-target-click.log")"
  ok=false; cause=none
  [[ "$fired" == true && "$press" -ge 1 && "$release" -ge 1 && "$stuck" == 0 && "$first" =~ ^[0-9]+$ ]] && { ok=true; landed=$((landed+1)); firsts+=("$first"); }
  [[ "$ok" == true ]] || cause=$(classify_failure "$run")
  echo "run $i: fired=$fired press=$press release=$release stuck=$stuck first_changed_ms=$first landed=$ok cause=$cause" | tee -a "$OUT/summary.txt"
  rm -rf "$work"
done

p95=none
if [[ ${#firsts[@]} -gt 0 ]]; then
  p95=$(printf '%s\n' "${firsts[@]}" | sort -n | awk -v n="${#firsts[@]}" 'NR==int((n*95+99)/100){print; exit}')
fi
echo "landed $landed/$N p95_first_changed_ms=$p95 (limit $P95_LIMIT_MS)" | tee -a "$OUT/summary.txt"
[[ "$landed" -eq "$N" && "$p95" != none && "$p95" -le "$P95_LIMIT_MS" ]] || fail "B4 gate not met (landed $landed/$N, p95=$p95)"
echo "B4 pointer reliability: PASS"
