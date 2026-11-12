#!/usr/bin/env bash
# Launch the packaged product E2E helper through LaunchServices, not by exec.
# Under the queue worker (LaunchAgent -> env -i -> bash -> exec) macOS attributed
# the helper's Accessibility check to that chain and refused it seven runs in a
# row, while the identical binary run from a Terminal shell passed and reached
# UI automation (2026-09-09). `open` makes the helper bundle its own responsible
# process; its grant must also match its current code requirement (A9 TCC note).
#
# open -W returns when the helper exits but does not carry its exit status; the
# result file is the contract, and the tier already treats its absence as a
# failure. No -a: several copies share this bundle id and -a launched an older
# one live (2026-09-09). Usage: launch-product-e2e-helper.sh HELPER LOG REQ RES
set -uo pipefail
helper="${1:?helper binary}"; log="${2:?log path}"; request="${3:?request}"; result="${4:?result}"
helper_app="${helper%/Contents/MacOS/*}"
[[ -d "$helper_app" && "$helper_app" == *.app ]] || { echo "helper is not inside an .app bundle: $helper" >&2; exit 2; }
launch_error="$(open -W -n "$helper_app" --stdout "$log" --stderr "$log" \
  --args --windows-product-e2e --request "$request" --result "$result" 2>&1)"
status=$?
[[ -z "$launch_error" ]] || printf '%s\n' "$launch_error" >&2
# A short-lived helper can exit before open registers its kevent watch. This is
# not a failed launch: accept only that exact wait error, with a regular result
# and no process still using this request. The tier still authenticates the
# result and proves lane cleanup. Never retry a possibly completed VM operation.
if (( status != 0 )) && [[ "$launch_error" == 'Unable to block on applications (initial call to kevent() failed: No such process)' && -f "$result" && ! -L "$result" ]]; then
  # Replace this shell: its argv also names the request and would match the probe.
  exec python3 "$(dirname "$0")/product_e2e_process_absent.py" "$request"
fi
exit "$status"
