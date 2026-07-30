#!/usr/bin/env bash
# Verifies A5 (audio): PCM produced inside the guest reaches the host's
# CoreAudio ring.
#
# The pass condition is frames_rendered>0 AND drops==0, read from the sink's
# end-of-run line. Both halves matter: drops==0 on its own is what a stream
# that never started looks like, since nothing was there to drop.
#
# Playback is driven through the agent with SoundPlayer, which blocks until the
# wav finishes, so when the command returns the guest has genuinely pushed the
# samples into the HDA stream.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

fail_early() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/audio-verify-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-180}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}

WORK=$HOME/BridgeVM/work/audio-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

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

# The host wraps anything that is not a protocol verb as RUN <base64(cmd)>
# itself (agent_console/protocol.rs:91-112, is_raw_verb). Wrapping it here too
# produced a double wrap, and the guest ran the literal word "RUN":
#   cmd.exe : 'RUN' is not recognized as an internal or external command
# So the command goes over the wire as plain text.
run_guest() { # $1 = shell command, $2 = completion pattern
  send "$1" "$2"
}

# --hda-coreaudio turns on both the device and the CoreAudio sink.
scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 \
  --hda-coreaudio \
  --agent-service-control "$CTL" \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'kill $LAUNCHER 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" \
  || fail_early "agent never reached service state within ${BOOT_TIMEOUT}s"
echo "agent up at ${SECONDS}s"

# The device has to be present and started inside Windows before playback
# means anything -- otherwise a silent guest and a broken host ring look the
# same.
run_guest 'powershell -NoProfile -Command "(Get-CimInstance Win32_SoundDevice | Select-Object -First 1 -ExpandProperty Status)"' \
  '^BVAGENT END '
DEV_STATUS=$(grep -A4 'BVAGENT CMD' "$RUN_LOG" | grep -oE '\bOK\b' | tail -1 || true)
echo "guest sound device status: ${DEV_STATUS:-<unknown>}"

# The sink only accepts 48 kHz stereo s16 and drops anything else, so generate
# a wav in exactly that format rather than trusting a bundled Windows asset.
# 2 seconds of a 440 Hz tone: long enough that a few dropped buffers would show
# up, short enough not to stretch the run.
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
run_guest "powershell -NoProfile -Command \"$GEN\"" '^BVAGENT END '

# SoundPlayer.PlaySync blocks until playback finishes, so a successful return
# means the frames really were handed to the audio stack.
run_guest 'powershell -NoProfile -Command "(New-Object System.Media.SoundPlayer \"C:\BridgeVM\a5-tone.wav\").PlaySync(); Write-Output played=1"' \
  '^BVAGENT END '
echo "playback returned at ${SECONDS}s"

# The stats line is printed when the sink is dropped, i.e. at end of run.
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true

STATS=$(grep -E '^hda CoreAudio stats:' "$RUN_LOG" | tail -1)
FRAMES=$(grep -oE 'frames_rendered=[0-9]+' <<< "$STATS" | cut -d= -f2)
# Anchored on a leading space so it cannot match dropped_bytes; a greedy sed
# expression swallowed the rest of the line and got this wrong.
DROPS=$(grep -oE '(^| )drops=[0-9]+' <<< "$STATS" | tr -d ' ' | cut -d= -f2)

if [[ "${FRAMES:-0}" -gt 0 ]] && [[ "${DROPS:-1}" -eq 0 ]]; then
  echo "A5 audio: PASS (frames_rendered=$FRAMES drops=$DROPS)"
  A5=pass
else
  echo "A5 audio: FAIL (frames_rendered=${FRAMES:-?} drops=${DROPS:-?})" >&2
  echo "  line: ${STATS:-<none>}" >&2
  A5=fail
fi

{
  echo "out_dir=$OUT"
  echo "a5_audio=$A5"
  echo "frames_rendered=${FRAMES:-}"
  echo "drops=${DROPS:-}"
  echo "guest_sound_device_status=${DEV_STATUS:-}"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"

[[ "$A5" == pass ]]
