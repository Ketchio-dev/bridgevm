#!/usr/bin/env bash
# T16: one sealed Windows warm sequential NVMe diagnostic run on a physical Mac.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
source "$REPO/scripts/live-gates/hvf-nvme-performance-manifest.sh"
WRITER="$REPO/scripts/write-hvf-nvme-performance-receipt.py"
OUT=""; INPUT_MANIFEST=""; SEALED_BINARY=""; JOB_ID="local-nvme-perf"; VALIDATE_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    *) echo "unknown NVMe-performance option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && "$OUT" == /* ]] || { echo "NVMe-performance tier needs absolute --out" >&2; exit 2; }
mkdir -p "$OUT"

seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
power_source() { command -v pmset >/dev/null && pmset -g batt | sed -n "s/^Now drawing from '\(.*\)'/\1/p" || true; }
source "$REPO/scripts/live-gates/hvf-nvme-performance-runtime.sh"
send_ok() {
  local command="$1" before
  before="$(grep -c '^BVAGENT END ' "$BOOT/run.log" 2>/dev/null || true)"
  printf '%s\n' "$command" >> "$CONTROL"
  wait_log '^BVAGENT END ' "$((before + 1))" 300 || return 1
  grep -E '^BVAGENT CMD .* exit=' "$BOOT/run.log" | tail -1 | grep -q ' exit=0$'
}

export NVME_PERF_JOB_ID="$JOB_ID" NVME_PERF_HARNESS_COMMIT="$(git -C "$REPO" rev-parse HEAD)"
export NVME_PERF_BINARY_HASH=absent NVME_PERF_MANIFEST_HASH=absent NVME_PERF_IMAGE_HASH=absent
export NVME_PERF_VARS_HASH=absent NVME_PERF_FIRMWARE_HASH=absent NVME_PERF_RENDERER_HASH=absent
export NVME_PERF_CONFIG_HASH=absent NVME_PERF_WORKLOAD_SCRIPT_HASH=absent
export NVME_PERF_CAMPAIGN_REGISTRY_HASH=absent
export NVME_PERF_CAMPAIGN_ID=unknown NVME_PERF_CAMPAIGN_MODE=unknown NVME_PERF_CAMPAIGN_ROLE=unknown
export NVME_PERF_CAMPAIGN_ORDINAL=0 NVME_PERF_CAMPAIGN_EXPECTED_RUNS=0
export NVME_PERF_HOST_MODEL="$(sysctl -n hw.model 2>/dev/null || uname -m)"
export NVME_PERF_MACOS_VERSION="$(sw_vers -productVersion 2>/dev/null || uname -sr)"
export NVME_PERF_POWER_SOURCE_START="$(power_source)" NVME_PERF_POWER_SOURCE_END=unknown
[[ -n "$NVME_PERF_POWER_SOURCE_START" ]] || NVME_PERF_POWER_SOURCE_START=unknown
export NVME_PERF_STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)" NVME_PERF_DESKTOP_ELAPSED_MS=""
POWER_LOG="$OUT/power-source.log"; : > "$POWER_LOG"
export NVME_PERF_POWER_LOG_HASH="$(seal "$POWER_LOG")"
INVALID_REASON=failed-before-receipt; RECEIPT_WRITTEN=0; VM_PID=""; POWER_MONITOR_PID=""; RENDERER=""; BOOT="$OUT/boot"
trap on_exit EXIT

