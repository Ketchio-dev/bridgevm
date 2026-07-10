#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INVOCATION_DIR="$(pwd -P)"

source "$ROOT/scripts/run-hvf-windows-installed-boot-usage.sh"
source "$ROOT/scripts/run-hvf-windows-installed-boot-validation.sh"
source "$ROOT/scripts/run-hvf-windows-installed-boot-args.sh"
source "$ROOT/scripts/run-hvf-windows-installed-boot-runner.sh"

init_installed_boot_defaults
parse_installed_boot_args "$@"
absolutize_installed_boot_paths "$INVOCATION_DIR"
validate_installed_boot_option_combinations
configure_installed_boot_xhci_policy
validate_installed_boot_required_paths

if [[ "$PRINT_POLICY" == "1" ]]; then
  print_installed_boot_policy
  exit 0
fi

run_installed_boot_probe

exit "$RUN_STATUS"
