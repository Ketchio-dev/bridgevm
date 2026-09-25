#!/usr/bin/env bash
# Exercise packaged wrapper executable selection without starting a guest.
# shellcheck disable=SC2016 # Bash -c snippets expand positional parameters in the child shell.
set -euo pipefail
unset CARGO_TARGET_DIR BRIDGEVM_PREBUILT_PROBE

ROOT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
store="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-wrapper-policy.XXXXXX")" && pwd -P)"
trap 'rm -rf "$store"' EXIT
fail() { echo "packaged wrapper policy: FAIL ($*)" >&2; exit 1; }
refuse() {
  local label="$1" expected="$2" output
  shift 2
  if output="$("$@" 2>&1)"; then fail "$label unexpectedly passed"; fi
  [[ "$output" == *"$expected"* ]] || fail "$label missed diagnostic: $output"
}

entry="$ROOT_REPO/scripts/run-hvf-windows-installed-boot.sh"
[[ "$(head -n 1 "$entry")" == '#!/bin/bash' ]] || fail 'wrapper interpreter is PATH resolved'
/bin/bash -n "$entry" "$ROOT_REPO/scripts/run-hvf-windows-installed-boot-runner.sh" \
  "$ROOT_REPO/scripts/run-hvf-windows-installed-boot-package-policy.sh"
rg -Fq 'run-hvf-windows-installed-boot-package-policy.sh' \
  "$ROOT_REPO/apps/macos/scripts/package-hvf-control-app.sh" || fail 'package omits wrapper policy module'

app="$store/BridgeVM.app"
contents="$app/Contents"
resources="$contents/Resources"
probe="$resources/target/release/examples/hvf_gic_boot_probe"
cli="$resources/target/release/bridgevm"
swtpm="$contents/Helpers/swtpm"
policy="$resources/scripts/run-hvf-windows-installed-boot-package-policy.sh"
mkdir -p "$contents/MacOS" "$(dirname "$probe")" "$(dirname "$swtpm")" "$(dirname "$policy")"
cp /usr/bin/true "$contents/MacOS/BridgeVMControl"
cp /usr/bin/true "$probe"
cp /usr/bin/true "$cli"
cp /usr/bin/true "$swtpm"
for script in run-hvf-windows-installed-boot{,-usage,-validation,-args,-runner,-package-policy}.sh; do
  cp "$ROOT_REPO/scripts/$script" "$resources/scripts/$script"
done
cat > "$contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.bridgevm.wrapper-policy-fixture</string>
<key>CFBundleName</key><string>BridgeVM</string>
<key>CFBundleExecutable</key><string>BridgeVMControl</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
codesign --force --sign - "$contents/MacOS/BridgeVMControl" "$probe" "$cli" "$swtpm" >/dev/null
codesign --force --deep --sign - "$app" >/dev/null
codesign --verify --deep --strict "$app" >/dev/null 2>&1 || fail 'fixture app signature failed'

ln -s "$resources/scripts/run-hvf-windows-installed-boot.sh" "$store/outside-entry"
trace="$(/bin/bash -x "$store/outside-entry" --help 2>&1)" || fail 'symlink entry help failed'
[[ "$trace" == *"+ ROOT=$resources"* ]] || fail 'symlink entry did not resolve canonical app resources'

selection='source "$1"; select_installed_boot_probe; [[ "$BIN" == "$2" ]]'
env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 VTPM_STATE_DIR='' \
  /bin/bash -c "$selection" _ "$policy" "$probe" || fail 'normal packaged probe selection failed'
env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 VTPM_STATE_DIR="$store/tpm" SWTPM_BIN="$swtpm" \
  /bin/bash -c "$selection" _ "$policy" "$probe" || fail 'normal packaged swtpm selection failed'
ln -s "$app" "$store/BridgeVMAlias.app"
env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 VTPM_STATE_DIR="$store/tpm" SWTPM_BIN="$store/BridgeVMAlias.app/Contents/Helpers/swtpm" \
  /bin/bash -c "$selection" _ "$policy" "$probe" || fail 'canonical bundled swtpm alias failed'
env ROOT="$resources" /bin/bash -c 'source "$1"; run_bridgevm_cli --version' _ "$policy" || fail 'packaged CLI failed'

refuse 'external Cargo target' 'forbids repository and Cargo target overrides' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 CARGO_TARGET_DIR="$store/external" \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"
refuse 'external prebuilt probe' 'forbids repository and Cargo target overrides' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 BRIDGEVM_PREBUILT_PROBE="$store/external" \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"
refuse 'external swtpm' 'requires its bundled swtpm' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 VTPM_STATE_DIR="$store/tpm" SWTPM_BIN=/usr/bin/true \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"

mv "$cli" "$store/removed-cli"
refuse 'missing packaged CLI' 'packaged executable is missing' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"
refuse 'missing packaged CLI fallback' 'packaged bridgevm CLI is missing' \
  env ROOT="$resources" /bin/bash -c 'source "$1"; run_bridgevm_cli --version' _ "$policy"
mv "$store/removed-cli" "$cli"

mv "$probe" "$store/original-probe"
ln -s /usr/bin/true "$probe"
refuse 'symlinked packaged probe' 'escapes its app' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"
rm "$probe"
mv "$store/original-probe" "$probe"
codesign --verify --deep --strict "$app" >/dev/null 2>&1 || fail 'fixture signature did not recover'
printf '\n' >> "$policy"
refuse 'tampered app signature' 'signature verification failed' \
  env ROOT="$resources" BUILD_PROFILE=release SKIP_BUILD=1 \
  /bin/bash -c 'source "$1"; select_installed_boot_probe' _ "$policy"

mkdir -p "$store/checkout"
env ROOT="$store/checkout" BUILD_PROFILE=debug SKIP_BUILD=0 CARGO_TARGET_DIR=isolated \
  /bin/bash -c 'source "$1"; select_installed_boot_probe; [[ "$BIN" == "$2" ]]' \
  _ "$ROOT_REPO/scripts/run-hvf-windows-installed-boot-package-policy.sh" \
  "$store/checkout/isolated/debug/examples/hvf_gic_boot_probe" || fail 'checkout selection changed'

echo 'packaged wrapper policy: PASS (signed resources selected; external overrides refused)'
