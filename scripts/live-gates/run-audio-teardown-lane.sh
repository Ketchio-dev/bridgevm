#!/usr/bin/env bash
# One private B7 lane: sealed playback script, clean shutdown, typed callback evidence.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
source "$REPO/scripts/live-gates/live-process-cleanup.sh"
DISK=""; VARS=""; BINARY=""; FIRMWARE=""; PLAYBACK=""; OUT=""; NONCE=""; ORDINAL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --disk) DISK="$2"; shift 2 ;; --vars) VARS="$2"; shift 2 ;;
    --binary) BINARY="$2"; shift 2 ;; --firmware) FIRMWARE="$2"; shift 2 ;;
    --playback-script) PLAYBACK="$2"; shift 2 ;; --out) OUT="$2"; shift 2 ;;
    --nonce) NONCE="$2"; shift 2 ;; --ordinal) ORDINAL="$2"; shift 2 ;;
    *) echo "unknown B7 lane option $1" >&2; exit 2 ;;
  esac
done
[[ "$OUT" == /* && "$NONCE" =~ ^[0-9a-f]{64}$ && "$ORDINAL" =~ ^([1-9]|10)$ ]] || exit 2
for path in "$DISK" "$VARS" "$BINARY" "$FIRMWARE" "$PLAYBACK"; do
  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] || exit 2
done
mkdir -m 700 "$OUT"
[[ -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)" ]] || exit 2
SHARE="$OUT/share"; mkdir -m 700 "$SHARE"; install -m 600 "$PLAYBACK" "$SHARE/bv-audio-teardown.ps1"
printf '%s\n' "$NONCE" > "$OUT/nonce"; chmod 600 "$OUT/nonce"
CTL="$OUT/agent.ctl"; : > "$CTL"; chmod 600 "$CTL"
LAUNCHER=""; EXIT_RECORDED=0
finish_launcher() {
  local status="$1"
  [[ "$EXIT_RECORDED" == 0 ]] || return 0
  printf '%s\n' "$status" > "$OUT/launcher.exit"; chmod 600 "$OUT/launcher.exit"; EXIT_RECORDED=1
}
cleanup() {
  local status=$?
  if [[ "$LAUNCHER" =~ ^[1-9][0-9]*$ ]] && bridgevm_process_alive "$LAUNCHER"; then
    bridgevm_terminate_owned_pid_bounded "$LAUNCHER" 100 50 || status=1
    finish_launcher 124
  fi
  exit "$status"
}
trap cleanup EXIT
wait_log() {
  local pattern="$1" count="$2" timeout="$3" deadline observed
  deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    observed="$(tr '\r' '\n' < "$OUT/run.log" 2>/dev/null | grep -Ec "$pattern" || true)"
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    [[ -z "$LAUNCHER" ]] || bridgevm_process_alive "$LAUNCHER" || return 1
    sleep 1
  done
  return 1
}
BRIDGEVM_PREBUILT_PROBE="$BINARY" "$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$DISK" --vars "$VARS" --firmware-code "$FIRMWARE" --evidence-dir "$OUT" \
  --release --skip-build --watchdog-ms 1500000 --ram-mib 6144 --smp-cpus 4 \
  --ramfb-samples 1000 --hda-coreaudio --no-guest-disk-harvest \
  --agent-service-control "$CTL" --agent-share-host "$SHARE" \
  --agent-share-guest 'C:\bridgevm-share' --agent-share-ms 1000 --agent-share-max-kb 1024 \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
wait_log '^BVAGENT SERVICE start' 1 1500 || exit 1
wait_log '^BVAGENT SHARE host->guest bv-audio-teardown\.ps1 bytes=' 1 180 || exit 1
result_name="b7-audio-result-${NONCE:0:12}.txt"; result_file="$SHARE/$result_name"
workload="cmd.exe /d /c powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-audio-teardown.ps1 -Nonce $NONCE > C:\\bridgevm-share\\$result_name 2>&1"
command="powershell.exe -NoProfile -Command \"\$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = '$workload' }; if (\$r.ReturnValue -ne 0 -or \$r.ProcessId -le 0) { exit 41 }; Write-Output B7-AUDIO-LAUNCHED\""
before="$(grep -c '^BVAGENT END ' "$OUT/run.log" 2>/dev/null || true)"
printf '%s\n' "$command" >> "$CTL"
wait_log '^BVAGENT END ' "$((before + 1))" 300 || exit 1
grep -F "BVAGENT CMD $command exit=0" "$OUT/run.log" >/dev/null || exit 1
deadline=$((SECONDS + 300)); result_ready=0
while (( SECONDS < deadline )); do
  if [[ -f "$result_file" && ! -L "$result_file" ]] && [[ "$(tr '\r' '\n' < "$result_file" | sed '/^$/d')" == "B7 PLAYBACK PASS nonce=$NONCE wav_bytes=384044" ]]; then result_ready=1; break; fi
  bridgevm_process_alive "$LAUNCHER" || exit 1
  sleep 1
done
(( result_ready == 1 )) || exit 1
printf '%s\n' 'shutdown.exe /s /t 3' >> "$CTL"
set +e
wait "$LAUNCHER"; launcher_status=$?
set -e
LAUNCHER=""; finish_launcher "$launcher_status"
python3 "$REPO/scripts/audio-teardown-result.py" --run-log "$OUT/run.log" \
  --result-file "$result_file" \
  --launcher-exit "$launcher_status" --nonce "$NONCE" --ordinal "$ORDINAL" \
  --output "$OUT/lane-result.json"
trap - EXIT
exit 0
