#!/usr/bin/env bash
# T16 v2: one sealed lower-noise Windows NVMe calibration lane on a physical Mac.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
source "$REPO/scripts/live-gates/hvf-nvme-performance-v2-manifest.sh"
source "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.sh"
source "$REPO/scripts/live-gates/live-process-cleanup.sh"
WRITER="$REPO/scripts/write-hvf-nvme-performance-v2-receipt.py"
QUIESCENCE_VERIFIER="$REPO/scripts/verify-hvf-nvme-quiescence-v2.py"
OUT="" INPUT_MANIFEST="" SEALED_BINARY="" JOB_ID=local-nvme-v2 VALIDATE_ONLY=0
SEEN_OUT=0 SEEN_MANIFEST=0 SEEN_BINARY=0 SEEN_JOB=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) [[ $# -ge 2 && "$SEEN_OUT" == 0 ]] || exit 2; SEEN_OUT=1; OUT="$2"; shift 2 ;;
    --input-manifest) [[ $# -ge 2 && "$SEEN_MANIFEST" == 0 ]] || exit 2; SEEN_MANIFEST=1; INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) [[ $# -ge 2 && "$SEEN_BINARY" == 0 ]] || exit 2; SEEN_BINARY=1; SEALED_BINARY="$2"; shift 2 ;;
    --job-id) [[ $# -ge 2 && "$SEEN_JOB" == 0 ]] || exit 2; SEEN_JOB=1; JOB_ID="$2"; shift 2 ;;
    --validate-only) [[ "$VALIDATE_ONLY" == 0 ]] || exit 2; VALIDATE_ONLY=1; shift ;;
    *) printf 'unknown NVMe v2 option %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && "$OUT" == /* ]] || { printf '%s\n' 'NVMe v2 tier needs absolute --out' >&2; exit 2; }

seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
power_source() { pmset -g batt 2>/dev/null | sed -n "s/^Now drawing from '\(.*\)'/\1/p"; }
wait_log() {
  local pattern="$1" count="$2" timeout="$3" deadline observed
  deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if [[ -f "$BOOT/run.log" ]]; then
      observed="$(tr '\r' '\n' < "$BOOT/run.log" | grep -Ec "$pattern" || true)"
    else observed=0; fi
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    [[ -z "$VM_PID" ]] || kill -0 "$VM_PID" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}
send_ok() {
  local command="$1" before
  before="$(grep -c '^BVAGENT END ' "$BOOT/run.log" 2>/dev/null || true)"
  printf '%s\n' "$command" >> "$CONTROL"
  wait_log '^BVAGENT END ' "$((before + 1))" 300 || return 1
  grep -E '^BVAGENT CMD .* exit=' "$BOOT/run.log" | tail -1 | grep -q ' exit=0$'
}
terminate_vm() {
  if [[ "$VM_PID" =~ ^[1-9][0-9]*$ ]] && kill -0 "$VM_PID" 2>/dev/null; then
    pkill -TERM -P "$VM_PID" 2>/dev/null || true
    bridgevm_terminate_owned_pid_bounded "$VM_PID" 100 50 || return 1
  fi
  VM_PID=""
}
stop_power_monitor() {
  if [[ "$POWER_MONITOR_PID" =~ ^[1-9][0-9]*$ ]] && kill -0 "$POWER_MONITOR_PID" 2>/dev/null; then
    bridgevm_terminate_owned_pid_bounded "$POWER_MONITOR_PID" 20 50 || return 1
  fi
  POWER_MONITOR_PID=""
  if [[ -s "$POWER_LOG" && ! -L "$POWER_LOG" ]]; then
    NVME_PERF_POWER_LOG_HASH="$(seal "$POWER_LOG")"
    export NVME_PERF_POWER_LOG_HASH
  else
    unset NVME_PERF_POWER_LOG_HASH
  fi
}
verify_caffeinated_ancestor() {
  local pid="$PPID" command parent hops=0
  while [[ "$pid" =~ ^[1-9][0-9]*$ && "$pid" != 1 && "$hops" -lt 12 ]]; do
    command="$(ps -ww -p "$pid" -o command= 2>/dev/null)" || return 1
    if [[ "$command" == /usr/bin/caffeinate\ -dimsu\ * ]]; then
      return 0
    fi
    parent="$(ps -p "$pid" -o ppid= 2>/dev/null | tr -d '[:space:]')" || return 1
    [[ "$parent" != "$pid" ]] || return 1
    pid="$parent"; hops=$((hops + 1))
  done
  return 1
}
export_quiescence_summary() {
  local summary="$1" field value valid=""
  while read -r field value; do
    case "$field" in
      guest_quiescence_sample_count) NVME_PERF_GUEST_QUIESCENCE_SAMPLE_COUNT="$value" ;;
      guest_cpu_median_percent) NVME_PERF_GUEST_CPU_MEDIAN_PERCENT="$value" ;;
      guest_cpu_p95_percent) NVME_PERF_GUEST_CPU_P95_PERCENT="$value" ;;
      guest_disk_bps_median) NVME_PERF_GUEST_DISK_BPS_MEDIAN="$value" ;;
      guest_disk_bps_p95) NVME_PERF_GUEST_DISK_BPS_P95="$value" ;;
      guest_disk_queue_p95) NVME_PERF_GUEST_DISK_QUEUE_P95="$value" ;;
      valid) valid="$value" ;;
    esac
  done < <(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); [print(k, str(v).lower() if isinstance(v,bool) else v) for k,v in sorted(d.items())]' "$summary")
  export NVME_PERF_GUEST_QUIESCENCE_SAMPLE_COUNT NVME_PERF_GUEST_CPU_MEDIAN_PERCENT
  export NVME_PERF_GUEST_CPU_P95_PERCENT NVME_PERF_GUEST_DISK_BPS_MEDIAN
  export NVME_PERF_GUEST_DISK_BPS_P95 NVME_PERF_GUEST_DISK_QUEUE_P95
  [[ "$valid" == true ]]
}

