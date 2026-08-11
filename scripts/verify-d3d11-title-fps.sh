#!/usr/bin/env bash
# Verifies A3 with PPSSPP, real PSP content, and the shipped ARM64 DXVK path.
# Samples come from PPSSPP's guest log; compositor flushes are not FPS.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/canonical-a3-staged-20260808.raw}
VARS=${VARS:-$HOME/BridgeVM/work/canonical-a3-staged-20260808-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/d3d11-title-fps-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-300}
TITLE_SECONDS=${TITLE_SECONDS:-45}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
DXVK_ARM64=${DXVK_ARM64:-$HOME/BridgeVM/dxvk/build.arm64/src}
PPSSPP_SOURCE=${PPSSPP_SOURCE:-$HOME/BridgeVM/apps/ppsspp}
D3D11_DLL=${D3D11_DLL:-$DXVK_ARM64/d3d11/d3d11.dll}
DXGI_DLL=${DXGI_DLL:-$DXVK_ARM64/dxgi/dxgi.dll}
PPSSPP_EXECUTABLE=${PPSSPP_EXECUTABLE:-$PPSSPP_SOURCE/PPSSPPWindowsARM64.exe}
TITLE_ISO=${TITLE_ISO:-$HOME/BridgeVM/apps/cube.iso}
PRESTAGED_TITLE=${PRESTAGED_TITLE:-1}
GATE_SOURCE=$REPO/scripts/win-assets/bvgpu-real-title-gate.ps1
IDENTITY_SOURCE=$REPO/scripts/win-assets/bvgpu-d3d11-identity.ps1
D3D11_CONFIG=$REPO/scripts/win-assets/bv-ppsspp-d3d11.ini
SKIP_BUILD=${SKIP_BUILD:-0}

sha256() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }
[[ -f "$TARGET" && -f "$VARS" ]] || fail "target/vars missing"
[[ -f "$TITLE_ISO" ]] || fail "real title content missing: $TITLE_ISO"
[[ -f "$GATE_SOURCE" && -f "$IDENTITY_SOURCE" && -f "$D3D11_CONFIG" ]] || fail "gate assets missing"
[[ -f "$D3D11_DLL" ]] || fail "ARM64 DXVK d3d11.dll missing"
[[ -f "$DXGI_DLL" ]] || fail "ARM64 DXVK dxgi.dll missing"
[[ -f "$PPSSPP_EXECUTABLE" ]] || fail "PPSSPP ARM64 payload missing"
EXPECTED_TITLE_SHA=$(sha256 "$TITLE_ISO")
EXPECTED_PPSSPP_SHA=$(sha256 "$PPSSPP_EXECUTABLE")
EXPECTED_D3D11_SHA=$(sha256 "$D3D11_DLL")
EXPECTED_DXGI_SHA=$(sha256 "$DXGI_DLL")
EXPECTED_VENUS_SHA=$(sha256 "$VIOGPU_DIR/vulkan_virtio.dll")

WORK=$HOME/BridgeVM/work/d3d11-title-fps-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

CTL=$OUT/agent.ctl
: > "$CTL"
RUN_LOG=$OUT/run.log
HOST_SHARE=$OUT/share-host
mkdir -p "$HOST_SHARE"
cp "$GATE_SOURCE" "$HOST_SHARE/bvgpu-real-title-gate.ps1"
cp "$IDENTITY_SOURCE" "$HOST_SHARE/bvgpu-d3d11-identity.ps1"
cp "$D3D11_CONFIG" "$HOST_SHARE/bv-ppsspp-d3d11.ini"
cp "$TITLE_ISO" "$HOST_SHARE/cube.iso"
cp "$D3D11_DLL" "$HOST_SHARE/d3d11.dll"
cp "$DXGI_DLL" "$HOST_SHARE/dxgi.dll"
if [[ "$PRESTAGED_TITLE" != 1 ]]; then
  (cd "$(dirname "$PPSSPP_SOURCE")" && ditto -c -k --keepParent \
    "$(basename "$PPSSPP_SOURCE")" "$HOST_SHARE/ppsspp.zip")
fi

wait_for() { # pattern, count, timeout seconds
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(grep -cE "$1" "$RUN_LOG" 2>/dev/null || true)
    (( n >= $2 )) && return 0
    if [[ -n "${LAUNCHER:-}" ]] && ! kill -0 "$LAUNCHER" 2>/dev/null; then
      wait "$LAUNCHER" 2>/dev/null || true
      return 1
    fi
    sleep 0.3
  done
  return 1
}

