# Each logger is a direct shell child; no logger sends process signals.
# FD9 releases FIFO startup, then is closed in the native probe and parent.
BOUNDED_FIFO=""
BOUNDED_FIFO_CREATED=0
BOUNDED_FIFO_IDENTITY=""
BOUNDED_FD9_OPEN=0
BOUNDED_LOGGER_PID=""

bounded_fifo_path() {
  /usr/bin/python3 - "$1" "$2" "${3:-}" <<'PY'
import os, stat, sys
action, path, expected = sys.argv[1:]
try:
    info = os.fstat(9) if action == "descriptor" else os.lstat(path)
    if not stat.S_ISFIFO(info.st_mode): raise ValueError("not an owned FIFO")
    identity = str(info.st_dev) + ":" + str(info.st_ino)
    if action == "identity": print(identity)
    elif action == "descriptor" and identity == expected: pass
    elif action == "remove" and identity == expected: os.unlink(path)
    else: raise ValueError("FIFO ownership changed")
except (OSError, ValueError):
    sys.exit(1)
PY
}

record_bounded_output_refusal() {
  # This fixed token in the bounded wrapper/cleanup log also covers marker I/O failure.
  echo 'DiagnosticOutputBoundRefusal' >&2
  /usr/bin/python3 - "$EVIDENCE_DIR/output-bound-refused.json" <<'PY'
import os, sys
path = sys.argv[1]
try:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
except FileExistsError:
    sys.exit(0)  # Existing paths are preserved and the verifier refuses them.
try:
    data = memoryview(b'{"schema":"bridgevm.output-bound-refusal.v1","refused":true}\n')
    while data:
        count = os.write(fd, data)
        if count <= 0: raise OSError("refusal write failed")
        data = data[count:]
    os.fsync(fd)
finally:
    os.close(fd)
directory = os.open(os.path.dirname(path), os.O_RDONLY | os.O_DIRECTORY)
try: os.fsync(directory)
finally: os.close(directory)
PY
}

start_bounded_file() {
  local output="$1" status="$2" limit="$3" count=0 token
  token="$(/usr/bin/python3 -c 'import secrets; print(secrets.token_hex(16))')" || { record_bounded_output_refusal; return 1; }
  [[ ! -e /dev/fd/9 ]] || { record_bounded_output_refusal; echo 'FAIL: diagnostic logger requires unused fd9' >&2; return 1; }
  BOUNDED_FIFO="$output.pipe"
  BOUNDED_FIFO_CREATED=0
  BOUNDED_FIFO_IDENTITY=""
  BOUNDED_FD9_OPEN=0
  BOUNDED_LOGGER_PID=""
  mkfifo -m 600 "$BOUNDED_FIFO" || { record_bounded_output_refusal; return 1; }
  BOUNDED_FIFO_CREATED=1
  BOUNDED_FIFO_IDENTITY="$(bounded_fifo_path identity "$BOUNDED_FIFO")" || { record_bounded_output_refusal; return 1; }
  /usr/bin/python3 "$ROOT/scripts/live-gates/bounded_output.py" --output "$output" \
    --status "$status" --limit "$limit" --token "$token" < "$BOUNDED_FIFO" &
  BOUNDED_LOGGER_PID="$!"
  exec 9<> "$BOUNDED_FIFO"
  BOUNDED_FD9_OPEN=1
  if ! bounded_fifo_path descriptor "$BOUNDED_FIFO" "$BOUNDED_FIFO_IDENTITY"; then
    finish_bounded_file || true
    record_bounded_output_refusal
    return 1
  fi
  while ! /usr/bin/python3 "$ROOT/scripts/live-gates/bounded_output.py" --check-ready "$status" --limit "$limit" --token "$token" >/dev/null 2>&1; do
    if (( count >= 20 )); then
      finish_bounded_file || true
      record_bounded_output_refusal
      echo 'FAIL: diagnostic logger did not publish owned readiness' >&2
      return 1
    fi
    sleep 0.05
    count=$((count + 1))
  done
}

finish_bounded_file() {
  local status=0
  if [[ "${BOUNDED_FD9_OPEN:-0}" == 1 ]]; then
    exec 9>&-
    BOUNDED_FD9_OPEN=0
  fi
  if [[ -n "${BOUNDED_LOGGER_PID:-}" ]]; then
    wait "$BOUNDED_LOGGER_PID" || status=1
    BOUNDED_LOGGER_PID=""
  fi
  if [[ "${BOUNDED_FIFO_CREATED:-0}" == 1 ]]; then
    if bounded_fifo_path remove "$BOUNDED_FIFO" "$BOUNDED_FIFO_IDENTITY"; then
      BOUNDED_FIFO_CREATED=0
    else
      status=1
    fi
  fi
  [[ "$status" == 0 ]] || record_bounded_output_refusal
  return "$status"
}

