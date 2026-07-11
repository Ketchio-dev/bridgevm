#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/report-hvf-boot-timer-metrics.sh EVIDENCE_DIR_OR_RUN_LOG [...]

Summarize BOOT_TIMER lines emitted by hvf_gic_boot_probe. Directory arguments
are expected to contain run.log and optionally preflight.txt. File arguments are
treated as run logs; a sibling preflight.txt is used when present.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

metadata_value() {
  local file="$1"
  local key="$2"
  [[ -f "$file" ]] || return 0
  awk -F= -v key="$key" '$1 == key { print substr($0, length(key) + 2); exit }' "$file"
}

canonical_u64_literal() {
  local value="$1"
  local normalized
  local decimal

  case "$value" in
    ""|unknown|\<unset\>)
      printf '%s\n' "$value"
      return 0
      ;;
    0x*|0X*)
      [[ "${value#??}" =~ ^[0-9a-fA-F]{1,16}$ ]] || {
        printf '<invalid-u64>\n'
        return 0
      }
      ;;
    *)
      [[ "$value" =~ ^[0-9]+$ ]] || {
        printf '<invalid-u64>\n'
        return 0
      }
      decimal="${value#"${value%%[!0]*}"}"
      [[ -n "$decimal" ]] || decimal=0
      value="$decimal"
      ;;
  esac

  if normalized="$(printf '0x%016x' "$value" 2>/dev/null)"; then
    printf '%s\n' "$normalized"
  else
    printf '<invalid-u64>\n'
  fi
}

sanitize_label() {
  tr '\t ' '__'
}

resolve_log_path() {
  local input="$1"
  if [[ -d "$input" ]]; then
    [[ -f "$input/run.log" ]] || fail "evidence dir is missing run.log: $input"
    printf '%s\n' "$input/run.log"
  elif [[ -f "$input" ]]; then
    printf '%s\n' "$input"
  else
    fail "input is neither a file nor a directory: $input"
  fi
}

derive_config_label() {
  local input="$1"
  local log="$2"
  local evidence_dir
  local preflight
  local build_profile
  local smp_cpus
  local daily_preset
  local ram_mib
  local watchdog_ms
  local xhci_report_interval_ms
  local gpu_3d
  local boot_timer
  local boot_timer_ramfb_ms
  local boot_timer_desktop_checksum64
  local boot_timer_desktop_agent
  local shutdown_after_agent_ready
  local virtio_console_test_periodic
  local host_pause_resume_proof_ms

  if [[ -d "$input" ]]; then
    evidence_dir="$input"
  else
    evidence_dir="$(dirname "$log")"
  fi
  preflight="$evidence_dir/preflight.txt"

  if [[ -f "$preflight" ]]; then
    build_profile="$(metadata_value "$preflight" build_profile)"
    smp_cpus="$(metadata_value "$preflight" smp_cpus)"
    daily_preset="$(metadata_value "$preflight" daily_preset)"
    ram_mib="$(metadata_value "$preflight" ram_mib)"
    watchdog_ms="$(metadata_value "$preflight" watchdog_ms)"
    xhci_report_interval_ms="$(metadata_value "$preflight" xhci_report_interval_ms)"
    gpu_3d="$(metadata_value "$preflight" virtio_gpu_3d)"
    boot_timer="$(metadata_value "$preflight" boot_timer)"
    boot_timer_ramfb_ms="$(metadata_value "$preflight" boot_timer_ramfb_ms)"
    boot_timer_desktop_checksum64="$(metadata_value "$preflight" boot_timer_desktop_checksum64)"
    boot_timer_desktop_agent="$(metadata_value "$preflight" boot_timer_desktop_agent)"
    shutdown_after_agent_ready="$(metadata_value "$preflight" shutdown_after_agent_ready)"
    virtio_console_test_periodic="$(metadata_value "$preflight" virtio_console_test_periodic)"
    host_pause_resume_proof_ms="$(metadata_value "$preflight" host_pause_resume_proof_ms)"
    printf 'profile=%s,smp=%s,daily=%s,ram=%s,watchdog=%s,xhci_ms=%s,gpu3d=%s,timer=%s,timer_ms=%s,desktop=%s,desktop_agent=%s,shutdown=%s,console_periodic=%s,host_pause_ms=%s\n' \
      "${build_profile:-unknown}" \
      "${smp_cpus:-<unset>}" \
      "${daily_preset:-unknown}" \
      "${ram_mib:-unknown}" \
      "${watchdog_ms:-unknown}" \
      "${xhci_report_interval_ms:-unknown}" \
      "${gpu_3d:-unknown}" \
      "${boot_timer:-unknown}" \
      "${boot_timer_ramfb_ms:-unknown}" \
      "${boot_timer_desktop_checksum64:-unknown}" \
      "${boot_timer_desktop_agent:-unknown}" \
      "${shutdown_after_agent_ready:-unknown}" \
      "${virtio_console_test_periodic:-unknown}" \
      "${host_pause_resume_proof_ms:-unknown}" | sanitize_label
  else
    basename "$evidence_dir" | sanitize_label
  fi
}

