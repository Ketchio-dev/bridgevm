#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-runner-launch.XXXXXX")"
STORE="$(cd "$STORE" && pwd -P)"
TARGET="$STORE/windows-target.raw"
PLACEHOLDER="$STORE/placeholder-nsid1.raw"
VARS="$STORE/vars.fd"
EVIDENCE="$STORE/evidence"
TRACE="$STORE/evidence/virtio-gpu.jsonl"
VIOGPU3D="$STORE/viogpu3d"
FAKE_REPO="$STORE/fake-repo"

touch "$TARGET" "$PLACEHOLDER" "$VARS"
mkdir -p "$EVIDENCE" "$VIOGPU3D" "$FAKE_REPO/scripts"
FAKE_REPO_REAL="$(cd "$FAKE_REPO" && pwd -P)"

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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly contained '$needle'; got: $haystack" ;;
    *) ;;
  esac
}

assert_line() {
  local haystack="$1"
  local line="$2"
  local label="$3"
  grep -Fqx -- "$line" <<<"$haystack" || fail "$label missing exact line '$line'; got: $haystack"
}

assert_args_exact() {
  local output="$1"
  local label="$2"
  shift 2
  local index=0
  local expected
  assert_line "$output" "arg_count=$#" "$label"
  for expected in "$@"; do
    assert_line "$output" "arg[$index]=$expected" "$label"
    index=$((index + 1))
  done
}

fake_wrapper="$FAKE_REPO/scripts/run-hvf-windows-installed-boot.sh"
cat >"$fake_wrapper" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail

printf 'cwd=%s\n' "$PWD"
printf 'arg_count=%s\n' "$#"
index=0
for arg in "$@"; do
  printf 'arg[%s]=%s\n' "$index" "$arg"
  index=$((index + 1))
done
WRAPPER
chmod +x "$fake_wrapper"

run_hvf_runner() {
  cargo run -q --manifest-path "$ROOT/Cargo.toml" -p hvf-runner -- "$@"
}

delegate_output="$(
  run_hvf_runner \
    --launch \
    --repo-root "$FAKE_REPO" \
    --target "$TARGET" \
    --placeholder-nsid1 "$PLACEHOLDER" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --watchdog-ms 12345 \
    --max-reboots 2 \
    --ram-mib 2048 \
    --smp-cpus 1 \
    --boot-timer \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --enable-xhci \
    --virtio-net \
    --virtio-gpu-3d \
    --virtio-gpu-device-id 1050 \
    --gpu-trace "$TRACE" \
    --gpu-trace-protocol venus \
    --require-gpu-trace-gate \
    --viogpu3d-dir "$VIOGPU3D" \
    --require-viogpu3d-readiness \
    --daily \
    --release \
    --skip-build \
    --print-policy 2>&1
)" || fail "hvf-runner launch fake-wrapper delegation failed: $delegate_output"

assert_contains "$delegate_output" "cwd=$FAKE_REPO_REAL" "fake-wrapper delegation"
assert_args_exact "$delegate_output" "fake-wrapper delegation" \
  --target "$TARGET" \
  --vars "$VARS" \
  --evidence-dir "$EVIDENCE" \
  --placeholder-nsid1 "$PLACEHOLDER" \
  --watchdog-ms 12345 \
  --max-reboots 2 \
  --ram-mib 2048 \
  --smp-cpus 1 \
  --boot-timer \
  --boot-timer-ramfb-ms 250 \
  --boot-timer-desktop-checksum64 0x1234abcd \
  --enable-xhci \
  --virtio-net \
  --virtio-gpu-3d \
  --virtio-gpu-device-id 1050 \
  --gpu-trace "$TRACE" \
  --gpu-trace-protocol venus \
  --require-gpu-trace-gate \
  --viogpu3d-dir "$VIOGPU3D" \
  --require-viogpu3d-readiness \
  --daily \
  --release \
  --skip-build \
  --print-policy

alias_output="$(
  BRIDGEVM_REPO_ROOT="$FAKE_REPO" run_hvf_runner \
    --launch \
    --disk "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --print-policy 2>&1
)" || fail "hvf-runner launch disk alias delegation failed: $alias_output"

