#!/usr/bin/env bash
# Give the fixed nested helper its LaunchServices identity for Accessibility attribution.
set -uo pipefail
helper="${1:?helper binary}"; log="${2:?log path}"; request="${3:?request}"; result="${4:?result}"
helper_app="${helper%/Contents/MacOS/*}"
[[ -d "$helper_app" && "$helper_app" == *.app ]] || { echo "helper is not inside an .app bundle: $helper" >&2; exit 2; }
exec open -W -n "$helper_app" --stdout "$log" --stderr "$log" \
  --args --windows-import-product-e2e --request "$request" --result "$result"
