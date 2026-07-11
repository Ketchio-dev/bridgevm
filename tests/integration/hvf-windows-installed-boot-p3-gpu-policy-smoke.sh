#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-installed-p3-gpu-policy.XXXXXX")"
TARGET="$STORE/windows-target.raw"
VARS="$STORE/vars.fd"
EVIDENCE="$STORE/evidence"
TRACE="$STORE/evidence/p3-gpu.jsonl"
VIOGPU3D="$STORE/viogpu3d"
SHARE="$STORE/share"
CONTROL="$EVIDENCE/app.ctl"

touch "$TARGET" "$VARS"
mkdir -p "$EVIDENCE" "$VIOGPU3D" "$SHARE"

cleanup() {
  rm -rf "$STORE"
}
trap cleanup EXIT

write_minimal_pe() {
  local path="$1"
  local machine_low_octal="$2"
  local machine_high_octal="$3"

  dd if=/dev/zero of="$path" bs=512 count=1 >/dev/null 2>&1
  printf 'MZ' | dd of="$path" bs=1 seek=0 conv=notrunc >/dev/null 2>&1
  printf '\200\000\000\000' | dd of="$path" bs=1 seek=60 conv=notrunc >/dev/null 2>&1
  printf "PE\000\000\\$machine_low_octal\\$machine_high_octal" |
    dd of="$path" bs=1 seek=128 conv=notrunc >/dev/null 2>&1
}

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

cat >"$VIOGPU3D/viogpu3d.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=venus
INF
write_minimal_pe "$VIOGPU3D/viogpu3d.sys" 144 252
printf 'fake catalog\n' >"$VIOGPU3D/viogpu3d.cat"

output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace "$TRACE" \
    --gpu-trace-protocol venus \
    --require-gpu-trace-gate \
    --viogpu3d-dir "$VIOGPU3D" \
    --require-viogpu3d-readiness \
    --print-policy 2>&1
)" || fail "installed boot P3 GPU policy failed: $output"

assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=$TRACE" "installed boot policy"
assert_contains "$output" "BRIDGEVM_GPU_TRACE_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_GPU_TRACE_GATE=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIOGPU3D_DIR=$VIOGPU3D" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_VIOGPU3D_READINESS=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_RAM_MIB=4096" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=900000" "installed boot policy"
assert_contains "$output" "BRIDGEVM_SMP_CPUS=<unset> (probe default 1)" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=<probe-default 1000>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=<unset>" "installed boot policy"
assert_contains "$output" "SHUTDOWN_AFTER_AGENT_READY=0" "installed boot policy"
assert_contains "$output" "HOST_PAUSE_RESUME_PROOF_MS=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_CTL=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE=<unset>" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=<unset>" "installed boot policy"
assert_contains "$output" "BUILD_PROFILE=debug" "installed boot policy"
assert_contains "$output" "BRIDGEVM_NVME_DISK_WRITABLE=1 when booting target as only NVMe" "installed boot policy"

boot_timer_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --print-policy 2>&1
)" || fail "installed boot timer policy failed: $boot_timer_output"

assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer policy"
assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=250" "boot timer policy"
assert_contains "$boot_timer_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234abcd" "boot timer policy"

agent_timer_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-agent \
    --print-policy 2>&1
)" || fail "installed boot agent timer policy failed: $agent_timer_output"
assert_contains "$agent_timer_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1" "agent timer policy"
assert_contains "$agent_timer_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "agent timer policy"

shutdown_policy_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 234567 \
    --shutdown-after-agent-ready \
    --print-policy 2>&1
)" || fail "installed boot agent shutdown policy failed: $shutdown_policy_output"
assert_contains "$shutdown_policy_output" "SHUTDOWN_AFTER_AGENT_READY=1" "agent shutdown policy"
assert_contains "$shutdown_policy_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "agent shutdown policy"
assert_contains "$shutdown_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "agent shutdown policy"
assert_contains "$shutdown_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "agent shutdown policy"
assert_contains "$shutdown_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=shutdown.exe /p /f" "agent shutdown policy"
assert_contains "$shutdown_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=234567" "agent shutdown policy"

