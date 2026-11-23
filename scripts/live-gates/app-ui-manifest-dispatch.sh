#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"; MODE="${1:?mode}"; TIER="${2:?tier}"; shift 2
case "$MODE:$TIER" in
    validate:d6-app-ui) exec python3 "$HERE/app_ui_diagnostic.py" validate-manifest "$@" ;;
    validate:d6-app-ui-host-v1) exec python3 "$HERE/app_ui_host_manifest.py" validate-manifest "$@" ;;
    seal:d6-app-ui-host-v1)
        exec python3 "$HERE/app_ui_host_manifest.py" seal-launcher "$@" ;;
    validate:d6-app-ui-host-v2|validate:d6-app-ui-install-cancel-v1) HELPER=app_ui_host_v2_manifest.py; [[ "$TIER" != d6-app-ui-install-cancel-v1 ]] || HELPER=app_ui_install_cancel_manifest.py; exec python3 "$HERE/$HELPER" validate-manifest "$@" ;;
    seal:d6-app-ui-host-v2|seal:d6-app-ui-install-cancel-v1) HELPER=app_ui_host_v2_manifest.py; [[ "$TIER" != d6-app-ui-install-cancel-v1 ]] || HELPER=app_ui_install_cancel_manifest.py; exec python3 "$HERE/$HELPER" seal-inputs "$@" ;;
    validate:t17-windows-hvf-product-e2e) python3 "$HERE/windows-product-e2e-launchservices-preflight.py" --manifest "$1" && exec python3 "$HERE/host_console_session.py" --require-unlocked ;; validate:t19-windows-hvf-import-product-e2e) exec python3 "$HERE/host_console_session.py" --require-unlocked ;;
    validate:*|seal:*) exit 0 ;;
    *) echo "unknown diagnostic manifest operation" >&2; exit 2 ;;
esac
