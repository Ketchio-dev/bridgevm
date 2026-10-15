#!/usr/bin/env bash
# Produce a compressed .dmg for a packaged BridgeVM app.
#
# The dmg is the conventional-looking install path. Unlike the tarball it DOES
# acquire com.apple.quarantine when downloaded by a browser, so docs/install.md
# walks the user through the one-time "Open Anyway" flow.
set -euo pipefail

usage() { echo "usage: $0 --app <path/to/BridgeVM.app> --output <path/to/out.dmg>" >&2; exit 2; }

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
exec python3 "$SCRIPT_DIR/package_artifact.py" dmg "$APP" "$OUT"