run_probe_process() {
  local name
  local -a env_command=(/usr/bin/env)
  # An installed-boot run is a closed, auditable configuration boundary.
  # Remove every inherited BridgeVM probe knob, then apply only ENV_ARGS built
  # from this wrapper's validated CLI. This prevents an old developer shell
  # from attaching a second writable disk, changing PCI topology, injecting
  # guest input, or enabling agent share/clipboard commands behind the
  # recorded policy.
  while IFS= read -r name; do
    case "$name" in
      BRIDGEVM_*) env_command+=(-u "$name") ;;
    esac
  done < <(compgen -e)
  HOST_PAUSE_RESUME_CONTROL_STATUS=0
  if [[ -n "${HOST_PAUSE_RESUME_PROOF_MS:-}" ]]; then
    : > "$(host_pause_resume_control_path)"
  fi
  prepare_virtio_gpu_trace
  set +e
  local output="$EVIDENCE_DIR/run.log"
  if [[ "${DIAGNOSTIC_OUTPUT_BOUNDS:-0}" == 1 ]]; then
    if ! start_bounded_file "$output" "$EVIDENCE_DIR/run-log-bound.json" 536870912; then
      RUN_STATUS=1
      set -e
      return 0
    fi
  fi
  if [[ "${DIAGNOSTIC_OUTPUT_BOUNDS:-0}" == 1 ]]; then
    "${env_command[@]}" "${ENV_ARGS[@]}" "$BIN" >&9 2>&1 9>&- &
  else
    "${env_command[@]}" "${ENV_ARGS[@]}" "$BIN" > "$output" 2>&1 &
  fi
  PROBE_PID="$!"
  if [[ "${BOUNDED_FD9_OPEN:-0}" == 1 ]]; then
    exec 9>&-
    BOUNDED_FD9_OPEN=0
  fi
  if [[ -n "${HOST_PAUSE_RESUME_PROOF_MS:-}" ]]; then
    if ! drive_host_pause_resume_proof; then
      HOST_PAUSE_RESUME_CONTROL_STATUS=1
      terminate_owned_probe
    fi
  fi
  wait "$PROBE_PID"
  RUN_STATUS="$?"
  PROBE_PID=""
  finish_bounded_file || RUN_STATUS=1
  set -e
}

write_installed_boot_target_stat_body() {
  {
    printf 'run_status=%s\n' "$RUN_STATUS"
    date -u
    if [[ "${VIRTIO_GPU_3D:-0}" == "1" ]]; then
      printf 'virtio_gpu_trace=%s\n' "$(virtio_gpu_trace_path)"
      printf 'probe_build_capabilities=%s\n' "$EVIDENCE_DIR/probe-build-capabilities.txt"
      printf 'virtio_gpu_trace_report=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-report.txt"
      printf 'virtio_gpu_trace_gate=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt"
      printf 'p3_gpu_readiness=%s\n' "$EVIDENCE_DIR/p3-gpu-readiness.txt"
      printf 'viogpu3d_package_manifest=%s\n' "$EVIDENCE_DIR/viogpu3d-package-manifest.txt"
      printf 'real_title_gate=%s\n' "$EVIDENCE_DIR/real-title-gate.txt"
      if (( TITLE_MANIFEST_COUNT > 0 )); then
        printf 'title_gate_report=%s\n' "$EVIDENCE_DIR/title-gates.txt"
        printf 'title_gate_json=%s\n' "$EVIDENCE_DIR/title-gates.json"
        printf 'title_gate_status=%s\n' "$EVIDENCE_DIR/title-gates-gate.txt"
        printf 'title_pre_run_state=%s\n' "$EVIDENCE_DIR/title-pre-run-state.json"
      fi
    fi
    if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
      printf 'agent_shutdown_gate=%s\n' "$EVIDENCE_DIR/agent-shutdown-gate.txt"
    fi
    if [[ -n "$AGENT_SERVICE_CONTROL" ]]; then
      printf 'agent_service_gate=%s\n' "$EVIDENCE_DIR/agent-service-gate.txt"
    fi
    if [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" ]]; then
      printf 'host_pause_resume_gate=%s\n' "$EVIDENCE_DIR/host-pause-resume-gate.txt"
      printf 'host_pause_resume_observation=%s\n' "$(host_pause_resume_observation_path)"
    fi
    print_media_stat after_target_stat "$TARGET"
    printf 'injector_boot_observed=%s\n' "$(injector_boot_observed)"
    printf 'after_vars_stat:\n'
    ls -lh "$VARS"
    printf 'ramfb_files:\n'
    find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
    printf 'run_log_summary_grep:\n'
    grep -En 'Windows|Boot Manager|UEFI|EFI|Bds|Boot####|NVMe|xHCI|qemu-xhci|HID|USB|PNP|BVAGENT|INTERNAL_POWER_ERROR|DRIVER_PNP_WATCHDOG|0x1D5|bugcheck|panic|HV_DENIED|hv_vm_create|watchdog|SYSTEM_RESET|SYSTEM_OFF|PSCI|storage target effect|exact_target_storage_evidence|target_effect_class' "$EVIDENCE_DIR/run.log" || true
  }
}

write_installed_boot_target_stat() {
  if [[ "${DIAGNOSTIC_OUTPUT_BOUNDS:-0}" != 1 ]]; then
    write_installed_boot_target_stat_body > "$EVIDENCE_DIR/target-stat.txt" 2>&1
    return
  fi
  if ! start_bounded_file "$EVIDENCE_DIR/target-stat.txt" "$EVIDENCE_DIR/target-stat-bound.json" 536870912; then
    RUN_STATUS=1
    return
  fi
  write_installed_boot_target_stat_body >&9 2>&1 9>&-
  finish_bounded_file || RUN_STATUS=1
}

write_installed_boot_cleanup_body() {
  local cleanup_fifo="$1" cleanup_fifo_identity="$2"
  {
    printf '\ncleanup_status=%s\n' "$status"
    date -u
    printf 'processes_before_cleanup:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    terminate_owned_probe
    if [[ -n "${prior_logger:-}" ]]; then
      wait "$prior_logger" || status=1
      prior_logger=""
    fi
    if [[ "${prior_fifo_created:-0}" == 1 ]]; then
      if bounded_fifo_path remove "$cleanup_fifo" "$cleanup_fifo_identity"; then
        prior_fifo_created=0
      else
        record_bounded_output_refusal
        status=1
      fi
    fi
    terminate_owned_swtpm
    cleanup_owned_swtpm_runtime
    printf 'processes_after_cleanup:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    printf 'tmux_sessions_after_cleanup:\n'
    tmux ls 2>/dev/null || true
  } }
