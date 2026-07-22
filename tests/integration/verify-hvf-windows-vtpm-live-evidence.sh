#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

usage() {
  fail "usage: $0 EVIDENCE_DIR [--ppi-action]"
}

[[ $# -ge 1 && $# -le 2 ]] || usage
evidence_dir="$1"
mode="command-path"
if [[ $# -eq 2 ]]; then
  [[ "$2" == "--ppi-action" ]] || usage
  mode="ppi-action"
fi
[[ -d "$evidence_dir" ]] || fail "evidence directory not found: $evidence_dir"

preflight="$evidence_dir/preflight.txt"
run_log="$evidence_dir/run.log"
cleanup="$evidence_dir/cleanup.txt"
target_stat="$evidence_dir/target-stat.txt"
for required in "$preflight" "$run_log" "$cleanup" "$target_stat"; do
  [[ -f "$required" ]] || fail "missing evidence file: $required"
done

grep -Fqx 'vtpm_enabled=1' "$preflight" || fail "preflight does not enable vTPM"
grep -Eq '^firmware_code=.*edk2-aarch64-secure-code[.]fd$' "$preflight" ||
  fail "preflight does not use BridgeVM's pinned secure+TPM firmware"
grep -Fq 'TPM2 TIS backend: swtpm data socket ' "$run_log" ||
  fail "run log has no concrete swtpm backend"
grep -Fq 'tpm2-tis: base=0xc000000 size=0x5000 ACPI=TPM0/MSFT0101+TPM2-log backend=swtpm ppi=shared-memory+dsm-1.3' "$run_log" ||
  fail "run log has no complete Windows TPM TIS/ACPI/PPI device contract"
grep -Fqx 'cleanup_status=0' "$cleanup" || fail "owned runtime cleanup did not succeed"
grep -Fqx 'run_status=0' "$target_stat" || fail "live run did not finish with status 0"

command_summary="$(grep -m1 '^TPM2 TIS command summary:' "$run_log" || true)"
[[ -n "$command_summary" ]] || fail "missing structured TPM command summary"
ppi_summary="$(grep -m1 '^TPM PPI shared-memory summary:' "$run_log" || true)"
[[ -n "$ppi_summary" ]] || fail "missing structured TPM PPI summary"

value_for() {
  local line="$1"
  local key="$2"
  local token
  for token in $line; do
    case "$token" in
      "$key="*) printf '%s\n' "${token#*=}"; return 0 ;;
    esac
  done
  fail "summary is missing $key"
}

optional_value_for() {
  local line="$1"
  local key="$2"
  local default="$3"
  local token
  for token in $line; do
    case "$token" in
      "$key="*) printf '%s\n' "${token#*=}"; return 0 ;;
    esac
  done
  printf '%s\n' "$default"
}

require_positive() {
  local line="$1"
  local key="$2"
  local value
  value="$(value_for "$line" "$key")"
  [[ "$value" =~ ^[0-9]+$ ]] || fail "$key is not an integer: $value"
  (( 10#$value > 0 )) || fail "$key must be positive"
}

require_zero() {
  local line="$1"
  local key="$2"
  local value
  value="$(value_for "$line" "$key")"
  [[ "$value" == "0" ]] || fail "$key must be zero, got $value"
}

for key in commands success get_capability pcr_read pcr_extend start_auth_session create_primary nv_read_public; do
  require_positive "$command_summary" "$key"
done
for key in backend_failures malformed_commands malformed_responses; do
  require_zero "$command_summary" "$key"
done

commands="$(value_for "$command_summary" commands)"
success="$(value_for "$command_summary" success)"
errors="$(value_for "$command_summary" errors)"
(( 10#$commands == 10#$success + 10#$errors )) ||
  fail "TPM response counts do not add up to commands"

classified=0
clear="$(optional_value_for "$command_summary" clear 0)"
[[ "$clear" =~ ^[0-9]+$ ]] || fail "clear is not an integer: $clear"
classified=$(( classified + 10#$clear ))
for key in startup self_test get_capability pcr_read pcr_extend start_auth_session create_primary read_public nv_read_public get_random other; do
  value="$(value_for "$command_summary" "$key")"
  [[ "$value" =~ ^[0-9]+$ ]] || fail "$key is not an integer: $value"
  classified=$(( classified + 10#$value ))
done
(( 10#$commands == classified )) ||
  fail "TPM command classification counts do not add up to commands"

require_positive "$ppi_summary" reads
require_zero "$ppi_summary" rejected_accesses

if [[ "$mode" != "ppi-action" ]]; then
  echo "PASS: live Windows vTPM enumeration and TIS command path verified"
  echo "NOTE: this receipt does not prove a PPI operation; run with --ppi-action for that gate"
  exit 0
fi

# --ppi-action: require a complete same-process Clear/reboot/F12/post-clear
# receipt. The probe exits 0 even after a watchdog cancel, so a clean guest
# SYSTEM_OFF stop line is required explicitly.
(( 10#$clear > 0 )) || fail "ppi-action requires clear>0 in the TIS command summary"
require_positive "$ppi_summary" writes

grep -Eq '^stop: PSCI .*\(system off\)' "$run_log" ||
  fail "ppi-action requires a clean guest PSCI SYSTEM_OFF stop"
# A benign length/pacing rejection of an operator echo (parse_error=too_long,
# Busy, queue_full) has no guest side effect and is tolerated. Replay-, malformed-,
# or misdelivery-class rejections are fatal: they would mean stale/garbled control
# reached the guest.
! grep -Eq 'live input rejected:.*(unknown_command|queue_error|command_too_long)' "$run_log" ||
  fail "ppi-action requires no replayed, malformed, or misdelivered live-input command"

line_of() {
  grep -nF -- "$1" "$run_log" | head -n1 | cut -d: -f1
}

first_reset="$(line_of 'PSCI SYSTEM_RESET: reboot 1/')"
second_reset="$(line_of 'PSCI SYSTEM_RESET: reboot 2/')"
f12_accepted="$(line_of 'live input accepted: command=Key("f12")')"
[[ -n "$first_reset" ]] || fail "ppi-action requires a first PSCI SYSTEM_RESET (Windows restart)"
[[ -n "$second_reset" ]] || fail "ppi-action requires a second PSCI SYSTEM_RESET (firmware post-clear reset)"
[[ -n "$f12_accepted" ]] || fail "ppi-action requires an accepted live F12 firmware approval"
(( f12_accepted > first_reset && f12_accepted < second_reset )) ||
  fail "ppi-action requires F12 accepted between the two resets (live, not replayed)"
# Capture the post-second-reset region into a variable before matching: a direct
# `awk ... | grep -q` pipeline lets grep close the pipe on its first match, which
# makes awk exit on SIGPIPE and (under `pipefail`) fails the whole check even
# though a match was found.
post_reset_log="$(awk -v start="$second_reset" 'NR > start' "$run_log")"
grep -Eq 'ramfb checkpoint: label=.* state=captured' <<<"$post_reset_log" ||
  fail "ppi-action requires a captured post-clear framebuffer checkpoint"

echo "PASS: live Windows vTPM PPI clear action verified"
echo "NOTE: TIS counters are cumulative across the in-process reboots; the PPI summary covers the final boot generation"
