#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-boot-timer-report.XXXXXX")"

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

canonical_checksum() {
  local value="$1"
  local decimal
  case "$value" in
    ""|\<unset\>) printf '%s\n' "$value" ;;
    0x*|0X*) printf '0x%016x\n' "$value" ;;
    *)
      decimal="${value#"${value%%[!0]*}"}"
      [[ -n "$decimal" ]] || decimal=0
      printf '0x%016x\n' "$decimal"
      ;;
  esac
}

make_run() {
  local name="$1"
  local profile="$2"
  local smp="$3"
  local daily="$4"
  local gpu3d="$5"
  local summary_ms="$6"
  local desktop_ms="$7"
  local exits0="$8"
  local exits1="$9"
  local rate0="${10}"
  local rate1="${11}"
  local desktop_agent="${12:-0}"
  local desktop_source="${13:-ramfb}"
  local cpu1_id="${14:-1}"
  local desktop_checksum="${15:-<unset>}"
  local milestone_checksum="${16:-0x1}"
  local start_checksum="${17:-$desktop_checksum}"
  local desktop_agent_log="false"
  local desktop_suffix=" checksum64=$(canonical_checksum "$milestone_checksum")"
  local dir="$STORE/$name"

  start_checksum="$(canonical_checksum "$start_checksum")"

  if [[ "$desktop_agent" == "1" ]]; then
    desktop_agent_log="true"
  fi
  if [[ "$desktop_source" == "agent" ]]; then
    desktop_suffix=""
  fi

  mkdir -p "$dir"
  cat >"$dir/preflight.txt" <<EOF
build_profile=$profile
daily_preset=$daily
smp_cpus=$smp
ram_mib=4096
watchdog_ms=900000
xhci_report_interval_ms=30
boot_timer=1
boot_timer_ramfb_ms=1000
boot_timer_desktop_checksum64=$desktop_checksum
boot_timer_desktop_agent=$desktop_agent
virtio_gpu_3d=$gpu3d
EOF
  cat >"$dir/run.log" <<EOF
BOOT_TIMER start ramfb_sample_ms=1000 desktop_checksum=$start_checksum desktop_agent=$desktop_agent_log
BOOT_TIMER milestone name=uefi source=serial elapsed_ms=100 exit=10
BOOT_TIMER milestone name=desktop source=$desktop_source elapsed_ms=$desktop_ms exit=20$desktop_suffix
BOOT_TIMER ramfb source=ramfb state=captured elapsed_ms=$desktop_ms exit=20 pc=0x0 checksum64=0x1 nonzero_pixels=1 unique_colors=1 desktop_match=true
BOOT_TIMER summary elapsed_ms=$summary_ms desktop_reached=true milestones=4/4
BOOT_TIMER vcpu cpu=0 exits=$exits0 exits_per_sec=$rate0
EOF
  if (( smp > 1 )); then
    printf 'BOOT_TIMER vcpu cpu=%s exits=%s exits_per_sec=%s\n' "$cpu1_id" "$exits1" "$rate1" >>"$dir/run.log"
  fi
  for (( cpu = 2; cpu < smp; cpu++ )); do
    printf 'BOOT_TIMER vcpu cpu=%s exits=0 exits_per_sec=0.00\n' "$cpu" >>"$dir/run.log"
  done
}

make_run smp1-a release 1 0 0 1000 900 100 200 10.00 20.00
make_run smp1-b release 1 0 0 1200 1000 150 250 15.00 25.00
make_run smp4-a release 4 1 1 800 700 400 500 40.00 50.00

output="$(
  scripts/report-hvf-boot-timer-metrics.sh \
    "$STORE/smp1-a" \
    "$STORE/smp1-b/run.log" \
    "$STORE/smp4-a"
)" || fail "boot timer report failed: $output"

assert_contains "$output" $'section\tconfig\tsource\tsummary_elapsed_ms' "report header"
assert_contains "$output" $'run\tprofile=release,smp=1,daily=0,ram=4096,watchdog=900000,xhci_ms=30,gpu3d=0,timer=1,timer_ms=1000,desktop=<unset>,desktop_agent=0' "smp1 run row"
assert_contains "$output" $'run\tprofile=release,smp=4,daily=1,ram=4096,watchdog=900000,xhci_ms=30,gpu3d=1,timer=1,timer_ms=1000,desktop=<unset>,desktop_agent=0' "smp4 run row"
assert_contains "$output" $'median\tprofile=release,smp=1,daily=0,ram=4096,watchdog=900000,xhci_ms=30,gpu3d=0,timer=1,timer_ms=1000,desktop=<unset>,desktop_agent=0\t2\t1100.00\t950.00\t125.00\t12.50\t2\t0' "smp1 median"
assert_contains "$output" $'median\tprofile=release,smp=4,daily=1,ram=4096,watchdog=900000,xhci_ms=30,gpu3d=1,timer=1,timer_ms=1000,desktop=<unset>,desktop_agent=0\t1\t800\t700\t900\t90.00' "smp4 median"