pause_policy_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 234567 \
    --host-pause-resume-proof-ms 1500 \
    --print-policy 2>&1
)" || fail "installed boot host pause/resume policy failed: $pause_policy_output"
assert_contains "$pause_policy_output" "HOST_PAUSE_RESUME_PROOF_MS=1500" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=ver" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=234567" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1" "host pause/resume policy"
assert_contains "$pause_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CTL=$EVIDENCE/host-pause-resume-control.txt" "host pause/resume policy"

service_policy_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 345678 \
    --ram-mib 6144 \
    --smp-cpus 4 \
    --agent-service-control "$CONTROL" \
    --agent-service-command "whoami /user" \
    --agent-clipboard-sync \
    --agent-share-host "$SHARE" \
    --agent-share-guest 'C:\bridgevm-share' \
    --agent-share-ms 2500 \
    --print-policy 2>&1
)" || fail "installed boot app service policy failed: $service_policy_output"
assert_contains "$service_policy_output" "BRIDGEVM_RAM_MIB=6144" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_SMP_CPUS=4" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=whoami /user" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=345678" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CTL=$CONTROL" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=1" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE=$SHARE::C:\bridgevm-share" "app service policy"
assert_contains "$service_policy_output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=2500" "app service policy"

boot_timer_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd
)" || fail "installed boot timer env failed: $boot_timer_env_output"

assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer env"
assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=250" "boot timer env"
assert_contains "$boot_timer_env_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234abcd" "boot timer env"

agent_timer_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-agent
)" || fail "installed boot agent timer env failed: $agent_timer_env_output"
assert_contains "$agent_timer_env_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1" "agent timer env"
assert_contains "$agent_timer_env_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "agent timer env"

shutdown_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 234567 \
    --shutdown-after-agent-ready
)" || fail "installed boot agent shutdown env failed: $shutdown_env_output"
assert_contains "$shutdown_env_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "agent shutdown env"
assert_contains "$shutdown_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "agent shutdown env"
assert_contains "$shutdown_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "agent shutdown env"
assert_contains "$shutdown_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=shutdown.exe /p /f" "agent shutdown env"
assert_contains "$shutdown_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=234567" "agent shutdown env"

pause_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 234567 \
    --host-pause-resume-proof-ms 1500
)" || fail "installed boot host pause/resume env failed: $pause_env_output"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "host pause/resume env"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "host pause/resume env"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "host pause/resume env"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=ver" "host pause/resume env"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1" "host pause/resume env"
assert_contains "$pause_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CTL=$EVIDENCE/host-pause-resume-control.txt" "host pause/resume env"

service_env_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-validation.sh
    source scripts/run-hvf-windows-installed-boot-args.sh
    source scripts/run-hvf-windows-installed-boot-runner.sh
    init_installed_boot_defaults
    parse_installed_boot_args "$@"
    build_installed_boot_env_args
  ' _ \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 345678 \
    --agent-service-control "$CONTROL" \
    --agent-service-command "whoami /user" \
    --agent-clipboard-sync \
    --agent-share-host "$SHARE" \
    --agent-share-guest 'C:\bridgevm-share' \
    --agent-share-ms 2500
)" || fail "installed boot app service env failed: $service_env_output"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE=1" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST=1" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CMDS=whoami /user" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=345678" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CTL=$CONTROL" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=1" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE=$SHARE::C:\bridgevm-share" "app service env"
assert_contains "$service_env_output" "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=2500" "app service env"

daily_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --daily \
    --print-policy 2>&1
)" || fail "installed boot daily policy failed: $daily_output"

assert_contains "$daily_output" "DAILY_PRESET=1" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_RAM_MIB=6144" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=86400000" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_SMP_CPUS=4" "daily policy"
assert_contains "$daily_output" "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30" "daily policy"
assert_contains "$daily_output" "BUILD_PROFILE=release" "daily policy"

daily_override_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --daily \
    --ram-mib 2048 \
    --watchdog-ms 12345 \
    --smp-cpus 1 \
    --print-policy 2>&1
)" || fail "installed boot daily override policy failed: $daily_override_output"