export NVME_PERF_JOB_ID="$JOB_ID" NVME_PERF_HARNESS_COMMIT="$(git -C "$REPO" rev-parse HEAD)"
export NVME_PERF_HOST_MODEL="$(sysctl -n hw.model 2>/dev/null || uname -m)"
export NVME_PERF_MACOS_VERSION="$(sw_vers -productVersion 2>/dev/null || uname -sr)"
export NVME_PERF_STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
export NVME_PERF_CAFFEINATED=false NVME_PERF_SECURITY_SERVICES_ENABLED=false
export NVME_PERF_POWER_SOURCE_START=unknown NVME_PERF_POWER_SOURCE_END=unknown
export NVME_PERF_DESKTOP_ELAPSED_MS=0
INVALID_REASON=sealed-input-invalid RECEIPT_WRITTEN=0 IDENTITY_READY=0
VM_PID="" POWER_MONITOR_PID="" ENVIRONMENT_STARTED=0 ENVIRONMENT_FINALIZED=0
POWER_LOG="$OUT/power-source.log"
BOOT="$OUT/boot"; CONTROL="$OUT/agent.ctl"

failure_args() {
  local reason="$1" args=(--failed-reason "$reason" --output "$OUT/receipt.json") pair
  for pair in \
    "power-log:$POWER_LOG" \
    "environment-policy:${ENVIRONMENT_POLICY:-}" \
    "environment-helper:$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py" \
    "pmset-policy:${HVF_NVME_V2_PMSET_POLICY:-}" \
    "thermal-log:${HVF_NVME_V2_THERMAL_JSON:-}" \
    "hid-log:${HVF_NVME_V2_HID_JSON:-}" \
    "guest-quiescence-script:${QUIESCENCE_SCRIPT:-}" \
    "guest-quiescence-config:${QUIESCENCE_CONFIG:-}" \
    "guest-quiescence-log:$OUT/share/nvme-quiescence-result.json"; do
    option="${pair%%:*}"; path="${pair#*:}"
    [[ -n "$path" && -s "$path" && ! -L "$path" ]] && args+=("--$option" "$path")
  done
  python3 "$WRITER" "${args[@]}"
}
on_exit() {
  local status="$?" cleanup_status=0
  trap - EXIT
  terminate_vm || cleanup_status=1
  if [[ "$ENVIRONMENT_STARTED" == 1 && "$ENVIRONMENT_FINALIZED" == 0 ]]; then
    hvf_nvme_v2_environment_stop || cleanup_status=1
    ENVIRONMENT_FINALIZED=1
  fi
  stop_power_monitor || cleanup_status=1
  if [[ "$RECEIPT_WRITTEN" == 0 && "$VALIDATE_ONLY" == 0 && "$IDENTITY_READY" == 1 ]]; then
    failure_args "$INVALID_REASON" || cleanup_status=1
    [[ -f "$OUT/receipt.json" ]] && RECEIPT_WRITTEN=1
  fi
  [[ "$cleanup_status" == 0 ]] || status=1
  exit "$status"
}
trap on_exit EXIT

