#!/bin/bash
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != --output || $2 != /* ]]; then
  echo 'usage: build-app-ui-launcher.sh --output ABSOLUTE_NEW_FILE' >&2
  exit 2
fi
output=$2
if [[ -e "$output" || -L "$output" || ! -d "$(dirname "$output")" ]]; then
  echo 'launcher output must be new and its parent must exist' >&2
  exit 2
fi
script_dir=$(cd "$(dirname "$0")" && pwd -P)
scratch=$(mktemp -d "$(dirname "$output")/.app-ui-launcher-build.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
# Compilation only. NSWorkspace is invoked exclusively by the sealed live tier.
xcrun swiftc -parse-as-library -swift-version 5 -O \
  -target arm64-apple-macosx14.0 -module-cache-path "$scratch/module-cache" \
  -framework AppKit -framework CryptoKit \
  "$script_dir/AppUIHostLaunchOwnership.swift" "$script_dir/LaunchAppUIHost.swift" "$script_dir/AppUIHostLauncherMain.swift" \
  -o "$scratch/AppUIHostLauncher"
chmod 0500 "$scratch/AppUIHostLauncher"
# A hard-link publish refuses a concurrently created destination as well.
ln "$scratch/AppUIHostLauncher" "$output"
