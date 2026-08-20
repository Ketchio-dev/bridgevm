#!/usr/bin/env bash
# Deterministic project check.
#
# The gate referenced by AGENTS.md and by CI; it must pass before work is called
# done. No live virtualization: no Hypervisor.framework, Windows media or GPU.
#
#   scripts/check-project.sh [--fast]   # --fast is the truth/format subset
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

TOOLCHAIN="${BRIDGEVM_CHECK_TOOLCHAIN:-+1.97.0}"
FAST=0
[[ "${1:-}" == "--fast" ]] && FAST=1

failures=0
declare -a failed_steps=()

step() {
  local name="$1"; shift
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
  for path in "$@"; do python3 -m json.tool "$path" >/dev/null || return 1; done
}

# --- truth and documentation -------------------------------------------------
step "capability registry" python3 scripts/render-capability-status.py --check
step "contract and schema json" json_valid docs/machine-contract/qemu-virt-deviations.json schemas/bridgevm-capability-v1.json
step "capability evidence" python3 scripts/check-capability-evidence.py
step "capability test counts" python3 scripts/check-capability-test-counts.py
step "documentation system" bash scripts/check-documentation-system.sh
step "documentation references" python3 scripts/check-doc-references.py
step "structural budgets" scripts/check-refactor-budgets.sh
step "shell scripts" bash scripts/check-shell-scripts.sh
step "python scripts" python3 scripts/check-python-scripts.py
step "workflow yaml" python3 scripts/check-workflow-yaml.py
step "daemon DTO decoders" python3 scripts/check-daemon-dto-decoders.py
step "swift force casts" python3 scripts/check-swift-force-casts.py
step "tests are reachable" python3 scripts/check-tests-are-reachable.py
step "virgl integer attributes" scripts/check-virgl-integer-attributes.sh
step "hvf coherence protocol" scripts/check-hvf-windows-coherence-protocol.sh
step "attribution honesty" scripts/check-attribution-honesty.sh
step "install verify" bash tests/integration/install-verify-smoke.sh

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

  # The non-Apple platform stubs only compile on a non-Apple target, so a
  # macOS-only gate cannot see them drift. Skipped here, required in CI.
  if rustup target list --installed --toolchain "${TOOLCHAIN#+}" 2>/dev/null \
      | grep -q '^aarch64-unknown-linux-gnu$'; then
    step "cross-compile (linux stubs)" cargo "$TOOLCHAIN" check --workspace \
      --target aarch64-unknown-linux-gnu --locked
  else
    printf 'cross-compile (linux stubs): SKIP (target not installed)\n'
  fi

  # --- macOS app -------------------------------------------------------------
  if command -v swift >/dev/null 2>&1; then
    step "swift build" swift build --package-path apps/macos
    step "swift tests" scripts/run-swift-tests.sh
    # run-swift-tests.sh only covers apps/macos/Tests/*SwiftTests; the shim
    # suites are a separate, much larger body that once went ungated.
    step "xctest shim suites" scripts/run-xctest-shim-suites.sh
    step "release overrides" scripts/check-release-overrides.sh
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