INVALID_REASON=sealed-input-mismatch
[[ -f "$INPUT_MANIFEST" && ! -L "$INPUT_MANIFEST" ]] || exit 1
NVME_PERF_MANIFEST_HASH="$(seal "$INPUT_MANIFEST")"; export NVME_PERF_MANIFEST_HASH
nvme_perf_manifest_validate "$REPO" "$INPUT_MANIFEST" "$SEALED_BINARY" || exit 1
export NVME_PERF_BINARY_HASH NVME_PERF_CAMPAIGN_ID NVME_PERF_CAMPAIGN_MODE
export NVME_PERF_CAMPAIGN_ROLE NVME_PERF_CAMPAIGN_ORDINAL NVME_PERF_CAMPAIGN_EXPECTED_RUNS
NVME_PERF_IMAGE_HASH="$(nvme_perf_manifest_hash image "$INPUT_MANIFEST")"
NVME_PERF_VARS_HASH="$(nvme_perf_manifest_hash vars "$INPUT_MANIFEST")"
NVME_PERF_FIRMWARE_HASH="$(nvme_perf_manifest_hash firmware "$INPUT_MANIFEST")"
NVME_PERF_RENDERER_HASH="$(nvme_perf_manifest_hash renderer "$INPUT_MANIFEST")"
NVME_PERF_CONFIG_HASH="$(nvme_perf_manifest_hash config "$INPUT_MANIFEST")"
NVME_PERF_WORKLOAD_SCRIPT_HASH="$(nvme_perf_manifest_hash workload_script "$INPUT_MANIFEST")"
NVME_PERF_CAMPAIGN_REGISTRY_HASH="$(nvme_perf_manifest_hash campaign_registry "$INPUT_MANIFEST")"
export NVME_PERF_IMAGE_HASH NVME_PERF_VARS_HASH NVME_PERF_FIRMWARE_HASH NVME_PERF_RENDERER_HASH
export NVME_PERF_CONFIG_HASH NVME_PERF_WORKLOAD_SCRIPT_HASH
export NVME_PERF_CAMPAIGN_REGISTRY_HASH
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  nvme_perf_manifest_verify_source_hashes "$INPUT_MANIFEST" || exit 1
  echo "HVF NVMe performance manifest: PASS"
  exit 0
fi

NVME_PERF_STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"; NVME_PERF_POWER_SOURCE_START="$(power_source)"
[[ -n "$NVME_PERF_POWER_SOURCE_START" ]] || NVME_PERF_POWER_SOURCE_START=unknown
export NVME_PERF_STARTED_AT NVME_PERF_POWER_SOURCE_START
INVALID_REASON=power-monitor-failed
pmset -g pslog > "$POWER_LOG" 2>&1 & POWER_MONITOR_PID=$!
for _ in {1..10}; do grep -q "^Now drawing from '" "$POWER_LOG" 2>/dev/null && break; kill -0 "$POWER_MONITOR_PID" 2>/dev/null || exit 1; sleep 1; done
grep -Fxq "Now drawing from 'AC Power'" "$POWER_LOG" || exit 1

INVALID_REASON=unsigned-or-invalid-binary
codesign --verify --strict "$SEALED_BINARY" >/dev/null 2>&1 || exit 1
FIRMWARE="$REPO/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
RENDERER="$(nvme_perf_manifest_value renderer "$INPUT_MANIFEST")"
CONFIG="$(nvme_perf_manifest_value config "$INPUT_MANIFEST")"
WORKLOAD="$(nvme_perf_manifest_value workload_script "$INPUT_MANIFEST")"
REGISTRY="$(nvme_perf_manifest_value campaign_registry "$INPUT_MANIFEST")"
NVME_PERF_FIRMWARE_HASH="$(seal "$FIRMWARE")"; NVME_PERF_RENDERER_HASH="$(seal "$RENDERER")"
NVME_PERF_CONFIG_HASH="$(seal "$CONFIG")"; NVME_PERF_WORKLOAD_SCRIPT_HASH="$(seal "$WORKLOAD")"
NVME_PERF_CAMPAIGN_REGISTRY_HASH="$(seal "$REGISTRY")"
export NVME_PERF_FIRMWARE_HASH NVME_PERF_RENDERER_HASH NVME_PERF_CONFIG_HASH NVME_PERF_WORKLOAD_SCRIPT_HASH NVME_PERF_CAMPAIGN_REGISTRY_HASH
[[ "$NVME_PERF_FIRMWARE_HASH" == "$(nvme_perf_manifest_hash firmware "$INPUT_MANIFEST")" \
  && "$NVME_PERF_RENDERER_HASH" == "$(nvme_perf_manifest_hash renderer "$INPUT_MANIFEST")" \
  && "$NVME_PERF_CONFIG_HASH" == "$(nvme_perf_manifest_hash config "$INPUT_MANIFEST")" \
  && "$NVME_PERF_WORKLOAD_SCRIPT_HASH" == "$(nvme_perf_manifest_hash workload_script "$INPUT_MANIFEST")" \
  && "$NVME_PERF_CAMPAIGN_REGISTRY_HASH" == "$(nvme_perf_manifest_hash campaign_registry "$INPUT_MANIFEST")" ]] || exit 1
