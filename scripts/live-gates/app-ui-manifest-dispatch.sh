#!/usr/bin/env bash
# Diagnostic manifest preparation only; other tiers retain their own sealing.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:?mode}"; TIER="${2:?tier}"; shift 2
case "$MODE:$TIER" in
    validate:d6-app-ui)
        exec python3 "$HERE/app_ui_diagnostic.py" validate-manifest "$@" ;;
    validate:d6-app-ui-host-v1)
        exec python3 "$HERE/app_ui_host_manifest.py" validate-manifest "$@" ;;
    seal:d6-app-ui-host-v1)
        exec python3 "$HERE/app_ui_host_manifest.py" seal-launcher "$@" ;;
    validate:*|seal:*) exit 0 ;;
    *) echo "unknown diagnostic manifest operation" >&2; exit 2 ;;
esac
