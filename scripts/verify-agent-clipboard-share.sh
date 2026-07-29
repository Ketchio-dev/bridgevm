#!/usr/bin/env bash
# Verifies A6 (clipboard) and A7 (folder share) against a live Windows guest.
#
# Both features were already wired end to end -- guest verbs in bvagent.ps1,
# host decoding in agent_console/, CLI flags in the runner -- so this is a
# verification task, not an implementation one. Nothing proved they actually
# work on a live guest, which is what this does.
#
# The checks are content-based on purpose: a marker line saying CLIPSET
# succeeded proves only that the guest accepted the command. These compare the
# text that comes back and the SHA-256 of the bytes that land on disk, and the
# clipboard payload is Korean so the base64(UTF-8) path is exercised rather
# than assumed from ASCII.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

TARGET=${TARGET:-$HOME/BridgeVM/work/wall-c8-clean-12041.raw}
VARS=${VARS:-$HOME/BridgeVM/work/wall-c8-clean-inject-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/agent-verify-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-90}

fail_early() { echo "FAIL: $*" >&2; exit 1; }

# --virtio-gpu-3d is what makes the runner build the probe with --features
# venus, and virtio-console only exists in that build. Without it the probe
# comes up with no console device at all, the agent env vars are set but
# unused, and the run just times out. Confirmed by
# BRIDGEVM_PROBE_PRINT_CAPABILITIES=1 reporting virtio_gpu_3d_compiled=false.
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
gpu_args() { printf '%s\n' --virtio-gpu-3d --gpu-trace "$1/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR"; }

WORK=$HOME/BridgeVM/work/agent-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
# cp -c: APFS clone, so the source lineage is never written to.
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

# The agent is planted by the WinPE injector (PLANT_AGENT defaults to 1 in
# build-hvf-windows-driver-injector.sh), so a bare base image has no agent to
# talk to -- a first attempt against one sat waiting for a service that was
# never going to start. Run the injector pass first, exactly as the boot gate
# does.
# The injector must carry KEEP_RUNNING, otherwise firstboot powers the guest
# off the moment it finishes and the agent -- which only starts after logon --
# never comes up. That is exactly how the first two attempts at this failed:
# agent-service-gate.txt reported guest_system_off=true, service_started=false.
INJECTOR=${INJECTOR:-/tmp/inj-keep-running.raw}
if [[ ! -f "$INJECTOR" ]]; then
  echo "building injector with KEEP_RUNNING=1 ..."
  ISO=${ISO:-/Volumes/PortableSSD/BridgeVM-archive/recovery/Win11_25H2_English_Arm64_v2.iso} \
  VIOGPU3D_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only} \
  KEEP_RUNNING=1 OUT="$INJECTOR" scripts/build-hvf-windows-viogpu3d-injector.sh \
    > "$OUT/injector-build.log" 2>&1 \
    || fail_early "injector build failed, see $OUT/injector-build.log"
fi
LC_ALL=C grep -a -q 'keep-running' "$INJECTOR" \
  || fail_early "$INJECTOR has no keep-running marker; firstboot would power the guest off"
cp "$INJECTOR" "$WORK/inj.raw"
echo "injector pass..."
scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" --placeholder-nsid1 "$WORK/inj.raw" \
  --evidence-dir "$OUT/inject" --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 \
  $(gpu_args "$OUT/inject") \
  > "$OUT/inject-launcher.out" 2>&1
observed=$(grep -h '^injector_boot_observed=' "$OUT/inject/target-stat.txt" 2>/dev/null | cut -d= -f2)
[[ "$observed" == true ]] || { echo "FAIL: injector pass did not run (observed=$observed)" >&2; exit 1; }
echo "injector done at ${SECONDS}s"

CTL=$OUT/agent.ctl
: > "$CTL"
HOST_SHARE=$OUT/share-host
GET_DIR=$OUT/fetched
mkdir -p "$HOST_SHARE" "$GET_DIR"

# The payloads. Korean text is the point of the clipboard check: the HID path
# cannot carry it (there is no key usage for a syllable), so the clipboard is
# the only transport, and it is base64(UTF-8) on both sides.
CLIP_TEXT='한글 클립보드 검증 clipboard ✓'
printf '%s' "$CLIP_TEXT" > "$OUT/clip-expected.txt"
CLIP_B64=$(printf '%s' "$CLIP_TEXT" | base64)

# Host -> guest share file, with content that is not a round number of blocks.
HOST_FILE=$HOST_SHARE/from-host.bin
head -c 12345 /dev/urandom > "$HOST_FILE"
HOST_SHA=$(shasum -a 256 "$HOST_FILE" | cut -d' ' -f1)

RUN_LOG=$OUT/run.log
fail() { echo "FAIL: $*" >&2; exit 1; }

wait_for() { # $1 = grep -E pattern, $2 = required count, $3 = timeout s
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
    || fail "no reply for: ${1:0:40} (pattern: $2)"
}

