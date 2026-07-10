#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat >&2 <<'EOF'
usage: scripts/run-hvf-boot-timer-matrix.sh --target RAW --vars FD --evidence-dir DIR [options] [-- wrapper args...]

Run the installed Windows HVF boot wrapper repeatedly with BOOT_TIMER enabled,
then summarize the resulting run.log files with report-hvf-boot-timer-metrics.sh.

Required:
  --target RAW            Installed Windows raw disk to boot.
  --vars FD               Writable UEFI vars file preserved from install.
  --evidence-dir DIR      Root directory for per-run evidence and matrix report.

Options:
  --placeholder-nsid1 RAW Optional blank NSID-1 disk, cloned per run by default.
  --runs N                Runs per SMP config. Default: 3.
  --smp-cpus LIST         Comma-separated vCPU counts. Default: 1,2,4.
  --boot-timer-ramfb-ms N BOOT_TIMER display checksum sample interval.
  --boot-timer-desktop-checksum64 N
                          Legacy exact-frame desktop oracle, decimal or hex.
                          Overrides the default agent READY/PONG oracle.
  --wrapper PATH          Installed-boot wrapper. Default:
                          scripts/run-hvf-windows-installed-boot.sh.
  --report PATH           Report output. Default:
                          <evidence-dir>/boot-timer-report.tsv.
  --release               Pass --release to the installed-boot wrapper.
  --skip-build            Pass --skip-build to the installed-boot wrapper.
  --copy-media            If APFS clonefile fails, allow a full media copy.
                          This can be slow and space-heavy for Windows raw disks.
  --no-clone-media        Reuse --target/--vars/--placeholder-nsid1 directly.
                          Faster, but run state is shared across matrix runs.
  -h, --help              Show this help.

Arguments after -- are passed through to the installed-boot wrapper, for example
--virtio-net, --enable-xhci, --daily, or --ram-mib.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