assert_contains "$daily_override_output" "BRIDGEVM_RAM_MIB=2048" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=12345" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_SMP_CPUS=1" "daily override policy"
assert_contains "$daily_override_output" "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30" "daily override policy"
assert_contains "$daily_override_output" "BUILD_PROFILE=release" "daily override policy"

bad_smp_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --smp-cpus 0 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid SMP count: $bad_smp_output"

assert_contains "$bad_smp_output" "--smp-cpus requires an integer from 1 to 123" "invalid SMP policy"

huge_smp_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --smp-cpus 999999999999999999999999999999999999 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted overflowing SMP count: $huge_smp_output"
assert_contains "$huge_smp_output" "--smp-cpus requires an integer from 1 to 123" "overflowing SMP policy"

bad_boot_timer_ms_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 99 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid boot timer interval: $bad_boot_timer_ms_output"

assert_contains "$bad_boot_timer_ms_output" "--boot-timer-ramfb-ms requires an integer from 100 to 60000" "invalid boot timer interval policy"

bad_pause_ms_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --host-pause-resume-proof-ms 99 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid host pause interval: $bad_pause_ms_output"
assert_contains "$bad_pause_ms_output" "--host-pause-resume-proof-ms requires an integer from 100 to 60000" "invalid host pause interval policy"

conflicting_shutdown_pause_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --shutdown-after-agent-ready \
    --host-pause-resume-proof-ms 1000 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted conflicting shutdown/pause controls: $conflicting_shutdown_pause_output"
assert_contains "$conflicting_shutdown_pause_output" "controls its own post-resume shutdown" "shutdown/pause conflict policy"

orphan_agent_option_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-clipboard-sync \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted an agent option without service control: $orphan_agent_option_output"
assert_contains "$orphan_agent_option_output" "require --agent-service-control" "orphan agent option policy"

conflicting_service_shutdown_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --shutdown-after-agent-ready \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted service and one-shot shutdown: $conflicting_service_shutdown_output"
assert_contains "$conflicting_service_shutdown_output" "cannot be combined with one-shot shutdown" "service/shutdown conflict policy"

orphan_share_path_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --agent-share-host "$SHARE" \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted an unpaired share path: $orphan_share_path_output"
assert_contains "$orphan_share_path_output" "must be provided together" "unpaired share path policy"

bad_share_interval_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --agent-share-ms 499 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted a short share interval: $bad_share_interval_output"
assert_contains "$bad_share_interval_output" "--agent-share-ms requires an integer from 500 to 60000" "share interval policy"

orphan_share_interval_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --agent-share-ms 2500 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted a share interval without share paths: $orphan_share_interval_output"
assert_contains "$orphan_share_interval_output" "requires --agent-share-host and --agent-share-guest" "orphan share interval policy"

bad_agent_command_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --agent-service-command 'whoami|shutdown.exe /p /f' \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted an agent command delimiter: $bad_agent_command_output"
assert_contains "$bad_agent_command_output" "without CR, LF, or |" "agent command delimiter policy"

MISSING_SHARE="$STORE/missing-share"
missing_share_dir_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --agent-service-control "$CONTROL" \
    --agent-share-host "$MISSING_SHARE" \
    --agent-share-guest 'C:\missing' \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted a missing host share directory: $missing_share_dir_output"
assert_contains "$missing_share_dir_output" "agent share host directory not found" "missing host share directory policy"

huge_boot_timer_ms_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-ramfb-ms 999999999999999999999999999999999999 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted overflowing boot timer interval: $huge_boot_timer_ms_output"
assert_contains "$huge_boot_timer_ms_output" "--boot-timer-ramfb-ms requires an integer from 100 to 60000" "overflowing boot timer policy"

bad_boot_timer_checksum_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-checksum64 0x10000000000000000 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted invalid boot timer checksum: $bad_boot_timer_checksum_output"

assert_contains "$bad_boot_timer_checksum_output" "--boot-timer-desktop-checksum64 requires a u64 decimal or 0x-prefixed hex value" "invalid boot timer checksum policy"

conflicting_oracle_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer-desktop-agent \
    --boot-timer-desktop-checksum64 0x1 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted two desktop oracles: $conflicting_oracle_output"
