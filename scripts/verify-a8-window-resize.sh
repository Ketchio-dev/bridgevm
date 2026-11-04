#!/usr/bin/env bash
# A8 live check of the product path on one VM. The guest must already have the
# Microsoft-signed virtio-win display driver (installed or staged).
#
# Boot a clone with the 2D virtio-gpu (`--virtio-gpu`, the app's launch for a
# VM with virtioDisplayDriverInstalled). Write only `RESIZE WxH` lines to the
# live input file, as the app does when its window is resized. For each size,
# check that:
# - the guest's applier reports exactly that mode (`BVDISPLAY ... result=0`);
# - the host display file holds a frame of exactly that size.
#
# After a guest restart the last size is sent again, as the app does after
# `BVAGENT SERVICE start`. Prints one `BVA8` line per check and exits 1 if any
# check fails.
set -euo pipefail
CALLER=$PWD
# Paths given relative to the caller stay relative to the caller after the cd below.
abs() { case $1 in /*) printf '%s\n' "$1" ;; *) printf '%s/%s\n' "$CALLER" "$1" ;; esac; }
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:?Windows disk with the signed display driver (it is cloned, never modified)}
TARGET=$(abs "$TARGET")
VARS=${VARS:?its UEFI variables (copied)}
VARS=$(abs "$VARS")
PROBE=${PROBE:?prebuilt release probe}
PROBE=$(abs "$PROBE")
OUT=${OUT:?evidence directory}
OUT=$(abs "$OUT")
REQUESTS=${REQUESTS:-1517x931 1600x900 1920x1080}
GUEST_EPOCH=${GUEST_EPOCH:-}   # optional: boot a date-bound test image at a fixed date
BOOT_TIMEOUT=${BOOT_TIMEOUT:-600}
STEP_TIMEOUT=${STEP_TIMEOUT:-60}

mkdir -p "$OUT"
WORK=$OUT/work; mkdir -p "$WORK/bin"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"
chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"   # staged sources are read-only
cp "$PROBE" "$WORK/bin/hvf_gic_boot_probe"
codesign --sign - --force --entitlements apps/macos/HvfRunner.entitlements "$WORK/bin/hvf_gic_boot_probe"
CTL=$OUT/agent.ctl; INPUT=$OUT/input.ctl; FB=$OUT/display.fb
: > "$CTL"; : > "$INPUT"
EVIDENCE=$OUT/hvf; RUN_LOG=$EVIDENCE/run.log
RESULTS=$OUT/results.txt; : > "$RESULTS"

log() { tr -d '\r' 2>/dev/null < "$RUN_LOG" || true; }
count() { log | grep -cE "$1" || true; }
wait_for() {   # regex, count, seconds
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(count "$1"); (( ${n:-0} >= $2 )) && return 0
    sleep 0.3
  done
  return 1
}
# Width x height of the newest frame in display.fb; optionally save it as PNG.
fb() {
  python3 - "$FB" "${1:-}" <<'PY'
import struct, sys, zlib
path, png = sys.argv[1], sys.argv[2]
try:
    f = open(path, "rb"); head = f.read(64)
except OSError:
    print("none"); sys.exit()
if len(head) < 64 or struct.unpack_from("<I", head, 0)[0] != 0x42564642:
    print("none"); sys.exit()
_, _, w, h, stride = struct.unpack_from("<5I", head, 0)
print(f"{w}x{h}")
data = f.read(stride * h) if png and w and h else b""
if png and len(data) >= stride * h:
    rows = bytearray()
    for y in range(h):
        px = data[y * stride:y * stride + w * 4]   # XRGB8888 little-endian: B, G, R, X
        rgb = bytearray(w * 3); rgb[0::3] = px[2::4]; rgb[1::3] = px[1::4]; rgb[2::3] = px[0::4]
        rows += b"\0" + rgb
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    open(png, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                          + chunk(b"IDAT", zlib.compress(bytes(rows), 6)) + chunk(b"IEND", b""))
PY
}
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }
# Milliseconds from the RESIZE write (arg 2) to the last frame published at the
# requested size, once no frame has followed for 1.5 s; -1 if no frame at that
# size came within 20 s, and "timeout" if frames never went quiet for 1.5 s.
# The first frames at a new size can still show the old desktop in the old
# area while Windows repaints.
settle_ms() {
  python3 - "$FB" "$1" "$2" <<'PY'
import struct, sys, time
path, want, t0 = sys.argv[1], sys.argv[2], int(sys.argv[3])
last_seq = last_change = None
settled = False
deadline = time.time() + 20
while time.time() < deadline:
    try:
        with open(path, "rb") as f:
            head = f.read(64)
        _, _, w, h, _ = struct.unpack_from("<5I", head, 0)
        seq = struct.unpack_from("<Q", head, 24)[0]
    except (OSError, struct.error):
        time.sleep(0.1); continue
    now = int(time.time() * 1000)
    if f"{w}x{h}" == want and seq % 2 == 0:
        if seq != last_seq:
            last_seq, last_change = seq, now
        elif now - last_change >= 1500:
            settled = True
            break
    time.sleep(0.1)
print(last_change - t0 if settled else ("timeout" if last_change else -1))
PY
}
check() {   # boot, request
  local boot=$1 request=$2 pattern n deadline size="none" guest="none" verdict=FAIL t0 settle
  pattern="^BVDISPLAY target=$request .*after=$request .*result=0"
  n=$(count "$pattern"); n=${n:-0}
  t0=$(now_ms)
  printf 'RESIZE %s\n' "$request" >> "$INPUT"
  if wait_for "$pattern" $((n + 1)) "$STEP_TIMEOUT"; then guest=$request; fi
  deadline=$((SECONDS + 15))
  while (( SECONDS < deadline )); do
    size=$(fb); [[ "$size" == "$request" ]] && break
    sleep 0.5
  done
  fb "$OUT/boot$boot-$request.png" > /dev/null
  [[ "$guest" == "$request" && "$size" == "$request" ]] && verdict=PASS
  settle=$(settle_ms "$request" "$t0")
  fb "$OUT/boot$boot-$request-settled.png" > /dev/null   # after the guest's repaint
  echo "BVA8 boot=$boot request=$request guest=$guest display_fb=$size settle_ms=$settle $verdict" | tee -a "$RESULTS"
}

# display.fb once a second for the whole run: shows who presented at boot.
( while :; do echo "$SECONDS $(fb)"; sleep 1; done ) > "$OUT/fb-samples.txt" 2>/dev/null &
SAMPLER=$!

LANE_PROBE=$WORK/bin/hvf_gic_boot_probe
lane_vm_running() { pgrep -f "^$LANE_PROBE( |\$)" > /dev/null; }
# Stops this lane's VM: TERM the launcher (it stops its own probe), then TERM and
# finally KILL the probe copy under $WORK, whose path is unique to this run.
stop_vm() {
  kill -TERM "$LAUNCHER" 2>/dev/null || true
  for _ in $(seq 1 60); do
    kill -0 "$LAUNCHER" 2>/dev/null || lane_vm_running || break
    sleep 0.5
  done
  if lane_vm_running; then pkill -TERM -f "^$LANE_PROBE( |\$)" || true; sleep 5; fi
  if lane_vm_running || kill -0 "$LAUNCHER" 2>/dev/null; then
    pkill -KILL -f "^$LANE_PROBE( |\$)" || true
    kill -KILL "$LAUNCHER" 2>/dev/null || true
    sleep 1
  fi
  wait "$LAUNCHER" 2>/dev/null || true
  ! lane_vm_running
}
# The clone is deleted only once this lane's VM is gone.
cleanup() {
  kill "$SAMPLER" 2>/dev/null || true
  if ! stop_vm; then echo "FAIL: this lane VM is still running; kept $WORK" >&2; return 1; fi
  [[ "${KEEP_WORK:-0}" == 1 ]] || rm -rf "${WORK:?}"
}

ENV=(BRIDGEVM_PREBUILT_PROBE="$LANE_PROBE")
[[ -z "$GUEST_EPOCH" ]] || ENV+=(BRIDGEVM_GUEST_RTC_EPOCH="$GUEST_EPOCH")
env "${ENV[@]}" scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$EVIDENCE" --release --skip-build --no-guest-disk-harvest \
  --watchdog-ms $((BOOT_TIMEOUT * 4000)) --ram-mib 6144 --smp-cpus 4 --enable-xhci \
  --virtio-gpu --display-export-fb "$FB" --input-control "$INPUT" \
  --agent-service-control "$CTL" > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap cleanup EXIT

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" || fail "agent service timeout"
echo "boot 1: agent up at ${SECONDS}s, display.fb $(fb "$OUT/boot1-start.png")"
last=""
for request in $REQUESTS; do check 1 "$request"; last=$request; done

printf '%s\n' 'shutdown /r /t 2' >> "$CTL"
wait_for '^BVAGENT SERVICE start' 2 "$BOOT_TIMEOUT" || fail "agent did not return after the restart"
sleep 3
echo "boot 2: agent up at ${SECONDS}s, display.fb before the resend $(fb "$OUT/boot2-start.png")"
check 2 "$last"

printf '%s\n' 'shutdown /s /t 2' >> "$CTL"
shutdown=clean
for _ in $(seq 1 240); do kill -0 "$LAUNCHER" 2>/dev/null || break; sleep 0.5; done
if kill -0 "$LAUNCHER" 2>/dev/null; then
  shutdown=timeout
  echo "FAIL: Windows did not power off within 120 s; stopping this lane VM" >&2
else
  launcher_status=0; wait "$LAUNCHER" || launcher_status=$?
  if (( launcher_status != 0 )); then
    shutdown=launcher-exit-$launcher_status
    echo "FAIL: the launcher exited with status $launcher_status (see $OUT/launcher.out)" >&2
  fi
fi
trap - EXIT
cleanup || shutdown=vm-still-running
log | grep -aE '^(BVDISPLAY|live input accepted: resize=|BVAGENT SERVICE start)' > "$OUT/display-lines.txt" || true
passed=$(grep -c ' PASS$' "$RESULTS" || true); total=$(wc -l < "$RESULTS" | tr -d ' ')
echo "BVA8 summary $passed/$total shutdown=$shutdown"
[[ "$passed" == "$total" && "$total" -gt 0 && "$shutdown" == clean ]]