smp_cpu_count() {
  positive_integer "$1" || return 1
  (( ${#1} <= 3 )) || return 1
  (( 10#$1 <= 123 ))
}

smp_cpu_list() {
  [[ "$1" =~ ^[1-9][0-9]*(,[1-9][0-9]*)*$ ]] || return 1
  local old_ifs="$IFS"
  local cpu
  IFS=,
  for cpu in $1; do
    smp_cpu_count "$cpu" || {
      IFS="$old_ifs"
      return 1
    }
  done
  IFS="$old_ifs"
}

boot_timer_ramfb_ms() {
  positive_integer "$1" || return 1
  (( ${#1} <= 5 )) || return 1
  (( 10#$1 >= 100 && 10#$1 <= 60000 ))
}

reject_matrix_owned_passthrough_args() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      --target|--target=*|--disk|--disk=*|--writable-disk|--writable-disk=*|\
      --placeholder-nsid1|--placeholder-nsid1=*|--vars|--vars=*|\
      --evidence-dir|--evidence-dir=*|--smp-cpus|--smp-cpus=*|\
      --boot-timer|--boot-timer-ramfb-ms|--boot-timer-ramfb-ms=*|\
      --boot-timer-desktop-agent|\
      --boot-timer-desktop-checksum64|--boot-timer-desktop-checksum64=*|\
      --release|--skip-build)
        fail "argument after -- overrides matrix-owned option: $arg"
        ;;
    esac
  done
}

u64_literal() {
  local value="$1"
  local trimmed
  case "$value" in
    0x*|0X*)
      [[ "${value#??}" =~ ^[0-9a-fA-F]{1,16}$ ]]
      ;;
    *)
      [[ "$value" =~ ^[0-9]+$ ]] || return 1
      trimmed="${value#"${value%%[!0]*}"}"
      [[ -n "$trimmed" ]] || trimmed="0"
      (( ${#trimmed} < 20 )) && return 0
      [[ ${#trimmed} -eq 20 && "$trimmed" < "18446744073709551616" ]]
      ;;
  esac
}

shell_quote_command() {
  printf '%q' "$1"
  shift
  local arg
  for arg in "$@"; do
    printf ' %q' "$arg"
  done
  printf '\n'
}

prepare_media_file() {
  local src="$1"
  local dst="$2"
  mkdir -p "$(dirname "$dst")"
  rm -f "$dst"
  if cp -c "$src" "$dst" 2>/dev/null; then
    :
  elif [[ "$COPY_MEDIA" == "1" ]]; then
    cp "$src" "$dst"
  else
    fail "failed to clone media with 'cp -c': $src -> $dst; use --copy-media for a full copy or --no-clone-media to reuse media"
  fi
  chmod u+rw "$dst" 2>/dev/null || true
}

ensure_run_report_artifacts() {
  local run_dir="$1"
  local smp="$2"
  local run="$3"
  local status="$4"
  local profile
  if [[ "$RELEASE" == "1" ]]; then
    profile="release"
  else
    profile="debug"
  fi

  if [[ ! -f "$run_dir/preflight.txt" ]]; then
    {
      date -u
      printf 'matrix_missing_preflight=1\n'
      printf 'matrix_status=%s\n' "$status"
      printf 'matrix_smp_cpus=%s\n' "$smp"
      printf 'matrix_run=%s\n' "$run"
      printf 'build_profile=%s\n' "$profile"
      printf 'daily_preset=unknown\n'
      printf 'smp_cpus=%s\n' "$smp"
      printf 'ram_mib=unknown\n'
      printf 'watchdog_ms=unknown\n'
      printf 'xhci_report_interval_ms=unknown\n'
      printf 'boot_timer=1\n'
      printf 'boot_timer_ramfb_ms=%s\n' "${BOOT_TIMER_RAMFB_MS:-<probe-default 1000>}"
      printf 'boot_timer_desktop_checksum64=%s\n' "${BOOT_TIMER_DESKTOP_CHECKSUM64:-<unset>}"
      printf 'boot_timer_desktop_agent=%s\n' "$BOOT_TIMER_DESKTOP_AGENT"
      printf 'virtio_gpu_3d=unknown\n'
    } > "$run_dir/preflight.txt"
  fi

  if [[ ! -f "$run_dir/run.log" ]]; then
    {
      date -u
      printf 'MATRIX wrapper did not produce run.log status=%s smp=%s run=%s\n' "$status" "$smp" "$run"
    } > "$run_dir/run.log"
  fi
}

TARGET=""
PLACEHOLDER_NSID1=""
VARS=""
EVIDENCE_DIR=""
RUNS="3"
SMP_CPUS_LIST="1,2,4"
BOOT_TIMER_RAMFB_MS=""
BOOT_TIMER_DESKTOP_CHECKSUM64=""
BOOT_TIMER_DESKTOP_AGENT="1"
WRAPPER="$ROOT/scripts/run-hvf-windows-installed-boot.sh"
REPORT=""
RELEASE="0"
SKIP_BUILD="0"
CLONE_MEDIA="1"
COPY_MEDIA="0"
PASSTHROUGH_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) [[ $# -ge 2 ]] || { usage; exit 2; }; TARGET="$2"; shift 2 ;;
    --placeholder-nsid1) [[ $# -ge 2 ]] || { usage; exit 2; }; PLACEHOLDER_NSID1="$2"; shift 2 ;;
    --vars) [[ $# -ge 2 ]] || { usage; exit 2; }; VARS="$2"; shift 2 ;;
    --evidence-dir) [[ $# -ge 2 ]] || { usage; exit 2; }; EVIDENCE_DIR="$2"; shift 2 ;;
    --runs)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --runs requires a positive integer" >&2; exit 2; }
      RUNS="$2"; shift 2
      ;;
    --smp-cpus)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      smp_cpu_list "$2" || { echo "FAIL: --smp-cpus requires comma-separated integers from 1 to 123" >&2; exit 2; }
      SMP_CPUS_LIST="$2"; shift 2
      ;;
    --boot-timer-ramfb-ms)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      boot_timer_ramfb_ms "$2" || { echo "FAIL: --boot-timer-ramfb-ms requires an integer from 100 to 60000" >&2; exit 2; }
      BOOT_TIMER_RAMFB_MS="$2"; shift 2
      ;;
    --boot-timer-desktop-checksum64)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      u64_literal "$2" || { echo "FAIL: --boot-timer-desktop-checksum64 requires a u64 decimal or 0x-prefixed hex value" >&2; exit 2; }
      BOOT_TIMER_DESKTOP_CHECKSUM64="$2"; BOOT_TIMER_DESKTOP_AGENT="0"; shift 2
      ;;
    --wrapper) [[ $# -ge 2 ]] || { usage; exit 2; }; WRAPPER="$2"; shift 2 ;;
    --report) [[ $# -ge 2 ]] || { usage; exit 2; }; REPORT="$2"; shift 2 ;;
    --release) RELEASE="1"; shift ;;
    --skip-build) SKIP_BUILD="1"; shift ;;
    --copy-media) COPY_MEDIA="1"; shift ;;
    --no-clone-media) CLONE_MEDIA="0"; shift ;;
    -h|--help) usage; exit 0 ;;
    --)
      shift
      PASSTHROUGH_ARGS=("$@")
      break
      ;;
    *) usage; exit 2 ;;
  esac
done

[[ -n "$TARGET" && -n "$VARS" && -n "$EVIDENCE_DIR" ]] || { usage; exit 2; }
if (( ${#PASSTHROUGH_ARGS[@]} > 0 )); then
  reject_matrix_owned_passthrough_args "${PASSTHROUGH_ARGS[@]}"
fi
[[ -f "$TARGET" ]] || fail "target image not found: $TARGET"
[[ -f "$VARS" ]] || fail "vars file not found: $VARS"
if [[ -n "$PLACEHOLDER_NSID1" ]]; then
  [[ -f "$PLACEHOLDER_NSID1" ]] || fail "placeholder NSID-1 image not found: $PLACEHOLDER_NSID1"
fi
[[ -x "$WRAPPER" ]] || fail "wrapper is not executable: $WRAPPER"

REPORT="${REPORT:-$EVIDENCE_DIR/boot-timer-report.tsv}"
mkdir -p "$EVIDENCE_DIR"
mkdir -p "$(dirname "$REPORT")"
[[ ! -e "$REPORT" ]] || fail "report already exists; use a fresh evidence directory or --report path: $REPORT"

MATRIX_STATUS="0"
RUN_DIRS=()
old_ifs="$IFS"
IFS=,
read -r -a SMP_VALUES <<< "$SMP_CPUS_LIST"
IFS="$old_ifs"

for smp in "${SMP_VALUES[@]}"; do
  for (( run = 1; run <= RUNS; run++ )); do
    run_dir="$EVIDENCE_DIR/smp-$smp/run-$run"
    media_dir="$run_dir/media"
    [[ ! -e "$run_dir" ]] || fail "run evidence already exists; refusing to mix stale and fresh samples: $run_dir"
    mkdir -p "$run_dir"

    run_target="$TARGET"
    run_vars="$VARS"
    run_placeholder="$PLACEHOLDER_NSID1"
    if [[ "$CLONE_MEDIA" == "1" ]]; then
      run_target="$media_dir/target.raw"
      run_vars="$media_dir/vars.fd"
      prepare_media_file "$TARGET" "$run_target"
      prepare_media_file "$VARS" "$run_vars"
      if [[ -n "$PLACEHOLDER_NSID1" ]]; then
        run_placeholder="$media_dir/placeholder-nsid1.raw"
        prepare_media_file "$PLACEHOLDER_NSID1" "$run_placeholder"
      fi
    fi

    args=(
      --target "$run_target"
      --vars "$run_vars"
      --evidence-dir "$run_dir"
      --smp-cpus "$smp"
      --boot-timer
    )
    if [[ -n "$run_placeholder" ]]; then
      args+=(--placeholder-nsid1 "$run_placeholder")
    fi
    if [[ -n "$BOOT_TIMER_RAMFB_MS" ]]; then
      args+=(--boot-timer-ramfb-ms "$BOOT_TIMER_RAMFB_MS")
    fi
    if [[ -n "$BOOT_TIMER_DESKTOP_CHECKSUM64" ]]; then
      args+=(--boot-timer-desktop-checksum64 "$BOOT_TIMER_DESKTOP_CHECKSUM64")
    elif [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" ]]; then
      args+=(--boot-timer-desktop-agent)
    fi
    [[ "$RELEASE" == "1" ]] && args+=(--release)
    [[ "$SKIP_BUILD" == "1" ]] && args+=(--skip-build)
    if (( ${#PASSTHROUGH_ARGS[@]} > 0 )); then
      args+=("${PASSTHROUGH_ARGS[@]}")
    fi

    {
      date -u
      printf 'matrix_smp_cpus=%s\n' "$smp"
      printf 'matrix_run=%s\n' "$run"
      printf 'clone_media=%s\n' "$CLONE_MEDIA"
      printf 'wrapper='
      shell_quote_command "$WRAPPER" "${args[@]}"
    } > "$run_dir/matrix-invocation.txt"

    set +e
    "$WRAPPER" "${args[@]}" > "$run_dir/matrix-wrapper.stdout" 2> "$run_dir/matrix-wrapper.stderr"
    status="$?"
    set -e
    printf 'status=%s\n' "$status" > "$run_dir/matrix-status.txt"
    ensure_run_report_artifacts "$run_dir" "$smp" "$run" "$status"
    RUN_DIRS+=("$run_dir")
    if [[ "$status" != "0" ]]; then
      MATRIX_STATUS="$status"
    fi
  done
done

set +e
"$ROOT/scripts/report-hvf-boot-timer-metrics.sh" "${RUN_DIRS[@]}" > "$REPORT"
REPORT_STATUS="$?"
set -e
printf 'Wrote boot timer matrix report: %s\n' "$REPORT"
if [[ "$MATRIX_STATUS" == "0" && "$REPORT_STATUS" != "0" ]]; then
  MATRIX_STATUS="$REPORT_STATUS"
fi
exit "$MATRIX_STATUS"