assert_contains "$conflicting_oracle_output" "choose exactly one BOOT_TIMER desktop oracle" "desktop oracle conflict"

bad_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=1 scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted BRIDGEVM_SMP_TRACE with boot timer: $bad_smp_trace_output"

assert_contains "$bad_smp_trace_output" "--boot-timer cannot be measured with BRIDGEVM_SMP_TRACE=1" "boot timer smp trace rejection"

trimmed_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=' true ' scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted whitespace-padded true SMP trace: $trimmed_smp_trace_output"
assert_contains "$trimmed_smp_trace_output" "--boot-timer cannot be measured with BRIDGEVM_SMP_TRACE= true " "trimmed SMP trace rejection"

false_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=0 scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" || fail "installed boot rejected falsey BRIDGEVM_SMP_TRACE with boot timer: $false_smp_trace_output"

assert_contains "$false_smp_trace_output" "BRIDGEVM_BOOT_TIMER=1" "boot timer falsey smp trace policy"

trimmed_false_smp_trace_output="$(
  BRIDGEVM_SMP_TRACE=' false ' scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --boot-timer \
    --print-policy 2>&1
)" || fail "installed boot rejected whitespace-padded false SMP trace: $trimmed_false_smp_trace_output"
assert_contains "$trimmed_false_smp_trace_output" "BRIDGEVM_BOOT_TIMER=1" "trimmed false SMP trace policy"

FAKE_PROBE="$STORE/fake-probe.sh"
SANITIZE_EVIDENCE="$STORE/sanitize-evidence"
mkdir -p "$SANITIZE_EVIDENCE"
cat >"$FAKE_PROBE" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
printf 'smp=%s\n' "${BRIDGEVM_SMP_CPUS-<unset>}"
printf 'timer=%s\n' "${BRIDGEVM_BOOT_TIMER-<unset>}"
printf 'timer_ms=%s\n' "${BRIDGEVM_BOOT_TIMER_RAMFB_MS-<unset>}"
printf 'desktop=%s\n' "${BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64-<unset>}"
printf 'desktop_agent=%s\n' "${BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT-<unset>}"
printf 'nvme=%s\n' "${BRIDGEVM_NVME_DISK-<unset>}"
printf 'nvme2=%s\n' "${BRIDGEVM_NVME_DISK2-<unset>}"
printf 'nvme2_writable=%s\n' "${BRIDGEVM_NVME_DISK2_WRITABLE-<unset>}"
printf 'installer=%s\n' "${BRIDGEVM_INSTALLER_ISO-<unset>}"
printf 'disable_xhci=%s\n' "${BRIDGEVM_DISABLE_XHCI-<unset>}"
printf 'xhci_interval=%s\n' "${BRIDGEVM_XHCI_REPORT_INTERVAL_MS-<unset>}"
printf 'net=%s\n' "${BRIDGEVM_VIRTIO_NET-<unset>}"
printf 'gpu=%s\n' "${BRIDGEVM_VIRTIO_GPU-<unset>}"
printf 'console=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE-<unset>}"
printf 'setup_actions=%s\n' "${BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS-<unset>}"
printf 'console_test=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_TEST-<unset>}"
printf 'console_test_periodic=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC-<unset>}"
printf 'console_cmds=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_CMDS-<unset>}"
printf 'console_timeout=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS-<unset>}"
printf 'console_service=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_SERVICE-<unset>}"
printf 'console_ctl=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_CTL-<unset>}"
printf 'console_clipsync=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC-<unset>}"
printf 'console_share=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_SHARE-<unset>}"
printf 'console_share_ms=%s\n' "${BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS-<unset>}"
printf 'unknown_poison=%s\n' "${BRIDGEVM_UNKNOWN_POISON-<unset>}"
printf 'vars=%s\n' "${BRIDGEVM_AARCH64_UEFI_VARS-<unset>}"
printf 'gpu_trace=%s\n' "${BRIDGEVM_VIRTIO_GPU_TRACE_JSONL-<unset>}"
PROBE
chmod +x "$FAKE_PROBE"

