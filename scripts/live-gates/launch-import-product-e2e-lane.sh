#!/usr/bin/env bash
# Launch one T19 lane's helper with the options its verified inputs seal. The only one is the opt-in
# BitLocker restart check (docs/history/windows-hvf/bitlocker-restart-check-20261023.md); without it the
# launcher receives exactly the four arguments it always received.
set -uo pipefail
verified="${1:?verified inputs}"; shift
here="$(cd "$(dirname "$0")" && pwd)"
switch="$(python3 "$here/product_e2e_json_value.py" "$verified" bitlocker_restart_check 2>/dev/null)"
[[ "$switch" == tpm-only ]] && exec "$here/launch-import-product-e2e-helper.sh" "$@" --bitlocker-restart-check
exec "$here/launch-import-product-e2e-helper.sh" "$@"
