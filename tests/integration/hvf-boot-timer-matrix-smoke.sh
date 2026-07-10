#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-boot-timer-matrix.XXXXXX")"
TARGET="$STORE/windows-target.raw"
VARS="$STORE/vars.fd"
PLACEHOLDER="$STORE/placeholder.raw"
EVIDENCE="$STORE/evidence"
FAKE_WRAPPER="$STORE/fake-installed-boot.sh"
FAIL_WRAPPER="$STORE/failing-installed-boot.sh"

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

printf 'target\n' > "$TARGET"
printf 'vars\n' > "$VARS"
printf 'placeholder\n' > "$PLACEHOLDER"

cat > "$FAKE_WRAPPER" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail

target=""
vars=""
placeholder=""
evidence=""
smp=""
boot_timer="0"
boot_timer_ramfb_ms="<probe-default 1000>"
desktop_checksum="<unset>"
desktop_agent="0"
profile="debug"
ram_mib="4096"
virtio_net="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --vars) vars="$2"; shift 2 ;;
    --placeholder-nsid1) placeholder="$2"; shift 2 ;;
    --evidence-dir) evidence="$2"; shift 2 ;;
    --smp-cpus) smp="$2"; shift 2 ;;
    --boot-timer) boot_timer="1"; shift ;;
    --boot-timer-ramfb-ms) boot_timer="1"; boot_timer_ramfb_ms="$2"; shift 2 ;;
    --boot-timer-desktop-checksum64) boot_timer="1"; desktop_checksum="$2"; shift 2 ;;
    --boot-timer-desktop-agent) boot_timer="1"; desktop_agent="1"; shift ;;
    --release) profile="release"; shift ;;
    --skip-build) shift ;;
    --virtio-net) virtio_net="1"; shift ;;
    --ram-mib) ram_mib="$2"; shift 2 ;;
    *) shift ;;
  esac
done

[[ -f "$target" ]] || { echo "missing target: $target" >&2; exit 1; }
[[ -f "$vars" ]] || { echo "missing vars: $vars" >&2; exit 1; }
if [[ -n "$placeholder" && ! -f "$placeholder" ]]; then
  echo "missing placeholder: $placeholder" >&2
  exit 1
fi
mkdir -p "$evidence/ramfb"

run_name="$(basename "$evidence")"
run_index="${run_name#run-}"
summary_ms=$((1000 + smp * 100 + run_index * 10))
desktop_ms=$((summary_ms - 100))
exits=$((smp * 1000 + run_index))
rate="$(awk -v exits="$exits" 'BEGIN { printf "%.2f", exits / 10 }')"

cat > "$evidence/preflight.txt" <<EOF
target=$target
placeholder_nsid1=${placeholder:-<none>}
vars=$vars
evidence_dir=$evidence
build_profile=$profile
daily_preset=0
ram_mib=$ram_mib
watchdog_ms=900000
smp_cpus=$smp
xhci_report_interval_ms=<probe-default 30>
boot_timer=$boot_timer
boot_timer_ramfb_ms=$boot_timer_ramfb_ms
boot_timer_desktop_checksum64=$desktop_checksum
boot_timer_desktop_agent=$desktop_agent
virtio_gpu_3d=0
virtio_net=$virtio_net
EOF

cat > "$evidence/run.log" <<EOF
BOOT_TIMER start ramfb_sample_ms=$boot_timer_ramfb_ms desktop_checksum=$desktop_checksum
BOOT_TIMER milestone name=desktop source=ramfb elapsed_ms=$desktop_ms exit=20 checksum64=0x1
BOOT_TIMER summary elapsed_ms=$summary_ms desktop_reached=true milestones=4/4
BOOT_TIMER vcpu cpu=0 exits=$exits exits_per_sec=$rate
EOF
for (( cpu = 1; cpu < smp; cpu++ )); do
  printf 'BOOT_TIMER vcpu cpu=%s exits=0 exits_per_sec=0.00\n' "$cpu" >>"$evidence/run.log"
done
WRAPPER
chmod +x "$FAKE_WRAPPER"

cat > "$FAIL_WRAPPER" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
echo "intentional wrapper failure before artifacts" >&2
exit 17
WRAPPER
chmod +x "$FAIL_WRAPPER"

output="$(
  scripts/run-hvf-boot-timer-matrix.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --placeholder-nsid1 "$PLACEHOLDER" \
    --evidence-dir "$EVIDENCE" \
    --runs 2 \
    --smp-cpus 1,4 \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --wrapper "$FAKE_WRAPPER" \
    --release \
    --skip-build \
    -- --virtio-net --ram-mib 2048
)" || fail "boot timer matrix failed: $output"

assert_contains "$output" "Wrote boot timer matrix report: $EVIDENCE/boot-timer-report.tsv" "matrix output"
[[ -f "$EVIDENCE/boot-timer-report.tsv" ]] || fail "missing matrix report"
[[ -f "$EVIDENCE/smp-1/run-1/matrix-invocation.txt" ]] || fail "missing run invocation"
[[ -f "$EVIDENCE/smp-4/run-2/run.log" ]] || fail "missing run log"