sanitized_probe_output="$(
  BRIDGEVM_SMP_CPUS=99 \
  BRIDGEVM_BOOT_TIMER=1 \
  BRIDGEVM_BOOT_TIMER_RAMFB_MS=777 \
  BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0xdead \
  BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1 \
  BRIDGEVM_NVME_DISK=/tmp/poison-disk.raw \
  BRIDGEVM_NVME_DISK2=/tmp/poison-disk2.raw \
  BRIDGEVM_NVME_DISK2_WRITABLE=1 \
  BRIDGEVM_INSTALLER_ISO=/tmp/poison.iso \
  BRIDGEVM_DISABLE_XHCI=1 \
  BRIDGEVM_XHCI_REPORT_INTERVAL_MS=999 \
  BRIDGEVM_VIRTIO_NET=1 \
  BRIDGEVM_VIRTIO_GPU=1 \
  BRIDGEVM_VIRTIO_CONSOLE=1 \
  BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS='win+r,text:poison,enter' \
  BRIDGEVM_VIRTIO_CONSOLE_TEST=1 \
  BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1 \
  BRIDGEVM_VIRTIO_CONSOLE_CMDS=poison \
  BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=777 \
  BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1 \
  BRIDGEVM_VIRTIO_CONSOLE_CTL=/tmp/poison-control \
  BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=1 \
  BRIDGEVM_VIRTIO_CONSOLE_SHARE=/tmp/poison-share \
  BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=777 \
  BRIDGEVM_UNKNOWN_POISON=1 \
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    BIN="$1"
    EVIDENCE_DIR="$2"
    ENV_ARGS=(BRIDGEVM_RAM_MIB=4096)
    PROBE_PID=""
    run_probe_process
    cat "$EVIDENCE_DIR/run.log"
  ' _ "$FAKE_PROBE" "$SANITIZE_EVIDENCE"
)" || fail "ambient CLI-owned env sanitization failed: $sanitized_probe_output"
assert_contains "$sanitized_probe_output" "smp=<unset>" "ambient SMP sanitization"
assert_contains "$sanitized_probe_output" "timer=<unset>" "ambient timer sanitization"
assert_contains "$sanitized_probe_output" "timer_ms=<unset>" "ambient timer interval sanitization"
assert_contains "$sanitized_probe_output" "desktop=<unset>" "ambient desktop checksum sanitization"
assert_contains "$sanitized_probe_output" "desktop_agent=<unset>" "ambient desktop agent sanitization"
assert_contains "$sanitized_probe_output" "nvme=<unset>" "ambient primary disk sanitization"
assert_contains "$sanitized_probe_output" "nvme2=<unset>" "ambient second disk sanitization"
assert_contains "$sanitized_probe_output" "nvme2_writable=<unset>" "ambient writable second disk sanitization"
assert_contains "$sanitized_probe_output" "installer=<unset>" "ambient installer sanitization"
assert_contains "$sanitized_probe_output" "disable_xhci=<unset>" "ambient xHCI policy sanitization"
assert_contains "$sanitized_probe_output" "xhci_interval=<unset>" "ambient xHCI pacing sanitization"
assert_contains "$sanitized_probe_output" "net=<unset>" "ambient network sanitization"
assert_contains "$sanitized_probe_output" "gpu=<unset>" "ambient GPU sanitization"
assert_contains "$sanitized_probe_output" "console=<unset>" "ambient console sanitization"
assert_contains "$sanitized_probe_output" "setup_actions=<unset>" "ambient input sanitization"
assert_contains "$sanitized_probe_output" "console_test=<unset>" "ambient console test sanitization"
assert_contains "$sanitized_probe_output" "console_test_periodic=<unset>" "ambient periodic console test sanitization"
assert_contains "$sanitized_probe_output" "console_cmds=<unset>" "ambient console command sanitization"
assert_contains "$sanitized_probe_output" "console_timeout=<unset>" "ambient console timeout sanitization"
assert_contains "$sanitized_probe_output" "console_service=<unset>" "ambient console service sanitization"
assert_contains "$sanitized_probe_output" "console_ctl=<unset>" "ambient console control sanitization"
assert_contains "$sanitized_probe_output" "console_clipsync=<unset>" "ambient console clipboard sanitization"
assert_contains "$sanitized_probe_output" "console_share=<unset>" "ambient console share sanitization"
assert_contains "$sanitized_probe_output" "console_share_ms=<unset>" "ambient console share interval sanitization"
assert_contains "$sanitized_probe_output" "unknown_poison=<unset>" "ambient unknown BridgeVM sanitization"

