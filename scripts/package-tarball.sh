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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/package_artifact.py" tarball "$APP" "$OUT"