binary_libraries="$(otool -L "$SEALED_BINARY")" || exit 1
grep -F "$RENDERER" <<< "$binary_libraries" >/dev/null || { INVALID_REASON=renderer-input-mismatch; exit 1; }

mkdir -p "$OUT/media" "$BOOT" "$OUT/share"
for stale in nvme-result.json nvme-raw.json nvme-result.done; do [[ ! -e "$OUT/share/$stale" ]] || { INVALID_REASON=stale-result-artifact; exit 1; }; done
INVALID_REASON=apfs-clone-failed
cp -c "$(nvme_perf_manifest_value image "$INPUT_MANIFEST")" "$OUT/media/target.raw" \
  && cp -c "$(nvme_perf_manifest_value vars "$INPUT_MANIFEST")" "$OUT/media/vars.fd" || exit 1
chmod u=rw,go= "$OUT/media/target.raw" "$OUT/media/vars.fd"
cp "$WORKLOAD" "$OUT/share/nvme-workload.ps1"; cp "$CONFIG" "$OUT/share/nvme-workload-config.json"
INVALID_REASON=cloned-input-mismatch
NVME_PERF_IMAGE_HASH="$(seal "$OUT/media/target.raw")"; NVME_PERF_VARS_HASH="$(seal "$OUT/media/vars.fd")"
export NVME_PERF_IMAGE_HASH NVME_PERF_VARS_HASH
[[ "$NVME_PERF_IMAGE_HASH" == "$(nvme_perf_manifest_hash image "$INPUT_MANIFEST")" \
  && "$NVME_PERF_VARS_HASH" == "$(nvme_perf_manifest_hash vars "$INPUT_MANIFEST")" \
  && "$(seal "$OUT/share/nvme-workload.ps1")" == "$NVME_PERF_WORKLOAD_SCRIPT_HASH" \
  && "$(seal "$OUT/share/nvme-workload-config.json")" == "$NVME_PERF_CONFIG_HASH" ]] || exit 1
printf 'harness_commit=%s\nbinary_sha256=%s\nimage_sha256=%s\nvars_sha256=%s\nfirmware_sha256=%s\nrenderer_sha256=%s\nconfig_sha256=%s\nworkload_script_sha256=%s\ncampaign_registry_sha256=%s\nworkload_profile=windows-nvme-warm-seq-v1\npower_source_start=%s\n' \
  "$NVME_PERF_HARNESS_COMMIT" "$NVME_PERF_BINARY_HASH" "$NVME_PERF_IMAGE_HASH" "$NVME_PERF_VARS_HASH" "$NVME_PERF_FIRMWARE_HASH" "$NVME_PERF_RENDERER_HASH" "$NVME_PERF_CONFIG_HASH" "$NVME_PERF_WORKLOAD_SCRIPT_HASH" "$NVME_PERF_CAMPAIGN_REGISTRY_HASH" "$NVME_PERF_POWER_SOURCE_START" > "$OUT/measurement-identity.txt"