explicit_probe_output="$(
  BRIDGEVM_SMP_CPUS=99 \
  BRIDGEVM_BOOT_TIMER=0 \
  BRIDGEVM_BOOT_TIMER_RAMFB_MS=777 \
  BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0xdead \
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    BIN="$1"
    EVIDENCE_DIR="$2"
    ENV_ARGS=(
      BRIDGEVM_SMP_CPUS=2
      BRIDGEVM_BOOT_TIMER=1
      BRIDGEVM_BOOT_TIMER_RAMFB_MS=250
      BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234
    )
    PROBE_PID=""
    run_probe_process
    cat "$EVIDENCE_DIR/run.log"
  ' _ "$FAKE_PROBE" "$SANITIZE_EVIDENCE"
)" || fail "explicit CLI-owned env application failed: $explicit_probe_output"
assert_contains "$explicit_probe_output" "smp=2" "explicit SMP env"
assert_contains "$explicit_probe_output" "timer=1" "explicit timer env"
assert_contains "$explicit_probe_output" "timer_ms=250" "explicit timer interval env"
assert_contains "$explicit_probe_output" "desktop=0x1234" "explicit desktop checksum env"

SHUTDOWN_GATE_OK="$STORE/shutdown-gate-ok"
mkdir -p "$SHUTDOWN_GATE_OK"
cat > "$SHUTDOWN_GATE_OK/run.log" <<'EOF'
BVAGENT READY host=BRIDGEVM v3-share2 t=1234
stop: PSCI 0x84000008 (system off)
EOF
shutdown_gate_ok_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    EVIDENCE_DIR="$1"
    SHUTDOWN_AFTER_AGENT_READY=1
    RUN_STATUS=0
    write_agent_shutdown_gate
    cat "$EVIDENCE_DIR/agent-shutdown-gate.txt"
    printf "run_status=%s\n" "$RUN_STATUS"
  ' _ "$SHUTDOWN_GATE_OK"
)" || fail "successful agent shutdown gate failed: $shutdown_gate_ok_output"
assert_contains "$shutdown_gate_ok_output" "agent_handshake=true" "successful agent shutdown gate"
assert_contains "$shutdown_gate_ok_output" "guest_system_off=true" "successful agent shutdown gate"
assert_contains "$shutdown_gate_ok_output" "status=0" "successful agent shutdown gate"
assert_contains "$shutdown_gate_ok_output" "run_status=0" "successful agent shutdown gate"

SHUTDOWN_GATE_FAIL="$STORE/shutdown-gate-fail"
mkdir -p "$SHUTDOWN_GATE_FAIL"
printf 'BVAGENT READY host=BRIDGEVM v3-share2 t=1234\n' > "$SHUTDOWN_GATE_FAIL/run.log"
shutdown_gate_fail_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    EVIDENCE_DIR="$1"
    SHUTDOWN_AFTER_AGENT_READY=1
    RUN_STATUS=0
    write_agent_shutdown_gate
    cat "$EVIDENCE_DIR/agent-shutdown-gate.txt"
    printf "run_status=%s\n" "$RUN_STATUS"
  ' _ "$SHUTDOWN_GATE_FAIL"
)" || fail "failed agent shutdown gate evaluation errored: $shutdown_gate_fail_output"
assert_contains "$shutdown_gate_fail_output" "agent_handshake=true" "failed agent shutdown gate"
assert_contains "$shutdown_gate_fail_output" "guest_system_off=false" "failed agent shutdown gate"
assert_contains "$shutdown_gate_fail_output" "status=1" "failed agent shutdown gate"
assert_contains "$shutdown_gate_fail_output" "run_status=1" "failed agent shutdown gate"