assert_contains "$alias_output" "cwd=$FAKE_REPO_REAL" "disk alias delegation"
assert_args_exact "$alias_output" "disk alias delegation" \
  --target "$TARGET" \
  --vars "$VARS" \
  --evidence-dir "$EVIDENCE" \
  --print-policy

INVOKE="$STORE/invoke"
REL_EVIDENCE="$STORE/relative-evidence"
REL_TRACE="$REL_EVIDENCE/virtio-gpu.jsonl"
mkdir -p "$INVOKE" "$REL_EVIDENCE"
touch "$REL_TRACE"
relative_output="$(
  cd "$INVOKE"
  run_hvf_runner \
    --launch \
    --repo-root ../fake-repo \
    --target ../windows-target.raw \
    --vars ../vars.fd \
    --evidence-dir ../relative-evidence \
    --gpu-trace ../relative-evidence/virtio-gpu.jsonl \
    --print-policy 2>&1
)" || fail "hvf-runner relative launch failed: $relative_output"
assert_line "$relative_output" "cwd=$FAKE_REPO_REAL" "relative launch cwd"
assert_args_exact "$relative_output" "relative launch arguments" \
  --target "$TARGET" \
  --vars "$VARS" \
  --evidence-dir "$REL_EVIDENCE" \
  --gpu-trace "$REL_TRACE" \
  --print-policy

real_policy_output="$(
  run_hvf_runner \
    --launch \
    --repo-root "$ROOT" \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --daily \
    --boot-timer-ramfb-ms 250 \
    --boot-timer-desktop-checksum64 0x1234abcd \
    --enable-xhci \
    --virtio-gpu-3d \
    --gpu-trace "$TRACE" \
    --gpu-trace-protocol virgl \
    --require-gpu-trace-gate \
    --viogpu3d-dir "$VIOGPU3D" \
    --require-viogpu3d-readiness \
    --print-policy 2>&1
)" || fail "hvf-runner launch real-wrapper policy failed: $real_policy_output"

assert_contains "$real_policy_output" "BRIDGEVM_DISABLE_XHCI=<unset> (--enable-xhci)" "real-wrapper policy"
assert_contains "$real_policy_output" "DAILY_PRESET=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_RAM_MIB=6144" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=86400000" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_SMP_CPUS=4" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_BOOT_TIMER=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_BOOT_TIMER_RAMFB_MS=250" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=0x1234abcd" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_VIRTIO_GPU=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_VIRTIO_GPU_3D=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=$TRACE" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_GPU_TRACE_PROTOCOL=virgl" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_REQUIRE_GPU_TRACE_GATE=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_VIOGPU3D_DIR=$VIOGPU3D" "real-wrapper policy"
assert_contains "$real_policy_output" "BRIDGEVM_REQUIRE_VIOGPU3D_READINESS=1" "real-wrapper policy"
assert_contains "$real_policy_output" "BUILD_PROFILE=release" "real-wrapper policy"

bad_smp_output="$(
  run_hvf_runner \
    --launch \
    --repo-root "$ROOT" \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --smp-cpus 0 \
    --print-policy 2>&1
)" && fail "hvf-runner launch unexpectedly accepted invalid SMP count: $bad_smp_output"

assert_contains "$bad_smp_output" "--smp-cpus requires an integer from 1 to 123" "invalid SMP delegation"

missing_vars_output="$(
  run_hvf_runner \
    --launch \
    --repo-root "$FAKE_REPO" \
    --target "$TARGET" \
    --evidence-dir "$EVIDENCE" \
    --print-policy 2>&1
)" && fail "hvf-runner launch unexpectedly accepted missing --vars: $missing_vars_output"

assert_contains "$missing_vars_output" "--launch requires --vars" "missing vars validation"

ambiguous_output="$(
  run_hvf_runner \
    --launch \
    --repo-root "$FAKE_REPO" \
    --target "$TARGET" \
    --disk "$PLACEHOLDER" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" 2>&1
)" && fail "hvf-runner launch unexpectedly accepted ambiguous targets: $ambiguous_output"
assert_contains "$ambiguous_output" "accepts only one of --target, --disk, or --writable-disk" "ambiguous target validation"

echo "PASS: hvf-runner installed boot launch policy smoke ($STORE)"