report="$(cat "$EVIDENCE/boot-timer-report.tsv")"
assert_contains "$report" $'run\tprofile=release,smp=1,daily=0,ram=2048,watchdog=900000,xhci_ms=<probe-default_30>,gpu3d=0,timer=1,timer_ms=250,desktop=0x1234abcd,desktop_agent=0' "smp1 run row"
assert_contains "$report" $'run\tprofile=release,smp=4,daily=0,ram=2048,watchdog=900000,xhci_ms=<probe-default_30>,gpu3d=0,timer=1,timer_ms=250,desktop=0x1234abcd,desktop_agent=0' "smp4 run row"
assert_contains "$report" $'median\tprofile=release,smp=1,daily=0,ram=2048,watchdog=900000,xhci_ms=<probe-default_30>,gpu3d=0,timer=1,timer_ms=250,desktop=0x1234abcd,desktop_agent=0\t2\t1115.00\t1015.00\t1001.50\t100.15' "smp1 median"
assert_contains "$report" $'median\tprofile=release,smp=4,daily=0,ram=2048,watchdog=900000,xhci_ms=<probe-default_30>,gpu3d=0,timer=1,timer_ms=250,desktop=0x1234abcd,desktop_agent=0\t2\t1415.00\t1315.00\t4001.50\t400.15' "smp4 median"

run_target="$(awk -F= '$1 == "target" { print substr($0, 8); exit }' "$EVIDENCE/smp-1/run-1/preflight.txt")"
run_vars="$(awk -F= '$1 == "vars" { print substr($0, 6); exit }' "$EVIDENCE/smp-1/run-1/preflight.txt")"
run_placeholder="$(awk -F= '$1 == "placeholder_nsid1" { print substr($0, 19); exit }' "$EVIDENCE/smp-1/run-1/preflight.txt")"
[[ "$run_target" != "$TARGET" ]] || fail "matrix should clone target by default"
[[ "$run_vars" != "$VARS" ]] || fail "matrix should clone vars by default"
[[ "$run_placeholder" != "$PLACEHOLDER" ]] || fail "matrix should clone placeholder by default"
[[ -f "$run_target" && -f "$run_vars" && -f "$run_placeholder" ]] || fail "cloned media files missing"

AGENT_EVIDENCE="$STORE/agent-evidence"
scripts/run-hvf-boot-timer-matrix.sh \
  --target "$TARGET" \
  --vars "$VARS" \
  --evidence-dir "$AGENT_EVIDENCE" \
  --runs 1 \
  --smp-cpus 1 \
  --wrapper "$FAKE_WRAPPER" \
  --no-clone-media >/dev/null || fail "default agent-oracle matrix failed"
agent_preflight="$(cat "$AGENT_EVIDENCE/smp-1/run-1/preflight.txt")"
assert_contains "$agent_preflight" "boot_timer_desktop_agent=1" "default agent oracle"
agent_invocation="$(cat "$AGENT_EVIDENCE/smp-1/run-1/matrix-invocation.txt")"
assert_contains "$agent_invocation" "--boot-timer-desktop-agent" "default agent invocation"

FAIL_EVIDENCE="$STORE/fail-evidence"
set +e
fail_output="$(
  scripts/run-hvf-boot-timer-matrix.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$FAIL_EVIDENCE" \
    --runs 1 \
    --smp-cpus 2 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --wrapper "$FAIL_WRAPPER" \
    --no-clone-media 2>&1
)"
fail_status="$?"
set -e
[[ "$fail_status" == "17" ]] || fail "expected failing matrix status 17, got $fail_status: $fail_output"
assert_contains "$fail_output" "Wrote boot timer matrix report: $FAIL_EVIDENCE/boot-timer-report.tsv" "failed matrix output"
[[ -f "$FAIL_EVIDENCE/smp-2/run-1/run.log" ]] || fail "failed matrix should synthesize run.log"
[[ -f "$FAIL_EVIDENCE/smp-2/run-1/preflight.txt" ]] || fail "failed matrix should synthesize preflight"
fail_report="$(cat "$FAIL_EVIDENCE/boot-timer-report.tsv")"
assert_contains "$fail_report" $'run\tprofile=debug,smp=2,daily=unknown,ram=unknown,watchdog=unknown,xhci_ms=unknown,gpu3d=unknown,timer=1,timer_ms=<probe-default_1000>,desktop=0x1234abcd,desktop_agent=0' "failed run row"
assert_contains "$fail_report" $'\tfalse\t\t0\t0.00\t0\t0\t17\tfalse\tmissing_start,missing_summary,desktop_not_reached,vcpu_count_mismatch,run_status_nonzero' "failed run validity"

set +e
stale_output="$(
  scripts/run-hvf-boot-timer-matrix.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --runs 1 \
    --smp-cpus 1 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --wrapper "$FAKE_WRAPPER" \
    --no-clone-media 2>&1
)"
stale_status="$?"
set -e
[[ "$stale_status" != "0" ]] || fail "matrix must reject stale evidence"
assert_contains "$stale_output" "report already exists" "stale evidence rejection"

OVERRIDE_EVIDENCE="$STORE/override-evidence"
set +e
override_output="$(
  scripts/run-hvf-boot-timer-matrix.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$OVERRIDE_EVIDENCE" \
    --runs 1 \
    --smp-cpus 1 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --wrapper "$FAKE_WRAPPER" \
    --no-clone-media \
    -- --smp-cpus 4 2>&1
)"
override_status="$?"
set -e
[[ "$override_status" != "0" ]] || fail "matrix must reject passthrough overrides"
assert_contains "$override_output" "overrides matrix-owned option: --smp-cpus" "passthrough override rejection"

echo "PASS: HVF boot timer matrix smoke ($STORE)"