make_run agent-valid release 1 0 0 900 800 90 0 10.00 0.00 1 agent
agent_output="$(scripts/report-hvf-boot-timer-metrics.sh "$STORE/agent-valid")" \
  || fail "valid agent-oracle report failed: $agent_output"
assert_contains "$agent_output" $'desktop_agent=1\t'"$STORE"$'/agent-valid/run.log\t900\t800\ttrue\t4/4\t90\t10.00\t1\t1\tunknown\ttrue\t' "valid agent oracle"

INVALID="$STORE/invalid"
mkdir -p "$INVALID"
cat >"$INVALID/preflight.txt" <<'EOF'
smp_cpus=1
EOF
cat >"$INVALID/run.log" <<'EOF'
BOOT_TIMER start ramfb_sample_ms=1000 desktop_checksum=0x1
EOF
set +e
invalid_output="$(scripts/report-hvf-boot-timer-metrics.sh "$INVALID" 2>&1)"
invalid_status="$?"
set -e
[[ "$invalid_status" == "1" ]] || fail "invalid report should fail, got $invalid_status: $invalid_output"
assert_contains "$invalid_output" $'false\tmissing_summary,desktop_not_reached,vcpu_count_mismatch' "invalid run reasons"

make_run agent-mismatch release 1 0 0 900 800 90 0 10.00 0.00 1 ramfb
make_run disabled-agent-source release 1 0 0 900 800 90 0 10.00 0.00 0 agent
make_run hv-run-error release 1 0 0 900 800 90 0 10.00 0.00
sed -i '' '1i\
secondary vCPU1 hv_vcpu_run error 0xdead
' "$STORE/hv-run-error/run.log"
make_run duplicate-vcpu release 2 0 0 900 800 90 10 10.00 1.00 0 ramfb 0
make_run checksum-mismatch release 1 0 0 900 800 90 0 10.00 0.00 0 ramfb 1 0x1234 0x5678
make_run checksum-start-mismatch release 1 0 0 900 800 90 0 10.00 0.00 0 ramfb 1 0x1234 0x5678 0x5678
make_run invalid-metric release 1 0 0 900 800 not-a-number 0 invalid-rate 0.00

set +e
validity_output="$(
  scripts/report-hvf-boot-timer-metrics.sh \
    "$STORE/agent-mismatch" \
    "$STORE/disabled-agent-source" \
    "$STORE/hv-run-error" \
    "$STORE/duplicate-vcpu" \
    "$STORE/checksum-mismatch" \
    "$STORE/checksum-start-mismatch" \
    "$STORE/invalid-metric" 2>&1
)"
validity_status="$?"
set -e
[[ "$validity_status" == "1" ]] \
  || fail "oracle/stop/vCPU-invalid report should fail, got $validity_status: $validity_output"
assert_contains "$validity_output" $'false\tdesktop_oracle_mismatch' "agent oracle mismatch"
assert_contains "$validity_output" $'desktop_agent=0\t'"$STORE"$'/disabled-agent-source/run.log\t900\t800\ttrue\t4/4\t90\t10.00\t1\t1\tunknown\tfalse\tdesktop_oracle_mismatch' "disabled agent source mismatch"
assert_contains "$validity_output" $'false\thv_vcpu_run_error' "hv_vcpu_run error"
assert_contains "$validity_output" $'false\tvcpu_ids_mismatch' "duplicate vCPU id"
assert_contains "$validity_output" $'false\tdesktop_oracle_mismatch' "checksum oracle mismatch"
assert_contains "$validity_output" $'desktop=0x1234,desktop_agent=0\t'"$STORE"$'/checksum-start-mismatch/run.log\t900\t800\ttrue\t4/4\t90\t10.00\t1\t1\tunknown\tfalse\tdesktop_oracle_mismatch' "recorded checksum mismatch"
assert_contains "$validity_output" $'false\tmetric_fields_invalid' "invalid metric fields"

echo "PASS: HVF boot timer report smoke ($STORE)"