derive_run_metadata() {
  local input="$1"
  local log="$2"
  local evidence_dir
  local preflight
  local status_file
  local target_stat
  local expected_vcpus=""
  local expected_desktop_agent=""
  local expected_desktop_checksum64=""
  local run_status="unknown"

  if [[ -d "$input" ]]; then
    evidence_dir="$input"
  else
    evidence_dir="$(dirname "$log")"
  fi
  preflight="$evidence_dir/preflight.txt"
  status_file="$evidence_dir/matrix-status.txt"
  target_stat="$evidence_dir/target-stat.txt"
  expected_vcpus="$(metadata_value "$preflight" smp_cpus)"
  expected_desktop_agent="$(metadata_value "$preflight" boot_timer_desktop_agent)"
  expected_desktop_checksum64="$(metadata_value "$preflight" boot_timer_desktop_checksum64)"
  expected_desktop_checksum64="$(canonical_u64_literal "$expected_desktop_checksum64")"
  if [[ -f "$status_file" ]]; then
    run_status="$(metadata_value "$status_file" status)"
  elif [[ -f "$target_stat" ]]; then
    run_status="$(metadata_value "$target_stat" run_status)"
  elif [[ -f "$preflight" ]]; then
    run_status="$(metadata_value "$preflight" matrix_status)"
  fi
  printf '%s\t%s\t%s\t%s\n' \
    "${expected_vcpus:-unknown}" \
    "${run_status:-unknown}" \
    "${expected_desktop_agent:-unknown}" \
    "${expected_desktop_checksum64:-unknown}"
}

