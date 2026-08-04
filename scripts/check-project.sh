#!/usr/bin/env bash
# Deterministic project check.
#
# This is the gate referenced by AGENTS.md and by CI. It must pass before any
# work is called done. It deliberately contains no live virtualization: every
# step here runs without Hypervisor.framework, private Windows media or a GPU.
#
#   scripts/check-project.sh          # full check
#   scripts/check-project.sh --fast   # truth/format subset used by the harness
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TOOLCHAIN="${BRIDGEVM_CHECK_TOOLCHAIN:-+1.97.0}"
FAST=0
[[ "${1:-}" == "--fast" ]] && FAST=1

failures=0
declare -a failed_steps=()

step() {
  local name="$1"
  shift
  printf '\n=== %s ===\n' "$name"
  if "$@"; then
    printf '%s: PASS\n' "$name"
  else
    printf '%s: FAIL\n' "$name" >&2
    failures=$((failures + 1))
    failed_steps+=("$name")
  fi
}

json_valid() {
  local path
  for path in "$@"; do
    python3 -m json.tool "$path" >/dev/null || return 1
  done
}

# --- truth and documentation -------------------------------------------------
step "capability registry" python3 scripts/render-capability-status.py --check
step "machine contract json" json_valid docs/machine-contract/qemu-virt-deviations.json
step "capability schema json" json_valid schemas/bridgevm-capability-v1.json
step "documentation system" bash scripts/check-documentation-system.sh
step "structural budgets" scripts/check-refactor-budgets.sh

# --- formatting --------------------------------------------------------------
step "rustfmt" cargo "$TOOLCHAIN" fmt --all --check

if [[ $FAST -eq 1 ]]; then
  printf '\n--- fast subset complete ---\n'
else
  # --- correctness -----------------------------------------------------------
  step "clippy (workspace)" cargo "$TOOLCHAIN" clippy --workspace --all-targets --locked -- -D warnings
  step "clippy (venus)" cargo "$TOOLCHAIN" clippy -p bridgevm-hvf --all-targets --features venus --locked -- -D warnings
  step "tests (workspace)" cargo "$TOOLCHAIN" test --workspace --locked
  step "tests (venus lib)" cargo "$TOOLCHAIN" test -p bridgevm-hvf --lib --features venus --locked
  step "tests (probe example)" cargo "$TOOLCHAIN" test -p bridgevm-hvf --features venus --example hvf_gic_boot_probe --locked

  # --- macOS app -------------------------------------------------------------
  if command -v swift >/dev/null 2>&1; then
    step "swift build" swift build --package-path apps/macos
    step "swift tests" scripts/run-swift-tests.sh
  else
    printf '\nswift toolchain absent: skipping macOS app checks\n' >&2
  fi
fi

printf '\n========================================\n'
if [[ $failures -ne 0 ]]; then
  printf 'project check: FAIL (%d step(s))\n' "$failures" >&2
  printf '  - %s\n' "${failed_steps[@]}" >&2
  exit 1
fi
printf 'project check: PASS\n'
