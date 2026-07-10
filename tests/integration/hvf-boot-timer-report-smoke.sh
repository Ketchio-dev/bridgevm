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
  local dir="$STORE/$name"

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
boot_timer_desktop_checksum64=<unset>
boot_timer_desktop_agent=0
virtio_gpu_3d=$gpu3d
EOF
  cat >"$dir/run.log" <<EOF
BOOT_TIMER start ramfb_sample_ms=1000 desktop_checksum=<unset>
BOOT_TIMER milestone name=uefi source=serial elapsed_ms=100 exit=10
BOOT_TIMER milestone name=desktop source=ramfb elapsed_ms=$desktop_ms exit=20 checksum64=0x1
BOOT_TIMER ramfb source=ramfb state=captured elapsed_ms=$desktop_ms exit=20 pc=0x0 checksum64=0x1 nonzero_pixels=1 unique_colors=1 desktop_match=true
BOOT_TIMER summary elapsed_ms=$summary_ms desktop_reached=true milestones=4/4
BOOT_TIMER vcpu cpu=0 exits=$exits0 exits_per_sec=$rate0
EOF
  if (( smp > 1 )); then
    printf 'BOOT_TIMER vcpu cpu=1 exits=%s exits_per_sec=%s\n' "$exits1" "$rate1" >>"$dir/run.log"
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

echo "PASS: HVF boot timer report smoke ($STORE)"