SERVICE_GATE_OK="$STORE/service-gate-ok"
mkdir -p "$SERVICE_GATE_OK"
cat > "$SERVICE_GATE_OK/run.log" <<'EOF'
BVAGENT READY host=BRIDGEVM v3-share2 t=1234
BVAGENT CMD whoami exit=0
bridgevm\user
BVAGENT END whoami
BVAGENT SERVICE start t=1300
stop: PSCI 0x84000008 (system off)
NVMe disk written back: /tmp/windows.raw
EOF
service_gate_ok_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    EVIDENCE_DIR="$1"
    AGENT_SERVICE_CONTROL="$1/app.ctl"
    AGENT_SERVICE_COMMAND=whoami
    RUN_STATUS=0
    write_agent_service_gate
    cat "$EVIDENCE_DIR/agent-service-gate.txt"
    printf "run_status=%s\n" "$RUN_STATUS"
  ' _ "$SERVICE_GATE_OK"
)" || fail "successful agent service gate failed: $service_gate_ok_output"
assert_contains "$service_gate_ok_output" "agent_handshake=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "initial_command_exit_zero=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "initial_command_complete=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "service_started=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "guest_system_off=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "nvme_writeback=true" "successful agent service gate"
assert_contains "$service_gate_ok_output" "status=0" "successful agent service gate"
assert_contains "$service_gate_ok_output" "run_status=0" "successful agent service gate"

SERVICE_GATE_FAIL="$STORE/service-gate-fail"
mkdir -p "$SERVICE_GATE_FAIL"
printf 'BVAGENT READY host=BRIDGEVM v3-share2 t=1234\nBVAGENT SERVICE start t=1300\n' > "$SERVICE_GATE_FAIL/run.log"
service_gate_fail_output="$(
  bash -c '
    set -euo pipefail
    source scripts/run-hvf-windows-installed-boot-runner.sh
    EVIDENCE_DIR="$1"
    AGENT_SERVICE_CONTROL="$1/app.ctl"
    AGENT_SERVICE_COMMAND=whoami
    RUN_STATUS=0
    write_agent_service_gate
    cat "$EVIDENCE_DIR/agent-service-gate.txt"
    printf "run_status=%s\n" "$RUN_STATUS"
  ' _ "$SERVICE_GATE_FAIL"
)" || fail "failed agent service gate evaluation errored: $service_gate_fail_output"
assert_contains "$service_gate_fail_output" "initial_command_exit_zero=false" "failed agent service gate"
assert_contains "$service_gate_fail_output" "guest_system_off=false" "failed agent service gate"
assert_contains "$service_gate_fail_output" "nvme_writeback=false" "failed agent service gate"
assert_contains "$service_gate_fail_output" "status=1" "failed agent service gate"
assert_contains "$service_gate_fail_output" "run_status=1" "failed agent service gate"

RELATIVE_WRAPPER_ROOT="$STORE/relative-wrapper"
mkdir -p \
  "$RELATIVE_WRAPPER_ROOT/media" \
  "$RELATIVE_WRAPPER_ROOT/state" \
  "$RELATIVE_WRAPPER_ROOT/evidence" \
  "$RELATIVE_WRAPPER_ROOT/traces" \
  "$RELATIVE_WRAPPER_ROOT/driver" \
  "$RELATIVE_WRAPPER_ROOT/share"
RELATIVE_WRAPPER_REAL="$(cd "$RELATIVE_WRAPPER_ROOT" && pwd -P)"
touch \
  "$RELATIVE_WRAPPER_ROOT/media/windows.raw" \
  "$RELATIVE_WRAPPER_ROOT/media/placeholder.raw" \
  "$RELATIVE_WRAPPER_ROOT/state/vars.fd"

