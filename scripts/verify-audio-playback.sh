#!/usr/bin/env bash
# Verifies A5 (audio): guest PCM reaches the host CoreAudio ring.
# PASS requires frames_rendered>0, zero drops/callback errors, and clean launcher exit.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
source "$REPO/scripts/live-gates/live-process-cleanup.sh"

fail_early() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-180}

# Keep temporary media beside the evidence directory so it stays out of archives.
if [[ -n "${OUT:-}" ]]; then
  mkdir -p -- "$(dirname -- "$OUT")"
  mkdir -m 700 -- "$OUT" || fail_early "choose a new output directory: $OUT"
else
  runs=$HOME/BridgeVM/runs
  mkdir -p -- "$runs"
  OUT=$(mktemp -d "$runs/audio-verify-$(date +%Y%m%d-%H%M%S).XXXXXX")
fi
WORK=$(mktemp -d "$(dirname -- "$OUT")/.bridgevm-audio-media.XXXXXX")
LAUNCHER=""

cleanup() {
  local status=$?
  if [[ -n "$LAUNCHER" ]]; then
    if ! bridgevm_terminate_process_group_bounded "$LAUNCHER"; then
      fail_early "launcher did not stop; media kept at $WORK"
    fi
    wait "$LAUNCHER" 2>/dev/null || true
  fi
  rm -rf -- "$WORK" || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

cp -c -- "$TARGET" "$WORK/disk.raw"
cp -- "$VARS" "$WORK/vars.fd"
chmod u+w "$WORK/disk.raw" "$WORK/vars.fd"

CTL=$OUT/agent.ctl
: > "$CTL"
RUN_LOG=$OUT/run.log

wait_for() { # $1 = pattern, $2 = count, $3 = timeout
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(grep -cE "$1" "$RUN_LOG" 2>/dev/null || true)
    (( n >= $2 )) && return 0
    sleep 0.3
  done
  return 1
}

send() { # $1 = ctl line, $2 = completion pattern
  local before
  before=$(grep -cE "$2" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$1" >> "$CTL"
  wait_for "$2" $((before + 1)) "$STEP_TIMEOUT" \
    || fail_early "no reply for: ${1:0:60}"
}

# The host wraps anything that is not a protocol verb as RUN <base64(cmd)>.
# Wrapping here too would make the guest run the literal word RUN.
run_guest() { # $1 = shell command, $2 = completion pattern
  local before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$1" "$2"
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before ]] \
    || fail_early "command produced no CMD result: ${1:0:60}"
  [[ "$line" == *" exit=0" ]] \
    || fail_early "guest command failed: ${line:0:160}"
}

powershell_encoded() { # stdin/string -> UTF-16LE base64 for -EncodedCommand
  printf '%s' "$1" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n'
}

launch_options=()
if [[ -n "${BRIDGEVM_PREBUILT_PROBE:-}" ]]; then
  launch_options+=(--release --skip-build)
