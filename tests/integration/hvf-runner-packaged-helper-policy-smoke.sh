#!/usr/bin/env bash
# A release runner only admits executable helpers inside its verified app bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
cargo +1.97.0 build --locked --release -p hvf-runner --quiet
runner="${CARGO_TARGET_DIR:-$ROOT/target}/release/hvf-runner"
store="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-runner-bundle.XXXXXX")" && pwd -P)"
trap 'rm -rf "$store"' EXIT
fail() { echo "release runner packaged helper policy: FAIL ($*)" >&2; exit 1; }
expect_refused() {
  local label="$1" expected="$2" output
  shift 2
  if output="$("$@" 2>&1)"; then fail "$label unexpectedly succeeded: $output"; fi
  [[ "$output" == *"$expected"* ]] || fail "$label missed '$expected': $output"
}

printf 'image' > "$store/disk.raw"
printf 'image' > "$store/vars.fd"
printf '{"version":1,"disk":"%s","uefi_vars":"%s","ram_mib":1024,"vcpus":1}\n' \
  "$store/disk.raw" "$store/vars.fd" > "$store/launch.json"
mkdir -p "$store/fake-checkout/.git"
fake_helper="$store/fake-checkout/fake-helper"
marker="$store/helper-executed"
printf '#!/bin/sh\nprintf executed > "%s"\n' "$marker" > "$fake_helper"
chmod +x "$fake_helper"
expect_refused 'checkout helper' 'requires packaged app topology' \
  "$runner" --launch-spec "$store/launch.json" --helper "$fake_helper"
[[ ! -e "$marker" ]] || fail 'checkout helper executed'
expect_refused 'validation-only launch' 'read launch manifest' \
  "$runner" --launch-spec "$store/absent.json"

app="$store/BridgeVM.app"
contents="$app/Contents"
bundle_runner="$contents/Resources/target/release/hvf-runner"
probe="$contents/Resources/target/release/examples/hvf_gic_boot_probe"
swtpm="$contents/Helpers/swtpm"
mkdir -p "$contents/MacOS" "$(dirname "$probe")" "$(dirname "$swtpm")"
cp "$runner" "$bundle_runner"
cp /usr/bin/true "$contents/MacOS/BridgeVMControl"
cp /usr/bin/true "$probe"
cp /usr/bin/true "$swtpm"
cat > "$contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.bridgevm.runner-policy-fixture</string>
<key>CFBundleName</key><string>BridgeVM</string>
<key>CFBundleExecutable</key><string>BridgeVMControl</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
codesign --force --sign - --entitlements apps/macos/HvfRunner.release.entitlements "$bundle_runner" >/dev/null
codesign --force --sign - "$probe" "$swtpm" "$contents/MacOS/BridgeVMControl" >/dev/null
codesign --force --deep --sign - "$app" >/dev/null
codesign --verify --deep --strict "$app" >/dev/null 2>&1 || fail 'signed fixture failed deep strict verification'

expect_refused 'signed bundled probe' 'read launch manifest' \
  "$bundle_runner" --launch-spec "$store/absent.json" --helper "$probe"
mkdir -p "$store/tpm"
expect_refused 'signed bundled swtpm' 'read launch manifest' \
  "$bundle_runner" --launch-spec "$store/absent.json" --helper "$probe" \
    --helper-vtpm-state "$store/tpm" --helper-swtpm-bin "$swtpm"
expect_refused 'missing bundled swtpm' 'requires explicit packaged --helper-swtpm-bin' \
  "$bundle_runner" --launch-spec "$store/launch.json" --helper "$probe" \
    --helper-vtpm-state "$store/tpm"
expect_refused 'arbitrary absolute swtpm' 'release --helper-swtpm-bin must use the packaged app executable' \
  "$bundle_runner" --launch-spec "$store/launch.json" --helper "$probe" \
    --helper-vtpm-state "$store/tpm" --helper-swtpm-bin "$fake_helper"
[[ ! -e "$marker" ]] || fail 'arbitrary swtpm executed'

rm "$probe"
ln -s "$fake_helper" "$probe"
expect_refused 'symlinked bundled probe' 'release --helper must use the packaged app executable' \
  "$bundle_runner" --launch-spec "$store/launch.json" --helper "$probe"
[[ ! -e "$marker" ]] || fail 'symlinked probe executed'

echo 'release runner packaged helper policy: PASS (signed bundle accepted; external and symlinked executables refused)'