relative_wrapper_output="$(
  cd "$RELATIVE_WRAPPER_ROOT"
  bash -c '
    set -euo pipefail
    root="$1"
    bin="$2"
    source "$root/scripts/run-hvf-windows-installed-boot-validation.sh"
    source "$root/scripts/run-hvf-windows-installed-boot-args.sh"
    source "$root/scripts/run-hvf-windows-installed-boot-runner.sh"
    init_installed_boot_defaults
    parse_installed_boot_args \
      --target media/windows.raw \
      --placeholder-nsid1 media/placeholder.raw \
      --vars state/vars.fd \
      --evidence-dir evidence \
      --virtio-gpu-3d \
      --gpu-trace traces/gpu.jsonl \
      --viogpu3d-dir driver \
      --agent-service-control state/app.ctl \
      --agent-clipboard-sync \
      --agent-share-host share \
      --agent-share-guest "C:\\bridgevm-share"
    absolutize_installed_boot_paths "$(pwd -P)"
    validate_installed_boot_option_combinations
    validate_installed_boot_required_paths
    printf "target_path=%s\n" "$TARGET"
    printf "placeholder_path=%s\n" "$PLACEHOLDER_NSID1"
    printf "vars_path=%s\n" "$VARS"
    printf "evidence_path=%s\n" "$EVIDENCE_DIR"
    printf "trace_path=%s\n" "$VIRTIO_GPU_TRACE_JSONL"
    printf "driver_path=%s\n" "$VIOGPU3D_DIR"
    build_installed_boot_env_args >/dev/null
    cd "$root"
    BIN="$bin"
    PROBE_PID=""
    run_probe_process
    cat "$EVIDENCE_DIR/run.log"
  ' _ "$ROOT" "$FAKE_PROBE"
)" || fail "relative installed-boot path freeze failed: $relative_wrapper_output"

assert_contains "$relative_wrapper_output" "target_path=$RELATIVE_WRAPPER_REAL/media/windows.raw" "relative target path"
assert_contains "$relative_wrapper_output" "placeholder_path=$RELATIVE_WRAPPER_REAL/media/placeholder.raw" "relative placeholder path"
assert_contains "$relative_wrapper_output" "vars_path=$RELATIVE_WRAPPER_REAL/state/vars.fd" "relative vars path"
assert_contains "$relative_wrapper_output" "evidence_path=$RELATIVE_WRAPPER_REAL/evidence" "relative evidence path"
assert_contains "$relative_wrapper_output" "trace_path=$RELATIVE_WRAPPER_REAL/traces/gpu.jsonl" "relative trace path"
assert_contains "$relative_wrapper_output" "driver_path=$RELATIVE_WRAPPER_REAL/driver" "relative driver path"
assert_contains "$relative_wrapper_output" "console_ctl=$RELATIVE_WRAPPER_REAL/state/app.ctl" "relative agent control path"
assert_contains "$relative_wrapper_output" "console_clipsync=1" "relative agent clipboard env"
assert_contains "$relative_wrapper_output" "console_share=$RELATIVE_WRAPPER_REAL/share::C:\bridgevm-share" "relative agent share env"
assert_contains "$relative_wrapper_output" "nvme=$RELATIVE_WRAPPER_REAL/media/placeholder.raw" "relative primary NVMe env"
assert_contains "$relative_wrapper_output" "nvme2=$RELATIVE_WRAPPER_REAL/media/windows.raw" "relative second NVMe env"
assert_contains "$relative_wrapper_output" "nvme2_writable=1" "relative second NVMe writable env"
assert_contains "$relative_wrapper_output" "vars=$RELATIVE_WRAPPER_REAL/state/vars.fd" "relative vars env"
assert_contains "$relative_wrapper_output" "gpu_trace=$RELATIVE_WRAPPER_REAL/traces/gpu.jsonl" "relative GPU trace env"

explicit_id_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" || fail "installed boot explicit GPU PCI ID policy failed: $explicit_id_output"

assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=<unset> (explicit device id 0x1050)" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=0x1050" "installed boot explicit PCI ID policy"

id_without_gpu_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted GPU PCI ID without --virtio-gpu-3d: $id_without_gpu_output"

assert_contains "$id_without_gpu_output" "--virtio-gpu-device-id requires --virtio-gpu-3d" "GPU PCI ID without GPU policy"

virgl_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace-protocol virgl \
    --print-policy 2>&1
)" || fail "installed boot unexpectedly rejected virgl protocol: $virgl_output"

assert_contains "$virgl_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl" "virgl policy"
assert_contains "$virgl_output" "BRIDGEVM_GPU_TRACE_PROTOCOL=virgl" "virgl policy"

echo "PASS: installed Windows P3 GPU policy smoke ($STORE)"