[[ -f "$INPUT_MANIFEST" && ! -L "$INPUT_MANIFEST" ]] || exit 1
export NVME_PERF_MANIFEST_HASH="$(seal "$INPUT_MANIFEST")"
nvme_perf_v2_lane_validate "$REPO" "$INPUT_MANIFEST" "$SEALED_BINARY" || exit 1
[[ "$(nvme_perf_v2_value harness_commit "$INPUT_MANIFEST")" == "$NVME_PERF_HARNESS_COMMIT" ]] || exit 1
export NVME_PERF_BINARY_HASH="$(nvme_perf_v2_hash binary "$INPUT_MANIFEST")"
export NVME_PERF_IMAGE_HASH="$(nvme_perf_v2_hash image "$INPUT_MANIFEST")"
export NVME_PERF_VARS_HASH="$(nvme_perf_v2_hash vars "$INPUT_MANIFEST")"
export NVME_PERF_FIRMWARE_HASH="$(nvme_perf_v2_hash firmware "$INPUT_MANIFEST")"
export NVME_PERF_RENDERER_HASH="$(nvme_perf_v2_hash renderer "$INPUT_MANIFEST")"
export NVME_PERF_CONFIG_HASH="$(nvme_perf_v2_hash config "$INPUT_MANIFEST")"
export NVME_PERF_WORKLOAD_SCRIPT_HASH="$(nvme_perf_v2_hash workload_script "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_REGISTRY_HASH="$(nvme_perf_v2_hash campaign_registry "$INPUT_MANIFEST")"
export NVME_PERF_PUBLIC_SEED_HASH="$(nvme_perf_v2_hash public_seed "$INPUT_MANIFEST")"
export NVME_PERF_ENVIRONMENT_POLICY_HASH="$(nvme_perf_v2_hash environment_policy "$INPUT_MANIFEST")"
export NVME_PERF_ENVIRONMENT_HELPER_HASH="$(seal "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py")"
export NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH="$(nvme_perf_v2_hash quiescence_script "$INPUT_MANIFEST")"
export NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH="$(nvme_perf_v2_hash quiescence_config "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_ID="$(nvme_perf_v2_value campaign_id "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_MODE=AA
export NVME_PERF_CAMPAIGN_LABEL="$(nvme_perf_v2_value campaign_label "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_ORDER="$(nvme_perf_v2_value campaign_order "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_PAIR="$(nvme_perf_v2_value campaign_pair "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_ORDINAL="$(nvme_perf_v2_value campaign_ordinal "$INPUT_MANIFEST")"
export NVME_PERF_CAMPAIGN_EXPECTED_RUNS=48
NONCE="$(nvme_perf_v2_value workload_nonce "$INPUT_MANIFEST")"
FIRMWARE="$(nvme_perf_v2_value firmware "$INPUT_MANIFEST")"
RENDERER="$(nvme_perf_v2_value renderer "$INPUT_MANIFEST")"
CONFIG="$(nvme_perf_v2_value config "$INPUT_MANIFEST")"
WORKLOAD="$(nvme_perf_v2_value workload_script "$INPUT_MANIFEST")"
REGISTRY="$(nvme_perf_v2_value campaign_registry "$INPUT_MANIFEST")"
ENVIRONMENT_POLICY="$(nvme_perf_v2_value environment_policy "$INPUT_MANIFEST")"
QUIESCENCE_SCRIPT="$(nvme_perf_v2_value quiescence_script "$INPUT_MANIFEST")"
QUIESCENCE_CONFIG="$(nvme_perf_v2_value quiescence_config "$INPUT_MANIFEST")"
IDENTITY_READY=1
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  printf '%s\n' 'HVF NVMe calibration v2 manifest: PASS'
  trap - EXIT
  exit 0
