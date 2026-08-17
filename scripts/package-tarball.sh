#!/usr/bin/env bash
# Produce the curl-installable .tar.gz for a packaged BridgeVM app.
#
# A tarball fetched with curl carries no com.apple.quarantine attribute, so an
# app installed from it launches without the Gatekeeper "unidentified
# developer" block. That makes this the primary no-Developer-ID install path;
# the dmg (package-dmg.sh) exists for people who expect one.
set -euo pipefail

usage() { echo "usage: $0 --app <path/to/BridgeVM.app> --output <path/to/out.tar.gz>" >&2; exit 2; }

APP="" OUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP=$2; shift 2 ;;
    --output) OUT=$2; shift 2 ;;
    *) usage ;;
  esac
done
[[ -n "$APP" && -n "$OUT" ]] || usage
[[ -d "$APP" ]] || { echo "FAIL: app bundle not found: $APP" >&2; exit 1; }

# The bundle must carry a signature (ad-hoc is fine) or macOS kills it on
# launch regardless of quarantine.
codesign --verify --deep --strict "$APP" ||
  { echo "FAIL: $APP does not pass codesign verification" >&2; exit 1; }

app_dir=$(dirname "$APP")
app_name=$(basename "$APP")
mkdir -p "$(dirname "$OUT")"
tar -czf "$OUT" -C "$app_dir" "$app_name"

# Prove the round trip: extract into a scratch dir and re-verify the signature.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
tar -xzf "$OUT" -C "$scratch"
codesign --verify --deep --strict "$scratch/$app_name" ||
  { echo "FAIL: signature broken by tar round trip" >&2; exit 1; }

shasum -a 256 "$OUT"
echo "tarball: PASS ($OUT)"