send() { # control line, completion pattern, optional timeout
  local before timeout=${3:-$STEP_TIMEOUT}
  before=$(grep -cE "$2" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$1" >> "$CTL"
  wait_for "$2" $((before + 1)) "$timeout" || fail "no reply for: ${1:0:100}"
}

run_guest() { # command, timeout
  local before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$1" '^BVAGENT END ' "${2:-$STEP_TIMEOUT}"
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before ]] \
    || fail "command produced no result"
  [[ "$line" == *" exit=0" ]] || fail "guest command failed: ${line:0:180}"
}

BUILD_ARGS=()
[[ "$SKIP_BUILD" == 1 ]] && BUILD_ARGS+=(--skip-build)
BRIDGEVM_BOOT_PROGRESS_KILL=1 scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 --release "${BUILD_ARGS[@]}" \
  --agent-service-control "$CTL" \
  --agent-share-host "$HOST_SHARE" --agent-share-guest 'C:\BridgeVMShare' \
  --agent-share-max-kb 32768 \
  --performance-risk aggressive --virtio-gpu-3d \
  --gpu-trace "$OUT/virtio-gpu.jsonl" --gpu-trace-protocol venus \
  --viogpu3d-dir "$VIOGPU_DIR" > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
cleanup() {
  kill "$LAUNCHER" 2>/dev/null || true
  pkill -f hvf_gic_boot_probe 2>/dev/null || true
  wait "$LAUNCHER" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" || fail "agent service timeout"
echo "agent up at ${SECONDS}s"
for file in bvgpu-real-title-gate.ps1 bvgpu-d3d11-identity.ps1 bv-ppsspp-d3d11.ini cube.iso d3d11.dll dxgi.dll; do
  bytes=$(stat -f %z "$HOST_SHARE/$file")
  wait_for "^BVAGENT SHARE host->guest $file bytes=$bytes " 1 600 || fail "$file sync timeout"
done
if [[ "$PRESTAGED_TITLE" != 1 ]]; then
  bytes=$(stat -f %z "$HOST_SHARE/ppsspp.zip")
  wait_for "^BVAGENT SHARE host->guest ppsspp[.]zip bytes=$bytes " 1 600 || fail "PPSSPP sync timeout"
  EXPAND='powershell -NoProfile -Command "Remove-Item -Recurse -Force -ErrorAction SilentlyContinue C:\BridgeVM\a2-title; Expand-Archive -Force C:\BridgeVMShare\ppsspp.zip C:\BridgeVM\a2-title"'
  run_guest "$EXPAND" 300
fi

# Put DXVK beside the title (normal Windows DLL search) and replace PPSSPP's
# canonical Vulkan config with a D3D11 config before the gate launches it.
PREP='powershell -NoProfile -Command "$d=''C:\BridgeVM\a2-title\ppsspp''; if (-not (Test-Path $d\PPSSPPWindowsARM64.exe)) { exit 4 }; Copy-Item -Force C:\BridgeVMShare\d3d11.dll,C:\BridgeVMShare\dxgi.dll -Destination $d; Copy-Item -Force C:\BridgeVMShare\bv-ppsspp-d3d11.ini -Destination $d\bv-ppsspp.ini; Write-Output prep=D3D11OK"'
run_guest "$PREP" 120
tr -d '\r' < "$RUN_LOG" | grep -q '^prep=D3D11OK$' || fail "D3D11 title preparation failed"

GUEST_GATE='set "VK_DRIVER_FILES=C:\BridgeVM\viogpu3d\virtio_icd.arm64.json" && set "VK_INSTANCE_LAYERS=" && set "DXVK_LOG_LEVEL=info" && set "DXVK_LOG_PATH=C:\BridgeVMShare" && powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMShare\bvgpu-real-title-gate.ps1 -Executable C:\BridgeVM\a2-title\ppsspp\PPSSPPWindowsARM64.exe -ContentPath C:\BridgeVMShare\cube.iso -MinimumSeconds '"$TITLE_SECONDS"' -RequiredModule d3d11.dll -ExtraArgs "--backend=DIRECT3D11"'
run_guest "$GUEST_GATE" $((TITLE_SECONDS + STEP_TIMEOUT))
IDENTITY='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMShare\bvgpu-d3d11-identity.ps1 -ContentPath C:\BridgeVMShare\cube.iso'
run_guest "$IDENTITY" 120

LOG=$(tr -d '\r' < "$RUN_LOG")
FPS_LINE=$(grep 'guest_fps samples=' <<< "$LOG" | tail -1 || true)
SAMPLES=$(grep -oE 'samples=[0-9]+' <<< "$FPS_LINE" | cut -d= -f2 || true)
P50=$(grep -oE 'p50=[0-9]+([.][0-9]+)?' <<< "$FPS_LINE" | cut -d= -f2 || true)
MODULE_LINE=$(grep '^identity module=d3d11.dll ' <<< "$LOG" | tail -1 || true)
MODULE2_LINE=$(grep '^identity module=dxgi.dll ' <<< "$LOG" | tail -1 || true)
VENUS_LINE=$(grep '^identity module=vulkan_virtio.dll ' <<< "$LOG" | tail -1 || true)
PROCESS_LINE=$(grep '^identity process_path=' <<< "$LOG" | tail -1 || true)
CONTENT_LINE=$(grep '^identity content_path=' <<< "$LOG" | tail -1 || true)
MODULE_SHA=$(grep -oE 'sha256=[0-9A-Fa-f]+' <<< "$MODULE_LINE" | cut -d= -f2 || true)
MODULE2_SHA=$(grep -oE 'sha256=[0-9A-Fa-f]+' <<< "$MODULE2_LINE" | cut -d= -f2 || true)
VENUS_SHA=$(grep -oE 'sha256=[0-9A-Fa-f]+' <<< "$VENUS_LINE" | cut -d= -f2 || true)
PROCESS_SHA=$(grep -oE 'process_sha256=[0-9A-Fa-f]+' <<< "$PROCESS_LINE" | cut -d= -f2 || true)
CONTENT_SHA=$(grep -oE 'content_sha256=[0-9A-Fa-f]+' <<< "$CONTENT_LINE" | cut -d= -f2 || true)
TITLE_PASS=$(grep -c 'BVGPU-REAL-TITLE-PASS' <<< "$LOG" || true)
A3=fail
if [[ "${SAMPLES:-0}" -gt 0 ]] \
   && awk -v p="${P50:-0}" 'BEGIN { exit !(p >= 30.0) }' \
   && [[ "$MODULE_LINE" == *'a2-title\ppsspp\d3d11.dll'* ]] \
   && [[ "$MODULE2_LINE" == *'a2-title\ppsspp\dxgi.dll'* ]] \
   && grep -q '^identity status=PASS$' <<< "$LOG" \
   && [[ "$(tr '[:upper:]' '[:lower:]' <<< "$MODULE_SHA")" == "$EXPECTED_D3D11_SHA" ]] \
   && [[ "$(tr '[:upper:]' '[:lower:]' <<< "$MODULE2_SHA")" == "$EXPECTED_DXGI_SHA" ]] \
   && [[ "$(tr '[:upper:]' '[:lower:]' <<< "$VENUS_SHA")" == "$EXPECTED_VENUS_SHA" ]] \
   && [[ "$(tr '[:upper:]' '[:lower:]' <<< "$PROCESS_SHA")" == "$EXPECTED_PPSSPP_SHA" ]] \
   && [[ "$(tr '[:upper:]' '[:lower:]' <<< "$CONTENT_SHA")" == "$EXPECTED_TITLE_SHA" ]] \
   && [[ "$TITLE_PASS" -gt 0 ]]; then
  A3=pass
  echo "A3 D3D11 real-title FPS: PASS (samples=$SAMPLES p50=$P50 exact_hashes=true)"
else
  echo "A3 D3D11 real-title FPS: FAIL (samples=${SAMPLES:-?} p50=${P50:-?} title_pass=$TITLE_PASS)" >&2
  echo "  d3d11: ${MODULE_LINE:-<none>}" >&2
  echo "  dxgi: ${MODULE2_LINE:-<none>}" >&2
  echo "  venus: ${VENUS_LINE:-<none>}" >&2
  echo "  process: ${PROCESS_LINE:-<none>}" >&2
  echo "  content: ${CONTENT_LINE:-<none>}" >&2
fi

printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true
{
  echo "out_dir=$OUT"
  echo "a3_d3d11_fps=$A3"
  echo "samples=${SAMPLES:-}"
  echo "p50=${P50:-}"
  echo "fps_line=${FPS_LINE:-}"
  echo "module_line=${MODULE_LINE:-}"
  echo "module2_line=${MODULE2_LINE:-}"
  echo "venus_line=${VENUS_LINE:-}"
  echo "process_line=${PROCESS_LINE:-}"
  echo "content_line=${CONTENT_LINE:-}"
  echo "expected_ppsspp_sha256=$EXPECTED_PPSSPP_SHA"
  echo "expected_title_sha256=$EXPECTED_TITLE_SHA"
  echo "expected_d3d11_sha256=$EXPECTED_D3D11_SHA"
  echo "expected_dxgi_sha256=$EXPECTED_DXGI_SHA"
  echo "expected_venus_sha256=$EXPECTED_VENUS_SHA"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$A3" == pass ]]