fi

mkdir -p "$OUT"
[[ ! -e "$POWER_LOG" ]] || { INVALID_REASON=artifact-invalid; exit 1; }
: > "$POWER_LOG"
verify_caffeinated_ancestor || { INVALID_REASON=host-preflight-invalid; exit 1; }
NVME_PERF_CAFFEINATED=true; export NVME_PERF_CAFFEINATED
INVALID_REASON=host-preflight-invalid
hvf_nvme_v2_environment_start "$OUT" || exit 1
ENVIRONMENT_STARTED=1
NVME_PERF_POWER_SOURCE_START="$(power_source)"; export NVME_PERF_POWER_SOURCE_START
[[ "$NVME_PERF_POWER_SOURCE_START" == 'AC Power' ]] || exit 1
INVALID_REASON=host-monitor-failed
pmset -g pslog > "$POWER_LOG" 2>&1 & POWER_MONITOR_PID=$!
for _ in {1..10}; do
  grep -q "^Now drawing from '" "$POWER_LOG" 2>/dev/null && break
  kill -0 "$POWER_MONITOR_PID" 2>/dev/null || exit 1
  sleep 1
done
grep -Fxq "Now drawing from 'AC Power'" "$POWER_LOG" || { INVALID_REASON=power-source-invalid; exit 1; }

INVALID_REASON=sealed-input-invalid
codesign --verify --strict "$SEALED_BINARY" >/dev/null 2>&1 || exit 1
binary_libraries="$(otool -L "$SEALED_BINARY")" || exit 1
grep -F "$RENDERER" <<< "$binary_libraries" >/dev/null || exit 1
mkdir -p "$OUT/media" "$BOOT" "$OUT/share"
for stale in nvme-result.json nvme-raw.json nvme-result.done nvme-quiescence-result.json; do
  [[ ! -e "$OUT/share/$stale" ]] || exit 1
done
cp -c "$(nvme_perf_v2_value image "$INPUT_MANIFEST")" "$OUT/media/target.raw" \
  && cp -c "$(nvme_perf_v2_value vars "$INPUT_MANIFEST")" "$OUT/media/vars.fd" || exit 1
chmod u=rw,go= "$OUT/media/target.raw" "$OUT/media/vars.fd"
NVME_PERF_IMAGE_HASH="$(seal "$OUT/media/target.raw")"
NVME_PERF_VARS_HASH="$(seal "$OUT/media/vars.fd")"
export NVME_PERF_IMAGE_HASH NVME_PERF_VARS_HASH
[[ "$NVME_PERF_IMAGE_HASH" == "$(nvme_perf_v2_hash image "$INPUT_MANIFEST")" \
  && "$NVME_PERF_VARS_HASH" == "$(nvme_perf_v2_hash vars "$INPUT_MANIFEST")" ]] || exit 1
sleep "$HVF_NVME_V2_HOST_COOLDOWN_SECONDS"
hvf_nvme_v2_environment_monitor_alive || { INVALID_REASON=host-monitor-failed; exit 1; }