CONTROL="$OUT/agent.ctl"; : > "$CONTROL"
INVALID_REASON=boot-wrapper-failed
BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$OUT/media/target.raw" --vars "$OUT/media/vars.fd" --firmware-code "$FIRMWARE" \
  --evidence-dir "$BOOT" --release --skip-build --daily --smp-cpus 4 --ram-mib 6144 \
  --watchdog-ms 1800000 --boot-timer --boot-timer-desktop-agent --virtio-net --enable-xhci --hda-coreaudio \
  --virtio-gpu-3d --virtio-gpu-device-id 1050 --gpu-trace-protocol virgl --performance-risk aggressive \
  --display-export-ms 100 --display-export-fb "$BOOT/display.fb" --input-control "$BOOT/input.ctl" \
  --agent-service-control "$CONTROL" --agent-share-host "$OUT/share" --agent-share-guest 'C:\BridgeVMNVMe' \
  --agent-share-ms 500 --agent-share-max-kb 8192 > "$OUT/boot-wrapper.stdout" 2> "$OUT/boot-wrapper.stderr" &
VM_PID=$!
wait_log '^BVAGENT SERVICE start' 1 1500 || { INVALID_REASON=agent-service-timeout; exit 1; }
for asset in nvme-workload.ps1 nvme-workload-config.json; do
  bytes="$(stat -f %z "$OUT/share/$asset")"
  wait_log "^BVAGENT SHARE host->guest $asset bytes=$bytes " 1 300 || { INVALID_REASON=workload-share-timeout; exit 1; }
done
verify_inputs="powershell -NoProfile -Command \"if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\nvme-workload.ps1' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_WORKLOAD_SCRIPT_HASH') { exit 41 }; if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\nvme-workload-config.json' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_CONFIG_HASH') { exit 42 }; Write-Output BRIDGEVM_NVME_INPUTS_VERIFIED\""
send_ok "$verify_inputs" || { INVALID_REASON=workload-input-hash-mismatch; exit 1; }
wait_log '^BRIDGEVM_NVME_INPUTS_VERIFIED$' 1 30 || { INVALID_REASON=workload-input-hash-unconfirmed; exit 1; }
launch='powershell -NoProfile -Command "Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = '\''cmd /c powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMNVMe\nvme-workload.ps1 > C:\BridgeVMNVMe\nvme-workload.log 2>&1'\'' } | Out-Null; Write-Output BRIDGEVM_NVME_WORKLOAD_LAUNCHED"'
send_ok "$launch" || { INVALID_REASON=workload-launch-failed; exit 1; }
wait_log '^BRIDGEVM_NVME_WORKLOAD_LAUNCHED$' 1 30 || { INVALID_REASON=workload-launch-unconfirmed; exit 1; }
deadline=$((SECONDS + 1200))
while (( SECONDS < deadline )); do
  [[ -f "$OUT/share/nvme-result.done" && -f "$OUT/share/nvme-result.json" && -f "$OUT/share/nvme-raw.json" ]] && break
  kill -0 "$VM_PID" 2>/dev/null || { INVALID_REASON=vm-exited-before-result; exit 1; }
  sleep 1
done
[[ -f "$OUT/share/nvme-result.done" && -f "$OUT/share/nvme-result.json" && -f "$OUT/share/nvme-raw.json" ]] || { INVALID_REASON=workload-result-timeout; exit 1; }
workload_status_ok=1
python3 -c 'import json,sys; assert all(json.load(open(p, encoding="utf-8")).get("status") == "passed" for p in sys.argv[1:])' \
  "$OUT/share/nvme-result.done" "$OUT/share/nvme-result.json" >/dev/null 2>&1 || workload_status_ok=0
