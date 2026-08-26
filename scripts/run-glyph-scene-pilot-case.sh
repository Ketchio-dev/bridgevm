#!/usr/bin/env bash
set -euo pipefail
: "${RUN:?}" "${WORK:?}" "${TARGET:?}" "${VARS:?}" "${VIOGPU_DIR:?}"; REPO=$(cd "$(dirname "$0")/.." && pwd); cd "$REPO"
CASE="$RUN"; rm -rf "$WORK"; mkdir -p "$WORK" "$CASE/share"; cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"; chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
for asset in bvgpu-apply-host-resolution.ps1 bv-windows-closure-proof.ps1; do cp "scripts/win-assets/$asset" "$CASE/share/"; done
CTL="$CASE/agent.ctl"; INPUT="$CASE/input.ctl"; : >"$CTL"; : >"$INPUT"; fail(){ echo "FAIL: $*" >&2; exit 1; }
wait_for(){ local pattern="$1" count="$2" timeout="$3" deadline n log; deadline=$((SECONDS+timeout)); while ((SECONDS<deadline)); do log="${RUN:-$CASE}/run.log"; n=$(grep -acE "$pattern" "$log" 2>/dev/null||true); ((n>=count))&&return; kill -0 "$pid" 2>/dev/null||return 1; sleep 1; done; return 1; }
send_ok(){ local cmd="$1" before; before=$(grep -c '^BVAGENT END ' "$CASE/run.log" 2>/dev/null||true); printf '%s\n' "$cmd" >>"$CTL"; wait_for '^BVAGENT END ' $((before+1)) 300 && [[ $(grep '^BVAGENT CMD .* exit=' "$CASE/run.log"|tail -1) == *' exit=0' ]]; }
source scripts/pointer-reliability-vm.sh; trap pointer_vm_cleanup EXIT; pointer_vm_start_until_agent
for asset in bvgpu-apply-host-resolution.ps1 bv-windows-closure-proof.ps1; do wait_for "^BVAGENT SHARE host->guest $asset " 1 120||fail "share timeout $asset"; done
printf 'RESIZE 1600x900\n' >>"$INPUT"; wait_for '^live input accepted: resize=1600x900$' 1 30||fail 'resize not accepted'
send_ok 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bvgpu-apply-host-resolution.ps1 -Width 1600 -Height 900'||fail 'guest resize failed'
send_ok 'powershell -NoProfile -Command "Start-Process notepad.exe; Start-Sleep -Seconds 3; Write-Output BVNOTEPADSTARTED"'||fail 'Notepad launch failed'
before=$(grep -c '^BVAGENT WINLIST WINEND$' "$CASE/run.log" 2>/dev/null||true); printf 'WINLIST\n' >>"$CTL"; wait_for '^BVAGENT WINLIST WINEND$' $((before+1)) 60||fail 'WINLIST failed'
line=$(grep '^BVAGENT WINLIST WIN ' "$CASE/run.log"|while IFS= read -r x; do title=$(awk '{print $10}'<<<"$x"|base64 -D 2>/dev/null||true); [[ "$title" == *Notepad* ]]&&{ echo "$x"; break; }; done||true); hwnd=$(awk '{print $4}'<<<"$line")
[[ "$hwnd" =~ ^[1-9][0-9]*$ ]]||fail 'Notepad HWND absent'
printf 'WINBOUNDS %s 50 60 700 500\nWINFOCUS %s\n' "$hwnd" "$hwnd" >>"$CTL"; wait_for "^BVAGENT WINFOCUS $hwnd -> OK WINFOCUS$" 1 60||fail 'Notepad focus failed'
cmd="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMPtr\\bv-windows-closure-proof.ps1 -Action Window -Hwnd $hwnd"; send_ok "$cmd"||fail 'window proof failed'
grep -Eq "^BVWINDOW hwnd=$hwnd exists=True pid=[0-9]+ rect=50,60,700,500 foreground=$hwnd\r?$" "$CASE/run.log"||fail 'window geometry/foreground mismatch'
for _ in $(seq 1 120); do [[ -s "$CASE/active-scanout.fb.iosurface" ]]&&break; sleep 1; done
[[ -s "$CASE/active-scanout.fb.iosurface" ]]||fail 'active CGL IOSurface absent'
# Inner HID text actions cap at 32 decoded bytes; keep the exact string as 32+32+32.
n=0; for chunk in 427269646765564d20476c7970682050726f6265204142434445464748494a4b 4c4d4e4f505152535455565758595a206162636465666768696a6b6c6d6e6f70 7172737475767778797a20303132333435363738392021402324255e262a2829; do n=$((n+1)); printf 'KEY text-hex:%s\n' "$chunk" >>"$INPUT"; wait_for 'live input accepted: command=Key\(' "$n" 30||fail 'fixed text not accepted'; done; sleep 2
printf 'WINBOUNDS %s 50 60 701 500\n' "$hwnd" >>"$CTL"; wait_for "^BVAGENT WINBOUNDS $hwnd 50 60 701 500 -> OK WINBOUNDS$" 1 60||fail 'intermediate geometry failed'
python3 scripts/capture-active-iosurface.py --iosurface "$CASE/active-scanout.fb.iosurface" --out "$CASE/glyph-scene" --ready "$CASE/glyph-capture.ready" --timeout-ms 10000 --settle-ms 250 >"$CASE/glyph-capture.log" 2>&1 & capture=$!
for _ in $(seq 1 30); do [[ -s "$CASE/glyph-capture.ready" ]]&&break; sleep 1; done; [[ -s "$CASE/glyph-capture.ready" ]]||fail 'capture did not arm'
printf 'WINBOUNDS %s 50 60 700 500\n' "$hwnd" >>"$CTL"; wait_for "^BVAGENT WINBOUNDS $hwnd 50 60 700 500 -> OK WINBOUNDS$" 2 60||fail 'final geometry failed'
send_ok "$cmd"||fail 'final window proof failed'; tr -d '\r' <"$CASE/run.log"|grep '^BVWINDOW '|tail -1|grep -Eq "^BVWINDOW hwnd=$hwnd exists=True pid=[0-9]+ rect=50,60,700,500 foreground=$hwnd$"||fail 'final geometry/foreground mismatch'
wait "$capture"||{ cat "$CASE/glyph-capture.log" >&2; fail 'active CGL capture failed'; }
python3 scripts/glyph_region_analysis.py --ppm "$CASE/glyph-scene/presented.ppm" --out "$CASE/glyph-scene/regions.json"
printf 'scene=notepad-1600x900 hwnd=%s rect=50,60,700,500 foreground=true\nglyph_correctness=unmeasured\n' "$hwnd" >"$CASE/summary.txt"
