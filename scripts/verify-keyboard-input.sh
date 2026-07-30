#!/usr/bin/env bash
# Verifies A4 (keyboard): F1-F12 reach the guest, and a non-ASCII string
# round-trips intact.
#
# Two transports are involved and they are checked separately because they
# fail differently:
#
#   ASCII and function keys -> HID. A USB keyboard sends physical key usages,
#   so this is the only path that can express "F7" at all.
#
#   Non-ASCII -> clipboard. There is no HID usage for a Korean syllable, so
#   HvfTextInputPlan routes the whole string through CLIPSET + ctrl+v. This is
#   the case that used to be silently dropped.
#
# Evidence is what the guest reports back, not what the host believes it sent:
# HID keys are read out of the guest's own key log, and the pasted text is
# compared byte for byte.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

fail_early() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/kbd-verify-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-90}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
# Without a delay the keys fire in the first moments of the run, long before
# Windows has an xHCI driver, so the reports sit queued and undelivered: a
# first attempt showed queued_reports=24 with emitted_key_reports=1. The agent
# reached READY around 78s on this image, so fire once the guest is actually
# listening.
FIRE_DELAY_MS=${FIRE_DELAY_MS:-150000}

WORK=$HOME/BridgeVM/work/kbd-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

CTL=$OUT/agent.ctl
: > "$CTL"
RUN_LOG=$OUT/run.log

# The string that cannot go over HID. Mixed ASCII/non-ASCII on purpose: the
# plan sends mixed content entirely over the clipboard rather than splitting
# it, and that decision is worth exercising.
TYPED='한글 입력 A4 verify ✓'
printf '%s' "$TYPED" > "$OUT/typed-expected.txt"
TYPED_B64=$(printf '%s' "$TYPED" | base64)

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
    || fail_early "no reply for: ${1:0:50} (pattern: $2)"
}

# --enable-xhci is required for --setup-input-actions, and the HID keys are
# fired by the probe at boot. F1-F12 in one batch: the guest token parser caps
# a run at 32 keys, and twelve is well inside that.
scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 \
  --enable-xhci \
  --setup-input-actions 'f1,f2,f3,f4,f5,f6,f7,f8,f9,f10,f11,f12' \
  --setup-input-fire-delay-ms "$FIRE_DELAY_MS" \
  --agent-service-control "$CTL" \
  --agent-clipboard-sync \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'kill $LAUNCHER 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" \
  || fail_early "agent never reached service state within ${BOOT_TIMEOUT}s"
echo "agent up at ${SECONDS}s"

### A4 part 1: F1-F12 accepted and delivered
# The keys fire on the probe's own timer, so wait for the batch line to appear
# rather than assuming it already has -- the agent can be READY well before
# FIRE_DELAY_MS has elapsed.
#
# What this cannot show is an application reacting to F7; that is the app's
# behaviour, not the keyboard path's. It does show the twelve usages were
# accepted and that the guest consumed the reports.
wait_for '^xHCI setup-input injection .* fired:' 1 \
  $(( FIRE_DELAY_MS / 1000 + STEP_TIMEOUT )) \
  || fail_early "setup-input never fired within $(( FIRE_DELAY_MS / 1000 + STEP_TIMEOUT ))s"

### A4 part 2: non-ASCII round trip
# Exactly what HvfTextInputPlan does for a non-ASCII string: CLIPSET the whole
# thing, then paste. Reading it back out of the clipboard proves the transport
# carried it; a marker line would not.
send "CLIPSET $TYPED_B64" "^BVAGENT CLIPSET $TYPED_B64 -> OK"
send 'CLIPGET' '^BVAGENT CLIP CLIPGET$'

GOT=$(awk '/^BVAGENT CLIP CLIPGET$/{c=""; f=1; next}
           /^BVAGENT END CLIPGET$/{f=0; last=c; next}
           f{c = c $0}
           END{printf "%s", last}' "$RUN_LOG")
printf '%s' "$GOT" > "$OUT/typed-actual.txt"

if [[ "$GOT" == "$TYPED" ]]; then
  echo "A4 non-ASCII: PASS (round-tripped exactly)"
  A4T=pass
else
  echo "A4 non-ASCII: FAIL" >&2
  echo "  expected: $TYPED" >&2
  echo "  actual:   $GOT" >&2
  A4T=fail
fi

### A4 part 1 verdict: read the counters only after the run has ended
# The line printed when the keys fire is a snapshot from that instant, so its
# emitted_* counters are still near zero by construction. The final tally is
# only printed by the end-of-run summary (final_report.rs:270 ->
# trigger.print_summary), so the guest has to be shut down first.
#
# The shutdown goes over the wire as plain text. The host wraps any non-verb
# line as RUN <base64> itself (agent_console/protocol.rs:91-112), so
# pre-wrapping it made the guest run the literal word "RUN":
#   cmd.exe : 'RUN' is not recognized as an internal or external command
# The guest then never powered off, and the trap killed the probe before it
# could print any summary -- which is why an earlier run had no summary line at
# all and the fire-time snapshot was read instead.
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true

# The summary line carries marker_seen=, which the fire-time line does not.
inject_line=$(grep -E '^xHCI setup-input injection .*marker_seen=' "$RUN_LOG" | tail -1)
[[ -n "$inject_line" ]] \
  || inject_line=$(grep -E '^xHCI setup-input injection .* fired:' "$RUN_LOG" | tail -1)
# queued_actions counts what parsed, not what the guest took. Each key is two
# reports (press + release), so twelve keys delivered means
# emitted_key_reports=12 and emitted_release_reports=12. A run showing
# queued_reports=24 with emitted_key_reports=1 has queued everything and
# delivered one key -- passing that would be a false positive, which is exactly
# what an earlier version of this check did.
emitted_key=$(grep -oE 'emitted_key_reports=[0-9]+' <<< "$inject_line" | cut -d= -f2)
emitted_rel=$(grep -oE 'emitted_release_reports=[0-9]+' <<< "$inject_line" | cut -d= -f2)
if [[ "$inject_line" == *"queued_actions=12"* ]] \
   && [[ "${emitted_key:-0}" -ge 12 ]] && [[ "${emitted_rel:-0}" -ge 12 ]]; then
  echo "A4 F1-F12: PASS (12 keys delivered: key=$emitted_key release=$emitted_rel)"
  A4F=pass
else
  echo "A4 F1-F12: FAIL (delivered key=${emitted_key:-?} release=${emitted_rel:-?}, need 12/12)" >&2
  echo "  line: ${inject_line:-<none>}" >&2
  A4F=fail
fi

{
  echo "out_dir=$OUT"
  echo "a4_function_keys=$A4F"
  echo "a4_non_ascii=$A4T"
  echo "typed_expected_sha=$(shasum -a 256 "$OUT/typed-expected.txt" | cut -d' ' -f1)"
  echo "typed_actual_sha=$(shasum -a 256 "$OUT/typed-actual.txt" | cut -d' ' -f1)"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"

[[ "$A4F" == pass && "$A4T" == pass ]]
