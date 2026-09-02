#!/usr/bin/env bash
# Fail-closed physical-host controls for T16 NVMe calibration v2.

HVF_NVME_V2_ENVIRONMENT_SCHEMA=bridgevm.hvf-nvme-environment.v2
HVF_NVME_V2_THERMAL_INTERVAL_SECONDS=5
HVF_NVME_V2_MIN_HID_IDLE_NS=300000000000
HVF_NVME_V2_HOST_COOLDOWN_SECONDS=120
HVF_NVME_V2_ENV_MONITOR_PID=""

hvf_nvme_v2_env_seal() {
  openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}

hvf_nvme_v2_power_source() {
  pmset -g batt 2>/dev/null | sed -n "s/^Now drawing from '\(.*\)'/\1/p"
}

hvf_nvme_v2_thermal_state() {
  local state
  state="$(/usr/bin/osascript -l JavaScript \
    -e 'ObjC.import("Foundation"); Number($.NSProcessInfo.processInfo.thermalState)' \
    2>/dev/null | tr -d '[:space:]')" || return 1
  [[ "$state" =~ ^[0-3]$ ]] || return 1
  printf '%s\n' "$state"
}

hvf_nvme_v2_hid_idle_ns() {
  local idle
  idle="$(/usr/sbin/ioreg -r -c IOHIDSystem -d 1 2>/dev/null \
    | awk '/"HIDIdleTime" =/ { print $NF; exit }')" || return 1
  [[ "$idle" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$idle"
}

hvf_nvme_v2_environment_preflight() {
  local power thermal idle
  for tool in /usr/bin/osascript /usr/sbin/ioreg /usr/bin/pmset; do
    [[ -x "$tool" ]] || { printf 'T16 v2 host preflight: missing %s\n' "$tool" >&2; return 1; }
  done
  power="$(hvf_nvme_v2_power_source)"
  thermal="$(hvf_nvme_v2_thermal_state)" || thermal=unknown
  idle="$(hvf_nvme_v2_hid_idle_ns)" || idle=0
  printf 'T16 v2 host preflight: power=%s thermal_state=%s hid_idle_ns=%s\n' \
    "${power:-unknown}" "$thermal" "$idle"
  [[ "$power" == "AC Power" && "$thermal" == 0 \
    && "$idle" =~ ^[0-9]+$ && "$idle" -ge "$HVF_NVME_V2_MIN_HID_IDLE_NS" ]]
}

hvf_nvme_v2_environment_start() {
  local output="$1" thermal idle
  hvf_nvme_v2_environment_preflight || return 1
  HVF_NVME_V2_PMSET_POLICY="$output/pmset-policy.txt"
  HVF_NVME_V2_THERMAL_RAW="$output/thermal.raw.tsv"
  HVF_NVME_V2_HID_RAW="$output/hid.raw.tsv"
  HVF_NVME_V2_THERMAL_JSON="$output/thermal.json"
  HVF_NVME_V2_HID_JSON="$output/hid.json"
  pmset -g custom > "$HVF_NVME_V2_PMSET_POLICY" || return 1
  [[ -s "$HVF_NVME_V2_PMSET_POLICY" ]] || return 1
  NVME_PERF_PMSET_POLICY_HASH="$(hvf_nvme_v2_env_seal "$HVF_NVME_V2_PMSET_POLICY")"
  export NVME_PERF_PMSET_POLICY_HASH
  thermal="$(hvf_nvme_v2_thermal_state)" || return 1
  idle="$(hvf_nvme_v2_hid_idle_ns)" || return 1
  printf '1\t%s\n' "$thermal" > "$HVF_NVME_V2_THERMAL_RAW"
  printf '1\t%s\n' "$idle" > "$HVF_NVME_V2_HID_RAW"
  (
    local ordinal=2 sampled_thermal sampled_idle
    while sleep "$HVF_NVME_V2_THERMAL_INTERVAL_SECONDS"; do
      sampled_thermal="$(hvf_nvme_v2_thermal_state)" || exit 91
      sampled_idle="$(hvf_nvme_v2_hid_idle_ns)" || exit 92
      printf '%s\t%s\n' "$ordinal" "$sampled_thermal" >> "$HVF_NVME_V2_THERMAL_RAW"
      printf '%s\t%s\n' "$ordinal" "$sampled_idle" >> "$HVF_NVME_V2_HID_RAW"
      ordinal=$((ordinal + 1))
    done
  ) &
  HVF_NVME_V2_ENV_MONITOR_PID=$!
}

hvf_nvme_v2_environment_monitor_alive() {
  [[ "$HVF_NVME_V2_ENV_MONITOR_PID" =~ ^[1-9][0-9]*$ ]] \
    && kill -0 "$HVF_NVME_V2_ENV_MONITOR_PID" 2>/dev/null
}

hvf_nvme_v2_environment_stop() {
  local status=0
  if hvf_nvme_v2_environment_monitor_alive; then
    kill -TERM "$HVF_NVME_V2_ENV_MONITOR_PID" 2>/dev/null || status=1
    wait "$HVF_NVME_V2_ENV_MONITOR_PID" 2>/dev/null || true
  fi
  HVF_NVME_V2_ENV_MONITOR_PID=""
  python3 "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py" finalize \
    --thermal-raw "$HVF_NVME_V2_THERMAL_RAW" --hid-raw "$HVF_NVME_V2_HID_RAW" \
    --thermal-json "$HVF_NVME_V2_THERMAL_JSON" --hid-json "$HVF_NVME_V2_HID_JSON" \
    || status=1
  NVME_PERF_THERMAL_LOG_HASH="$(hvf_nvme_v2_env_seal "$HVF_NVME_V2_THERMAL_JSON")"
  NVME_PERF_HID_LOG_HASH="$(hvf_nvme_v2_env_seal "$HVF_NVME_V2_HID_JSON")"
  export NVME_PERF_THERMAL_LOG_HASH NVME_PERF_HID_LOG_HASH
  return "$status"
}

hvf_nvme_v2_environment_validate() {
  local current_hash summary field value valid
  current_hash="$(pmset -g custom | openssl dgst -sha256 -r | cut -d' ' -f1)"
  [[ "$current_hash" == "$NVME_PERF_PMSET_POLICY_HASH" ]] || return 1
  summary="$(python3 "$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.py" summarize \
    --thermal-json "$HVF_NVME_V2_THERMAL_JSON" --hid-json "$HVF_NVME_V2_HID_JSON")" || return 1
  while read -r field value; do
    case "$field" in
      thermal_sample_count) NVME_PERF_THERMAL_SAMPLE_COUNT="$value" ;;
      thermal_nominal_samples) NVME_PERF_THERMAL_NOMINAL_SAMPLES="$value" ;;
      host_hid_idle_start_seconds) NVME_PERF_HOST_HID_IDLE_START_SECONDS="$value" ;;
      host_hid_idle_end_seconds) NVME_PERF_HOST_HID_IDLE_END_SECONDS="$value" ;;
      host_hid_reset_count) NVME_PERF_HOST_HID_RESET_COUNT="$value" ;;
      valid) valid="$value" ;;
    esac
  done < <(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); [print(k, str(v).lower() if isinstance(v,bool) else v) for k,v in sorted(d.items())]' "$summary")
  export NVME_PERF_THERMAL_SAMPLE_COUNT NVME_PERF_THERMAL_NOMINAL_SAMPLES
  export NVME_PERF_HOST_HID_IDLE_START_SECONDS NVME_PERF_HOST_HID_IDLE_END_SECONDS
  export NVME_PERF_HOST_HID_RESET_COUNT
  [[ "$valid" == true && "$NVME_PERF_THERMAL_SAMPLE_COUNT" -ge 48 ]]
}

hvf_nvme_v2_environment_self_test() {
  python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/hvf-nvme-performance-v2-environment.py" self-test
  [[ "$HVF_NVME_V2_HOST_COOLDOWN_SECONDS" == 120 \
    && "$HVF_NVME_V2_MIN_HID_IDLE_NS" == 300000000000 ]]
  printf '%s\n' 'HVF NVMe v2 environment policy self-test: PASS'
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  case "${1:-}" in
    self-test) [[ $# -eq 1 ]] && hvf_nvme_v2_environment_self_test ;;
    preflight) [[ $# -eq 1 ]] && hvf_nvme_v2_environment_preflight ;;
    *) printf 'usage: %s {self-test|preflight}\n' "$0" >&2; exit 2 ;;
  esac
fi