emit_run_row() {
  local input="$1"
  local log
  local config
  local expected_vcpus
  local expected_desktop_agent
  local expected_desktop_checksum64
  local run_status

  log="$(resolve_log_path "$input")"
  config="$(derive_config_label "$input" "$log")"
  IFS=$'\t' read -r expected_vcpus run_status expected_desktop_agent expected_desktop_checksum64 < <(derive_run_metadata "$input" "$log")
  awk -v config="$config" -v source="$log" -v expected_vcpus="$expected_vcpus" -v run_status="$run_status" -v expected_desktop_agent="$expected_desktop_agent" -v expected_desktop_checksum64="$expected_desktop_checksum64" '
    function field_value(key, pos, rest) {
      pos = index($0, key "=")
      if (!pos) {
        return ""
      }
      rest = substr($0, pos + length(key) + 1)
      sub(/[[:space:]].*$/, "", rest)
      return rest
    }

    BEGIN {
      generation = 0
      found_start = 0
      summary_elapsed = ""
      desktop_elapsed = ""
      desktop_source = ""
      desktop_checksum = ""
      start_desktop_agent = ""
      start_desktop_checksum = ""
      desktop_reached = ""
      milestones = ""
      total_exits = 0
      total_exit_rate = 0
      vcpu_count = 0
      vcpu_ids_valid = 1
      metric_fields_valid = 1
      any_hv_vcpu_run_error = 0
    }

    index($0, "hv_vcpu_run error ") > 0 {
      any_hv_vcpu_run_error = 1
    }

    index($0, "BOOT_TIMER ") > 0 {
      if (index($0, "BOOT_TIMER start ") > 0) {
        for (cpu_id in seen_vcpu) {
          delete seen_vcpu[cpu_id]
        }
        generation += 1
        found_start = 1
        summary_elapsed = ""
        desktop_elapsed = ""
        desktop_source = ""
        desktop_checksum = ""
        start_desktop_agent = field_value("desktop_agent")
        start_desktop_checksum = field_value("desktop_checksum")
        desktop_reached = ""
        milestones = ""
        total_exits = 0
        total_exit_rate = 0
        vcpu_count = 0
        vcpu_ids_valid = 1
        metric_fields_valid = 1
        next
      }
      if (index($0, "BOOT_TIMER milestone name=desktop") > 0) {
        value = field_value("elapsed_ms")
        if (value != "") {
          desktop_elapsed = value
          if (value !~ /^[0-9]+$/) {
            metric_fields_valid = 0
          }
        }
        desktop_source = field_value("source")
        desktop_checksum = field_value("checksum64")
      }
      if (index($0, "BOOT_TIMER summary ") > 0) {
        summary_elapsed = field_value("elapsed_ms")
        if (summary_elapsed != "" && summary_elapsed !~ /^[0-9]+$/) {
          metric_fields_valid = 0
        }
        desktop_reached = field_value("desktop_reached")
        milestones = field_value("milestones")
      }
      if (index($0, "BOOT_TIMER vcpu ") > 0) {
        cpu = field_value("cpu")
        exits = field_value("exits")
        rate = field_value("exits_per_sec")
        if (cpu !~ /^[0-9]+$/) {
          vcpu_ids_valid = 0
        } else {
          cpu_id = sprintf("%d", cpu + 0)
          if (cpu_id in seen_vcpu) {
            vcpu_ids_valid = 0
          }
          seen_vcpu[cpu_id] = 1
        }
        if (exits ~ /^[0-9]+$/) {
          total_exits += exits + 0
        } else {
          metric_fields_valid = 0
        }
        if (rate ~ /^[0-9]+([.][0-9]+)?$/) {
          total_exit_rate += rate + 0
        } else {
          metric_fields_valid = 0
        }
        vcpu_count += 1
      }
    }

    END {
      reason = ""
      if (!found_start) {
        reason = "missing_start"
      }
      if (summary_elapsed == "") {
        reason = reason (reason == "" ? "" : ",") "missing_summary"
      }
      if (desktop_reached == "") {
        desktop_reached = "false"
      }
      if (desktop_reached != "true" || desktop_elapsed == "") {
        reason = reason (reason == "" ? "" : ",") "desktop_not_reached"
      }
      oracle_valid = 1
      if (found_start && desktop_elapsed != "") {
        if (expected_desktop_agent == "1" && (start_desktop_agent != "true" || desktop_source != "agent")) {
          oracle_valid = 0
        }
        if (expected_desktop_agent == "0" && (start_desktop_agent == "true" || desktop_source == "agent")) {
          oracle_valid = 0
        }
        checksum_expected = expected_desktop_checksum64 != "" && expected_desktop_checksum64 != "<unset>" && expected_desktop_checksum64 != "unknown"
        if (checksum_expected && (start_desktop_checksum == "" || start_desktop_checksum == "<unset>" || tolower(start_desktop_checksum) != tolower(expected_desktop_checksum64) || desktop_source == "agent" || desktop_checksum == "" || tolower(desktop_checksum) != tolower(start_desktop_checksum))) {
          oracle_valid = 0
        }
      }
      if (!oracle_valid) {
        reason = reason (reason == "" ? "" : ",") "desktop_oracle_mismatch"
      }
      if (vcpu_count == 0 || (expected_vcpus ~ /^[0-9]+$/ && vcpu_count != expected_vcpus + 0)) {
        reason = reason (reason == "" ? "" : ",") "vcpu_count_mismatch"
      } else if (expected_vcpus ~ /^[0-9]+$/) {
        for (cpu_id = 0; cpu_id < expected_vcpus + 0; cpu_id += 1) {
          if (!(sprintf("%d", cpu_id) in seen_vcpu)) {
            vcpu_ids_valid = 0
          }
        }
        for (cpu_id in seen_vcpu) {
          if (cpu_id + 0 >= expected_vcpus + 0) {
            vcpu_ids_valid = 0
          }
        }
      }
      if (!vcpu_ids_valid) {
        reason = reason (reason == "" ? "" : ",") "vcpu_ids_mismatch"
      }
      if (!metric_fields_valid) {
        reason = reason (reason == "" ? "" : ",") "metric_fields_invalid"
      }
      if (run_status ~ /^[0-9]+$/ && run_status + 0 != 0) {
        reason = reason (reason == "" ? "" : ",") "run_status_nonzero"
      }
      if (any_hv_vcpu_run_error) {
        reason = reason (reason == "" ? "" : ",") "hv_vcpu_run_error"
      }
      valid = reason == "" ? "true" : "false"
      printf "run\t%s\t%s\t%s\t%s\t%s\t%s\t%d\t%.2f\t%d\t%d\t%s\t%s\t%s\n", \
        config, source, summary_elapsed, desktop_elapsed, desktop_reached, \
        milestones, total_exits, total_exit_rate, vcpu_count, generation, \
        run_status, valid, reason
    }
  ' "$log"
}