[[ "$workload_status_ok" == 1 ]] || INVALID_REASON=workload-status-failed
printf '%s\n' 'shutdown.exe /p /f' >> "$CONTROL"
status=0; wait "$VM_PID" || status=$?; VM_PID=""
[[ "$status" == 0 ]] || { INVALID_REASON=boot-wrapper-nonzero; exit 1; }
[[ "$workload_status_ok" == 1 ]] || exit 1
grep -Fxq 'status=0' "$BOOT/agent-service-gate.txt" || { INVALID_REASON=agent-service-gate-failed; exit 1; }
kill -0 "$POWER_MONITOR_PID" 2>/dev/null || { INVALID_REASON=power-monitor-ended-early; exit 1; }
stop_power_monitor
awk '$0 ~ /^Now drawing from / { seen++; if ($0 != "Now drawing from '\''AC Power'\''") bad=1 } END { exit !(seen >= 1 && !bad) }' "$POWER_LOG" \
  || { INVALID_REASON=power-source-changed-or-unknown; exit 1; }
grep -Eq '^stop: PSCI .*\(system off\)' "$BOOT/run.log" || { INVALID_REASON=guest-shutdown-missing; exit 1; }
grep -Eq '^NVMe (second namespace )?disk written back:' "$BOOT/run.log" || { INVALID_REASON=nvme-writeback-missing; exit 1; }
summary="$(grep '^storage target effect summary:' "$BOOT/run.log" | tail -1 || true)"
[[ "$summary" =~ io_write_success_count=([1-9][0-9]*) && "$summary" =~ io_flush_success_count=([1-9][0-9]*) ]] \
  || { INVALID_REASON=nvme-write-flush-evidence-missing; exit 1; }

INVALID_REASON=boot-timer-report-failed
"$REPO/scripts/report-hvf-boot-timer-metrics.sh" "$BOOT" > "$BOOT/boot-timer-report.tsv"
desktop="$(awk -F '\t' '$1 == "run" && $13 == "true" { print $5; exit }' "$BOOT/boot-timer-report.tsv")"
[[ "$desktop" =~ ^[0-9]+([.][0-9]+)?$ ]] || exit 1
NVME_PERF_DESKTOP_ELAPSED_MS="$desktop"; NVME_PERF_POWER_SOURCE_END="$(power_source)"
[[ -n "$NVME_PERF_POWER_SOURCE_END" ]] || NVME_PERF_POWER_SOURCE_END=unknown
export NVME_PERF_DESKTOP_ELAPSED_MS NVME_PERF_POWER_SOURCE_END
[[ "$NVME_PERF_POWER_SOURCE_START" != unknown && "$NVME_PERF_POWER_SOURCE_END" == "$NVME_PERF_POWER_SOURCE_START" ]] \
  || { INVALID_REASON=power-source-changed-or-unknown; exit 1; }
[[ "$(seal "$SEALED_BINARY")" == "$NVME_PERF_BINARY_HASH" && "$(seal "$FIRMWARE")" == "$NVME_PERF_FIRMWARE_HASH" \
  && "$(seal "$RENDERER")" == "$NVME_PERF_RENDERER_HASH" && "$(seal "$CONFIG")" == "$NVME_PERF_CONFIG_HASH" \
  && "$(seal "$WORKLOAD")" == "$NVME_PERF_WORKLOAD_SCRIPT_HASH" \
  && "$(seal "$REGISTRY")" == "$NVME_PERF_CAMPAIGN_REGISTRY_HASH" \
  && "$(seal "$OUT/share/nvme-workload-config.json")" == "$NVME_PERF_CONFIG_HASH" \
  && "$(seal "$OUT/share/nvme-workload.ps1")" == "$NVME_PERF_WORKLOAD_SCRIPT_HASH" ]] \
  || { INVALID_REASON=sealed-input-changed-during-run; exit 1; }
python3 "$WRITER" --result-json "$OUT/share/nvme-result.json" --raw-json "$OUT/share/nvme-raw.json" \
  --done-json "$OUT/share/nvme-result.done" --config-json "$OUT/share/nvme-workload-config.json" --output "$OUT/receipt.json"
RECEIPT_WRITTEN=1; INVALID_REASON=""
