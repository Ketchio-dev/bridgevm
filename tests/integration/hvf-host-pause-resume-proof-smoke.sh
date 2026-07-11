#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-host-pause-resume.XXXXXX")"
EVIDENCE="$STORE/evidence"
FAKE_PROBE="$STORE/fake-probe.sh"

cleanup() {
  rm -rf "$STORE"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  trap - EXIT
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

mkdir -p "$EVIDENCE"
cat > "$FAKE_PROBE" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail

control="${BRIDGEVM_VIRTIO_CONSOLE_CTL:?}"
printf 'BVAGENT READY host=BRIDGEVM t=100\n'
printf 'BVAGENT PONG\n'
printf 'BVAGENT CMD ver exit=0\nMicrosoft Windows [Version test]\nBVAGENT END ver\n'
printf 'BVAGENT SERVICE start t=200\n'
printf 'BVAGENT SERVICE alive t=200\n'

seen=0
for _ in $(seq 1 200); do
  line="$(sed -n "$((seen + 1))p" "$control")"
  if [[ -n "$line" ]]; then
    seen=$((seen + 1))
    case "$line" in
      ver)
        printf 'BVAGENT CMD ver exit=0\nMicrosoft Windows [Version test]\nBVAGENT END ver\n'
        ;;
      'shutdown.exe /p /f')
        printf 'BVAGENT CMD shutdown.exe /p /f exit=0\nBVAGENT END shutdown.exe /p /f\n'
        printf 'NVMe disk written back: /tmp/fake.raw (4096 bytes)\n'
        printf 'stop: PSCI 0x84000008 (system off)\n'
        exit 0
        ;;
      *)
        printf 'unexpected control line: %s\n' "$line" >&2
        exit 7
        ;;
    esac
  fi
  sleep 0.05
done
exit 8
PROBE
chmod +x "$FAKE_PROBE"

source scripts/run-hvf-windows-installed-boot-runner.sh

BIN="$FAKE_PROBE"
WATCHDOG_MS=5000
HOST_PAUSE_RESUME_PROOF_MS=200
SHUTDOWN_AFTER_AGENT_READY=0
EVIDENCE_DIR="$EVIDENCE"
ENV_ARGS=("BRIDGEVM_VIRTIO_CONSOLE_CTL=$(host_pause_resume_control_path)")
PROBE_PID=""
RUN_STATUS=0

run_probe_process
[[ "$RUN_STATUS" == "0" ]] || fail "fake probe exited with $RUN_STATUS"
write_host_pause_resume_gate
[[ "$RUN_STATUS" == "0" ]] || fail "host pause/resume gate failed"

observation="$(cat "$EVIDENCE/host-pause-resume-observation.txt")"
assert_contains "$observation" "service_ready=true" "pause observation"
assert_contains "$observation" "during_state=T" "pause observation"
assert_contains "$observation" "log_stable_while_stopped=true" "pause observation"
assert_contains "$observation" "continue_signal_sent=true" "pause observation"
assert_contains "$observation" "post_resume_command_ok=true" "pause observation"
assert_contains "$observation" "control_status=0" "pause observation"

gate="$(cat "$EVIDENCE/host-pause-resume-gate.txt")"
assert_contains "$gate" "scope=process-resident-host-pause-resume" "pause gate"
assert_contains "$gate" "disk_backed_suspend=false" "pause gate"
assert_contains "$gate" "process_stopped=true" "pause gate"
assert_contains "$gate" "post_resume_agent_round_trip=true" "pause gate"
assert_contains "$gate" "guest_system_off=true" "pause gate"
assert_contains "$gate" "nvme_writeback=true" "pause gate"
assert_contains "$gate" "status=0" "pause gate"

control="$(cat "$EVIDENCE/host-pause-resume-control.txt")"
[[ "$control" == $'ver\nshutdown.exe /p /f' ]] \
  || fail "unexpected control sequence: $control"

FAIL_EVIDENCE="$STORE/fail-evidence"
mkdir -p "$FAIL_EVIDENCE"
cat > "$FAIL_EVIDENCE/host-pause-resume-observation.txt" <<'EOF'
service_ready=true
during_state=T
log_stable_while_stopped=true
continue_signal_sent=true
control_status=0
EOF
printf 'stop: watchdog (CANCELED)\n' > "$FAIL_EVIDENCE/run.log"
EVIDENCE_DIR="$FAIL_EVIDENCE"
HOST_PAUSE_RESUME_CONTROL_STATUS=0
RUN_STATUS=0
write_host_pause_resume_gate
[[ "$RUN_STATUS" == "1" ]] || fail "incomplete proof gate did not fail"
assert_contains "$(cat "$FAIL_EVIDENCE/host-pause-resume-gate.txt")" \
  "post_resume_agent_round_trip=false" "failed pause gate"
assert_contains "$(cat "$FAIL_EVIDENCE/host-pause-resume-gate.txt")" \
  "status=1" "failed pause gate"

sleep 30 &
PROBE_PID="$!"
kill -STOP "$PROBE_PID"
terminate_owned_probe 2>/dev/null
wait "$PROBE_PID" 2>/dev/null || true
kill -0 "$PROBE_PID" 2>/dev/null && fail "cleanup left stopped child alive"

echo "PASS: HVF host pause/resume proof gate ($STORE)"