median_for_field() {
  local rows="$1"
  local config="$2"
  local field="$3"
  local values=()
  local value
  local count
  local mid

  while IFS= read -r value; do
    values+=("$value")
  done < <(awk -F '\t' -v config="$config" -v field="$field" '
    $2 == config && $13 == "true" && $field ~ /^[0-9]+([.][0-9]+)?$/ { print $field }
  ' "$rows" | sort -n)

  count="${#values[@]}"
  if (( count == 0 )); then
    printf ''
    return 0
  fi
  mid=$((count / 2))
  if (( count % 2 == 1 )); then
    printf '%s' "${values[$mid]}"
  else
    awk -v a="${values[$((mid - 1))]}" -v b="${values[$mid]}" \
      'BEGIN { printf "%.2f", (a + b) / 2 }'
  fi
}

[[ $# -gt 0 ]] || { usage; exit 2; }

ROWS="$(mktemp "${TMPDIR:-/tmp}/bridgevm-boot-timer.XXXXXX")"
trap 'rm -f "$ROWS"' EXIT

for input in "$@"; do
  emit_run_row "$input" >>"$ROWS"
done

printf 'section\tconfig\tsource\tsummary_elapsed_ms\tdesktop_elapsed_ms\tdesktop_reached\tmilestones\ttotal_exits\ttotal_exits_per_sec\tvcpus\tgeneration\trun_status\tvalid\tinvalid_reason\n'
cat "$ROWS"

printf '\n'
printf 'section\tconfig\truns\tmedian_summary_elapsed_ms\tmedian_desktop_elapsed_ms\tmedian_total_exits\tmedian_total_exits_per_sec\tvalid_runs\tinvalid_runs\n'
while IFS= read -r config; do
  [[ -n "$config" ]] || continue
  runs="$(awk -F '\t' -v config="$config" '$2 == config { count += 1 } END { print count + 0 }' "$ROWS")"
  valid_runs="$(awk -F '\t' -v config="$config" '$2 == config && $13 == "true" { count += 1 } END { print count + 0 }' "$ROWS")"
  invalid_runs="$((runs - valid_runs))"
  printf 'median\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$config" \
    "$runs" \
    "$(median_for_field "$ROWS" "$config" 4)" \
    "$(median_for_field "$ROWS" "$config" 5)" \
    "$(median_for_field "$ROWS" "$config" 8)" \
    "$(median_for_field "$ROWS" "$config" 9)" \
    "$valid_runs" \
    "$invalid_runs"
done < <(awk -F '\t' '{ print $2 }' "$ROWS" | sort -u)

if awk -F '\t' '$13 != "true" { invalid = 1 } END { exit !invalid }' "$ROWS"; then
  echo "FAIL: one or more BOOT_TIMER runs are invalid; inspect invalid_reason" >&2
  exit 1
fi