cp "$WORKLOAD" "$OUT/share/nvme-workload.ps1"
cp "$CONFIG" "$OUT/share/nvme-workload-config.json"
cp "$QUIESCENCE_SCRIPT" "$OUT/share/bv-nvme-quiescence-v2.ps1"
cp "$QUIESCENCE_CONFIG" "$OUT/share/hvf-nvme-performance-v2-quiescence.json"
CONTROL="$OUT/agent.ctl"; : > "$CONTROL"
INVALID_REASON=guest-unreachable
BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$OUT/media/target.raw" --vars "$OUT/media/vars.fd" --firmware-code "$FIRMWARE" \
  --evidence-dir "$BOOT" --release --skip-build --daily --smp-cpus 4 --ram-mib 6144 \
  --watchdog-ms 3600000 --boot-timer --boot-timer-desktop-agent --virtio-net --enable-xhci --hda-coreaudio \
  --virtio-gpu-3d --virtio-gpu-device-id 1050 --gpu-trace-protocol virgl --performance-risk aggressive \
  --display-export-ms 100 --display-export-fb "$BOOT/display.fb" --input-control "$BOOT/input.ctl" \
  --agent-service-control "$CONTROL" --agent-share-host "$OUT/share" --agent-share-guest 'C:\BridgeVMNVMe' \
  --agent-share-ms 500 --agent-share-max-kb 8192 > "$OUT/boot-wrapper.stdout" 2> "$OUT/boot-wrapper.stderr" &
VM_PID=$!
wait_log '^BVAGENT SERVICE start' 1 1500 || exit 1
for asset in nvme-workload.ps1 nvme-workload-config.json bv-nvme-quiescence-v2.ps1 hvf-nvme-performance-v2-quiescence.json; do
  bytes="$(stat -f %z "$OUT/share/$asset")"
  wait_log "^BVAGENT SHARE host->guest $asset bytes=$bytes " 1 300 || exit 1
done
INVALID_REASON=workload-input-invalid
verify_inputs="powershell -NoProfile -Command \"if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\nvme-workload.ps1' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_WORKLOAD_SCRIPT_HASH') { exit 41 }; if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\nvme-workload-config.json' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_CONFIG_HASH') { exit 42 }; if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\bv-nvme-quiescence-v2.ps1' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH') { exit 43 }; if ((Get-FileHash -LiteralPath 'C:\\BridgeVMNVMe\\hvf-nvme-performance-v2-quiescence.json' -Algorithm SHA256).Hash.ToLowerInvariant() -ne '$NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH') { exit 44 }; Write-Output BRIDGEVM_NVME_V2_INPUTS_VERIFIED\""
send_ok "$verify_inputs" || exit 1
wait_log '^BRIDGEVM_NVME_V2_INPUTS_VERIFIED$' 1 30 || exit 1

sleep 120
hvf_nvme_v2_environment_monitor_alive || { INVALID_REASON=host-monitor-failed; exit 1; }
INVALID_REASON=guest-quiescence-invalid
launch_quiet="powershell -NoProfile -Command \"Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMNVMe\\bv-nvme-quiescence-v2.ps1 -Nonce $NONCE -ConfigPath C:\\BridgeVMNVMe\\hvf-nvme-performance-v2-quiescence.json -OutputPath C:\\BridgeVMNVMe\\nvme-quiescence-result.json' } | Out-Null; Write-Output BRIDGEVM_NVME_V2_QUIESCENCE_LAUNCHED\""
send_ok "$launch_quiet" || exit 1
wait_log '^BRIDGEVM_NVME_V2_QUIESCENCE_LAUNCHED$' 1 30 || exit 1
deadline=$((SECONDS + 120))
while (( SECONDS < deadline )); do
  [[ -f "$OUT/share/nvme-quiescence-result.json" ]] && break
  kill -0 "$VM_PID" 2>/dev/null || exit 1
  sleep 1
