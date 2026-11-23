#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="${1:?run-special-tier.sh needs a tier}"; OUT="${2:?run-special-tier.sh needs an output directory}"
JOB_ID="${3:?run-special-tier.sh needs a job id}"; args=(--out "$OUT" --job-id "$JOB_ID")
case "$TIER" in d5-guest-input) exec "$REPO/scripts/live-gates/run-development-tier.sh" "$TIER" "$OUT" "$JOB_ID" "$REPO" "${4:-}" "${5:-}" ;;
  d6-app-ui|d6-app-ui-host-v1|d6-app-ui-host-v2|d6-app-ui-install-cancel-v1) adapter=app_ui_diagnostic.py; [[ "$TIER" != d6-app-ui-host-v1 ]] || adapter=app_ui_host_diagnostic.py; [[ "$TIER" != d6-app-ui-host-v2 ]] || adapter=app_ui_host_v2_diagnostic.py; [[ "$TIER" != d6-app-ui-install-cancel-v1 ]] || adapter=app_ui_install_cancel_diagnostic.py; exec python3 "$REPO/scripts/live-gates/$adapter" run "$OUT" "$REPO" "$(git -C "$REPO" rev-parse HEAD)" "${4:-}" "${5:-}" ;;
  t17-windows-hvf-product-e2e|t18-audio-teardown|t19-windows-hvf-import-product-e2e) helper=run-windows-product-e2e-tier.sh; [[ "$TIER" != t18-* ]] || helper=run-audio-teardown-tier.sh; [[ "$TIER" != t19-* ]] || helper=run-windows-import-product-e2e-tier.sh; args+=(--input-manifest "${4:-}"); [[ "$TIER" == t17-* || "$TIER" == t19-* ]] || args+=(--sealed-binary "${5:-}"); [[ "$TIER" != t17-* ]] || args+=(--retain-import-source "$(python3 "$REPO/scripts/live-gates/product_e2e_retained_destination.py" "$OUT" "$JOB_ID")") ;;
  *) echo "unknown special tier $TIER" >&2; exit 2 ;;
esac
"$REPO/scripts/live-gates/$helper" "${args[@]}"