fi
if [[ -n "${VIOGPU3D_DIR:-}" ]]; then
  launch_options+=(--virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl"
    --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU3D_DIR")
fi

# Run the launcher and its children in their own process group.
set -m
scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 \
  --hda-coreaudio \
  --agent-service-control "$CTL" \
  ${launch_options[@]+"${launch_options[@]}"} \
  < /dev/null > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
set +m

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" \
  || fail_early "agent never reached service state within ${BOOT_TIMEOUT}s"
echo "agent up at ${SECONDS}s"

# Require a started Windows sound device before playback evidence is meaningful.
run_guest 'powershell -NoProfile -Command "(Get-CimInstance Win32_SoundDevice | Select-Object -First 1 -ExpandProperty Status)"' \
  '^BVAGENT END '
DEV_STATUS=$(grep -A4 'BVAGENT CMD' "$RUN_LOG" | grep -oE '\bOK\b' | tail -1 || true)
echo "guest sound device status: ${DEV_STATUS:-<unknown>}"

# Generate two seconds of 48 kHz stereo s16, the only format this sink accepts.
GEN='$p="C:\BridgeVM\a5-tone.wav";$sr=48000;$sec=2;$n=$sr*$sec;'
GEN+='$ms=New-Object System.IO.MemoryStream;$bw=New-Object System.IO.BinaryWriter($ms);'
GEN+='$data=$n*4;'
GEN+='$bw.Write([char[]]"RIFF");$bw.Write([int](36+$data));$bw.Write([char[]]"WAVE");'
GEN+='$bw.Write([char[]]"fmt ");$bw.Write([int]16);$bw.Write([int16]1);$bw.Write([int16]2);'
GEN+='$bw.Write([int]$sr);$bw.Write([int]($sr*4));$bw.Write([int16]4);$bw.Write([int16]16);'
GEN+='$bw.Write([char[]]"data");$bw.Write([int]$data);'
GEN+='for($i=0;$i -lt $n;$i++){$v=[int16](12000*[Math]::Sin(2*[Math]::PI*440*$i/$sr));'
GEN+='$bw.Write($v);$bw.Write($v)};'
GEN+='[System.IO.File]::WriteAllBytes($p,$ms.ToArray());'
GEN+='Write-Output ("wav_bytes=" + (Get-Item $p).Length)'
GEN_ENCODED=$(powershell_encoded "$GEN")
run_guest "powershell -NoProfile -EncodedCommand $GEN_ENCODED" '^BVAGENT END '
grep -qE '^([|] )?wav_bytes=384044$' < <(tr -d '\r' < "$RUN_LOG") \
  || fail_early "generated wav was not the expected 384044 bytes"

# PlaySync returns only after Windows hands the complete wav to its audio stack.
PLAY='(New-Object System.Media.SoundPlayer "C:\BridgeVM\a5-tone.wav").PlaySync(); Write-Output "played=1"'
PLAY_ENCODED=$(powershell_encoded "$PLAY")
run_guest "powershell -NoProfile -EncodedCommand $PLAY_ENCODED" '^BVAGENT END '
grep -qE '^([|] )?played=1$' < <(tr -d '\r' < "$RUN_LOG") \
  || fail_early "SoundPlayer did not report completion"
echo "playback returned at ${SECONDS}s"

# The sink prints its final counters while the launcher shuts down.
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
LAUNCHER_EXIT=0
wait "$LAUNCHER" 2>/dev/null || LAUNCHER_EXIT=$?
if bridgevm_process_group_alive "$LAUNCHER"; then
  fail_early "launcher exited with owned processes still running"
fi
LAUNCHER=""

STATS=$(python3 scripts/live-gates/hvf_terminal_evidence.py --host-stats "$RUN_LOG" || true)
FRAMES=""; DROPS=""; CALLBACK_ERRORS=""
set +e
COUNTERS=$(python3 scripts/audio-playback-result.py \
  --launcher-exit "$LAUNCHER_EXIT" --stats-line "$STATS")
CLASSIFIER_EXIT=$?
set -e
if (( CLASSIFIER_EXIT == 0 )); then
  read -r FRAMES DROPS CALLBACK_ERRORS <<< "$COUNTERS"
  echo "A5 audio: PASS (frames_rendered=$FRAMES drops=$DROPS callback_errors=$CALLBACK_ERRORS launcher_exit=$LAUNCHER_EXIT)"
  A5=pass
else
  echo "A5 audio: FAIL (frames_rendered=? drops=? callback_errors=? launcher_exit=$LAUNCHER_EXIT)" >&2
  echo "  line: ${STATS:-<none>}" >&2
  A5=fail
fi

{
  echo "out_dir=$OUT"
  echo "a5_audio=$A5"
  echo "frames_rendered=$FRAMES"
  echo "drops=$DROPS"
  echo "callback_errors=$CALLBACK_ERRORS"
  echo "launcher_exit=$LAUNCHER_EXIT"
  echo "guest_sound_device_status=${DEV_STATUS:-}"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"

[[ "$A5" == pass ]]
