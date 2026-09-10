#!/usr/bin/env bash
# Launch the packaged product E2E helper through LaunchServices, not by exec.
# Under the queue worker (LaunchAgent -> env -i -> bash -> exec) macOS attributed
# the helper's Accessibility check to that chain and refused it seven runs in a
# row, while the identical binary run from a Terminal shell passed and reached
# UI automation (2026-09-09). `open` makes the helper bundle its own responsible
# process, so the grant the user gave to dev.bridgevm.product-e2e applies.
#
# open -W returns when the helper exits but does not carry its exit status; the
# result file is the contract, and the tier already treats its absence as a
# failure. No -a: several copies share this bundle id and -a launched an older
# one live (2026-09-09). Usage: launch-product-e2e-helper.sh HELPER LOG REQ RES
set -uo pipefail
helper="${1:?helper binary}"; log="${2:?log path}"; request="${3:?request}"; result="${4:?result}"
helper_app="${helper%/Contents/MacOS/*}"
[[ -d "$helper_app" && "$helper_app" == *.app ]] || { echo "helper is not inside an .app bundle: $helper" >&2; exit 2; }
exec open -W -n "$helper_app" --stdout "$log" --stderr "$log" \
  --args --windows-product-e2e --request "$request" --result "$result"