done
[[ -f "$OUT/share/nvme-quiescence-result.json" ]] || exit 1
quiet_summary="$(python3 "$QUIESCENCE_VERIFIER" --result "$OUT/share/nvme-quiescence-result.json" \
  --config "$OUT/share/hvf-nvme-performance-v2-quiescence.json" --nonce "$NONCE")" || exit 1
export_quiescence_summary "$quiet_summary" || exit 1
NVME_PERF_GUEST_QUIESCENCE_LOG_HASH="$(seal "$OUT/share/nvme-quiescence-result.json")"
export NVME_PERF_GUEST_QUIESCENCE_LOG_HASH
NVME_PERF_SECURITY_SERVICES_ENABLED=true; export NVME_PERF_SECURITY_SERVICES_ENABLED
sleep 15

INVALID_REASON=workload-failed
launch_workload='powershell -NoProfile -Command "Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = '\''cmd /c powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMNVMe\nvme-workload.ps1 > C:\BridgeVMNVMe\nvme-workload.log 2>&1'\'' } | Out-Null; Write-Output BRIDGEVM_NVME_V2_WORKLOAD_LAUNCHED"'
send_ok "$launch_workload" || exit 1
wait_log '^BRIDGEVM_NVME_V2_WORKLOAD_LAUNCHED$' 1 30 || exit 1
INVALID_REASON=workload-timeout
deadline=$((SECONDS + 1800))
while (( SECONDS < deadline )); do
  [[ -f "$OUT/share/nvme-result.done" && -f "$OUT/share/nvme-result.json" \
    && -f "$OUT/share/nvme-raw.json" ]] && break
  kill -0 "$VM_PID" 2>/dev/null || { INVALID_REASON=worker-interrupted; exit 1; }
  sleep 1
done
[[ -f "$OUT/share/nvme-result.done" && -f "$OUT/share/nvme-result.json" \
  && -f "$OUT/share/nvme-raw.json" ]] || exit 1
python3 -c 'import json,sys; assert all(json.load(open(p, encoding="utf-8")).get("status") == "passed" for p in sys.argv[1:])' \
  "$OUT/share/nvme-result.done" "$OUT/share/nvme-result.json" >/dev/null 2>&1 \
  || { INVALID_REASON=workload-failed; exit 1; }
printf '%s\n' 'shutdown.exe /p /f' >> "$CONTROL"
status=0; wait "$VM_PID" || status=$?; VM_PID=""
[[ "$status" == 0 ]] || { INVALID_REASON=worker-interrupted; exit 1; }
grep -Fxq 'status=0' "$BOOT/agent-service-gate.txt" || { INVALID_REASON=guest-unreachable; exit 1; }
grep -Eq '^stop: PSCI .*\(system off\)' "$BOOT/run.log" || { INVALID_REASON=worker-interrupted; exit 1; }
grep -Eq '^NVMe (second namespace )?disk written back:' "$BOOT/run.log" || { INVALID_REASON=workload-failed; exit 1; }
summary="$(grep '^storage target effect summary:' "$BOOT/run.log" | tail -1 || true)"
[[ "$summary" =~ io_write_success_count=([1-9][0-9]*) \
  && "$summary" =~ io_flush_success_count=([1-9][0-9]*) ]] || { INVALID_REASON=workload-failed; exit 1; }

INVALID_REASON=host-monitor-failed
hvf_nvme_v2_environment_monitor_alive || exit 1
hvf_nvme_v2_environment_stop || exit 1
ENVIRONMENT_FINALIZED=1
hvf_nvme_v2_environment_validate || { INVALID_REASON=host-environment-invalid; exit 1; }
kill -0 "$POWER_MONITOR_PID" 2>/dev/null || exit 1
stop_power_monitor || exit 1
awk '$0 ~ /^Now drawing from / { seen++; if ($0 != "Now drawing from '\''AC Power'\''") bad=1 } END { exit !(seen >= 1 && !bad) }' "$POWER_LOG" \
  || { INVALID_REASON=power-source-invalid; exit 1; }
