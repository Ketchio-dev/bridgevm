# Shared resource closure for both packaged HVF wrappers.
install_installed_boot_modules() {
  local resources="$1" script
  install -d "$resources/scripts/live-gates"
  for script in \
    run-hvf-windows-installed-boot.sh \
    run-hvf-windows-installed-boot-usage.sh run-hvf-windows-installed-boot-usage-core.sh \
    run-hvf-windows-installed-boot-validation.sh \
    run-hvf-windows-installed-boot-args.sh run-hvf-windows-installed-boot-policy.sh \
    run-hvf-windows-installed-boot-runner.sh run-hvf-windows-installed-boot-output.sh \
    run-hvf-windows-installed-boot-package-policy.sh hvf-terminal-report.sh; do
    install -m 755 "$ROOT/scripts/$script" "$resources/scripts/$script"
  done
  install -m 644 "$ROOT/scripts/live-gates/bounded_output.py" "$resources/scripts/live-gates/bounded_output.py"
}