# BRIDGEVM_VIRTIO_CONSOLE_GET_DIR has no CLI flag, and the launcher strips
# inherited BRIDGEVM_* vars before exec'ing the probe -- but it strips them
# from the *probe*, while the runner reads this one from its own environment
# to build ENV_ARGS. Export it here so GET writes the bytes to disk.
export BRIDGEVM_VIRTIO_CONSOLE_GET_DIR="$GET_DIR"

scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 \
  --agent-service-control "$CTL" \
  --agent-clipboard-sync \
  --agent-share-host "$HOST_SHARE" --agent-share-guest 'C:\BridgeVMShare' \
  $(gpu_args "$OUT") \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'kill $LAUNCHER 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT

# ctl lines are only drained once the agent reaches Service state.
wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" \
  || fail "agent never reached service state within ${BOOT_TIMEOUT}s"
echo "agent up at ${SECONDS}s"

### A6: clipboard, host -> guest -> host, Korean
send "CLIPSET $CLIP_B64" "^BVAGENT CLIPSET $CLIP_B64 -> OK"
send 'CLIPGET' '^BVAGENT CLIP CLIPGET$'

# The reply body sits between the CLIP header and its END marker. Take the
# last occurrence: CLIPSET refreshes the guest's cache, so an earlier poll may
# have logged the pre-existing clipboard.
CLIP_GOT=$(awk '/^BVAGENT CLIP CLIPGET$/{c=""; f=1; next}
                /^BVAGENT END CLIPGET$/{f=0; last=c; next}
                f{c = c $0}
                END{printf "%s", last}' "$RUN_LOG")
printf '%s' "$CLIP_GOT" > "$OUT/clip-actual.txt"

if [[ "$CLIP_GOT" == "$CLIP_TEXT" ]]; then
  echo "A6 clipboard: PASS (round-tripped Korean exactly)"
  A6=pass
else
  echo "A6 clipboard: FAIL" >&2
  echo "  expected: $CLIP_TEXT" >&2
  echo "  actual:   $CLIP_GOT" >&2
  A6=fail
fi

### A7: folder share, both directions, compared by SHA-256
# Host -> guest: the share syncer copies HOST_SHARE into the guest directory.
GUEST_PATH='C:\BridgeVMShare\from-host.bin'
send "GET $(printf '%s' "$GUEST_PATH" | base64)" '^BVAGENT GET GET .* ok=true'

FETCHED=$GET_DIR/from-host.bin
if [[ -f "$FETCHED" ]] && [[ "$(shasum -a 256 "$FETCHED" | cut -d' ' -f1)" == "$HOST_SHA" ]]; then
  echo "A7 host->guest: PASS (sha256 $HOST_SHA)"
  A7A=pass
else
  echo "A7 host->guest: FAIL (expected $HOST_SHA)" >&2
  A7A=fail
fi

# Guest -> host: write a file in the guest, fetch it, compare against what the
# guest itself reports the hash to be.
GUEST_OUT='C:\BridgeVMShare\from-guest.bin'
send "powershell -NoProfile -Command \"\$b=[byte[]]::new(9001); (New-Object Random 42).NextBytes(\$b); [IO.File]::WriteAllBytes('$GUEST_OUT', \$b)\"" \
     '^BVAGENT END '
send "powershell -NoProfile -Command \"(Get-FileHash -Algorithm SHA256 '$GUEST_OUT').Hash\"" \
     '^BVAGENT END '
GUEST_SHA=$(awk '/^BVAGENT CMD .*Get-FileHash/{f=1; next}
                 /^BVAGENT END /{f=0}
                 f' "$RUN_LOG" | tr -d ' \r\n' | tail -c 64 | tr 'A-F' 'a-f')

send "GET $(printf '%s' "$GUEST_OUT" | base64)" '^BVAGENT GET GET .* ok=true'
FETCHED2=$GET_DIR/from-guest.bin
if [[ -f "$FETCHED2" ]] && [[ -n "$GUEST_SHA" ]] \
   && [[ "$(shasum -a 256 "$FETCHED2" | cut -d' ' -f1)" == "$GUEST_SHA" ]]; then
  echo "A7 guest->host: PASS (sha256 $GUEST_SHA)"
  A7B=pass
else
  echo "A7 guest->host: FAIL (guest reported '$GUEST_SHA')" >&2
  A7B=fail
fi

{
  echo "out_dir=$OUT"
  echo "a6_clipboard=$A6"
  echo "a7_host_to_guest=$A7A"
  echo "a7_guest_to_host=$A7B"
  echo "clip_expected_sha=$(shasum -a 256 "$OUT/clip-expected.txt" | cut -d' ' -f1)"
  echo "clip_actual_sha=$(shasum -a 256 "$OUT/clip-actual.txt" | cut -d' ' -f1)"
  echo "share_host_to_guest_sha=$HOST_SHA"
  echo "share_guest_to_host_sha=$GUEST_SHA"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"

[[ "$A6" == pass && "$A7A" == pass && "$A7B" == pass ]]