NVME_PERF_POWER_SOURCE_END="$(power_source)"; export NVME_PERF_POWER_SOURCE_END
[[ "$NVME_PERF_POWER_SOURCE_END" == 'AC Power' ]] || { INVALID_REASON=power-source-invalid; exit 1; }
"$REPO/scripts/report-hvf-boot-timer-metrics.sh" "$BOOT" > "$BOOT/boot-timer-report.tsv"
NVME_PERF_DESKTOP_ELAPSED_MS="$(awk -F '\t' '$1 == "run" && $13 == "true" { print $5; exit }' "$BOOT/boot-timer-report.tsv")"
export NVME_PERF_DESKTOP_ELAPSED_MS
[[ "$NVME_PERF_DESKTOP_ELAPSED_MS" =~ ^[0-9]+([.][0-9]+)?$ ]] || { INVALID_REASON=artifact-invalid; exit 1; }

INVALID_REASON=sealed-input-changed
[[ "$(seal "$SEALED_BINARY")" == "$NVME_PERF_BINARY_HASH" \
  && "$(seal "$INPUT_MANIFEST")" == "$NVME_PERF_MANIFEST_HASH" \
  && "$(seal "$(nvme_perf_v2_value public_seed "$INPUT_MANIFEST")")" == "$NVME_PERF_PUBLIC_SEED_HASH" \
  && "$(seal "$FIRMWARE")" == "$NVME_PERF_FIRMWARE_HASH" \
  && "$(seal "$RENDERER")" == "$NVME_PERF_RENDERER_HASH" \
  && "$(seal "$CONFIG")" == "$NVME_PERF_CONFIG_HASH" \
  && "$(seal "$WORKLOAD")" == "$NVME_PERF_WORKLOAD_SCRIPT_HASH" \
  && "$(seal "$REGISTRY")" == "$NVME_PERF_CAMPAIGN_REGISTRY_HASH" \
  && "$(seal "$ENVIRONMENT_POLICY")" == "$NVME_PERF_ENVIRONMENT_POLICY_HASH" \
  && "$(seal "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py")" == "$NVME_PERF_ENVIRONMENT_HELPER_HASH" \
  && "$(seal "$QUIESCENCE_SCRIPT")" == "$NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH" \
  && "$(seal "$QUIESCENCE_CONFIG")" == "$NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH" \
  && "$(seal "$OUT/share/nvme-workload.ps1")" == "$NVME_PERF_WORKLOAD_SCRIPT_HASH" \
  && "$(seal "$OUT/share/nvme-workload-config.json")" == "$NVME_PERF_CONFIG_HASH" \
  && "$(seal "$OUT/share/bv-nvme-quiescence-v2.ps1")" == "$NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH" \
  && "$(seal "$OUT/share/hvf-nvme-performance-v2-quiescence.json")" == "$NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH" ]] || exit 1

python3 "$WRITER" --result-json "$OUT/share/nvme-result.json" \
  --raw-json "$OUT/share/nvme-raw.json" --done-json "$OUT/share/nvme-result.done" \
  --config-json "$OUT/share/nvme-workload-config.json" --power-log "$POWER_LOG" \
  --environment-policy "$ENVIRONMENT_POLICY" \
  --environment-helper "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py" \
  --pmset-policy "$HVF_NVME_V2_PMSET_POLICY" --thermal-log "$HVF_NVME_V2_THERMAL_JSON" \
  --hid-log "$HVF_NVME_V2_HID_JSON" --guest-quiescence-script "$OUT/share/bv-nvme-quiescence-v2.ps1" \
  --guest-quiescence-config "$OUT/share/hvf-nvme-performance-v2-quiescence.json" \
  --guest-quiescence-log "$OUT/share/nvme-quiescence-result.json" --output "$OUT/receipt.json"
RECEIPT_WRITTEN=1; INVALID_REASON=""
