#!/usr/bin/env bash
# Sealed packaged-product T17 scaffold. It never substitutes harness or synthetic guest success.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; JOB_ID=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown T17 option: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$JOB_ID" ]] || exit 2
[[ "$JOB_ID" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || exit 2
mkdir -p "$OUT"
PRIVATE="$OUT/private"; mkdir -m 700 "$PRIVATE"
[[ -z "$(find "$PRIVATE" -mindepth 1 -maxdepth 1 -print -quit)" ]] || { echo "T17 private result directory is not empty" >&2; exit 1; }
MANIFEST_TOOL="$REPO/scripts/live-gates/windows-product-e2e-manifest.py"
WRITER="$REPO/scripts/live-gates/write-windows-product-e2e-receipt.py"
REQUEST_WRITER="$REPO/scripts/live-gates/make-windows-product-e2e-request.py"
VERIFIED="$PRIVATE/verified-inputs.json"
COMMIT="$(git -C "$REPO" rev-parse HEAD)"
STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
WORK=""; EMITTED=0; MODE=pilot; ATTEMPTS=0; VALID=false; SIGNING=development-ad-hoc

json_value() {
  python3 - "$1" "$2" <<'PY'
import json, sys
value = json.load(open(sys.argv[1], encoding="utf-8"))
for part in sys.argv[2].split("."):
    value = value[part]
if not isinstance(value, (str, int, bool)):
    raise SystemExit(2)
print(str(value).lower() if isinstance(value, bool) else value)
PY
}

cleanup_work() {
  [[ -z "$WORK" ]] && return 0
  case "$WORK" in "/tmp/bridgevm-e2e-$JOB_ID."??????) ;; *) return 1 ;; esac
  mount | grep -F "$WORK" >/dev/null 2>&1 && return 1
  pgrep -f "$WORK" >/dev/null 2>&1 && return 1
  rm -rf -- "$WORK"
  [[ ! -e "$WORK" ]]
}

emit() {
  local outcome="$1" failure="$2" attempts="$3" valid="$4" signing="${5:-development-ad-hoc}" cleanup=0
  cleanup_work && cleanup=1 || { outcome=cleanup-failed; failure=cleanup-failed; : > "$PRIVATE/cleanup-failed"; }
  local args=(--out "$OUT/receipt.json" --private "$PRIVATE" --input-manifest "$INPUT_MANIFEST" --verified "$VERIFIED" --job-id "$JOB_ID" --commit "$COMMIT" --mode "$MODE" --attempts "$attempts" --started-at "$STARTED" --outcome "$outcome" --failure-code "$failure" --signing-class "$signing")
  (( cleanup == 1 )) && args+=(--cleanup)
  [[ "$valid" == true ]] && args+=(--valid)
  EMITTED=1; trap - EXIT
  python3 "$WRITER" "${args[@]}" || return 1
  [[ "$outcome" == completed ]] && python3 "$REPO/scripts/verify-windows-product-e2e-receipt.py" "$OUT/receipt.json" --expected-commit "$COMMIT" >/dev/null
}

on_exit() {
  local status=$?
  (( EMITTED == 1 )) || cleanup_work || : > "$PRIVATE/cleanup-failed"
  return "$status"
}
on_signal() {
  trap - INT TERM HUP
  emit canceled canceled "$ATTEMPTS" "$VALID" "$SIGNING" || exit 1
  exit 130
}
trap on_exit EXIT
trap on_signal INT TERM HUP

set +e
python3 "$MANIFEST_TOOL" --manifest "$INPUT_MANIFEST" --out "$VERIFIED"
manifest_status=$?
set -e
MODE="$(json_value "$VERIFIED" campaign_mode 2>/dev/null || echo pilot)"
if (( manifest_status != 0 )); then
  failure="$(json_value "$VERIFIED" failure_code 2>/dev/null || echo internal-error)"
  valid="$(json_value "$VERIFIED" valid 2>/dev/null || echo false)"
  emit preflight-blocked "$failure" 0 "$valid" || exit 1
  exit 1
fi
VALID=true

if [[ -f "$OUT/cancel.requested" ]]; then emit canceled canceled 0 true || exit 1; exit 1; fi
APP="$(json_value "$VERIFIED" assets.app_bundle.path)"
HELPER="$(json_value "$VERIFIED" assets.product_helper.path)"
if ! codesign --verify --deep --strict "$APP" >/dev/null 2>&1; then emit preflight-blocked product-model-failed 0 true || exit 1; exit 1; fi
if codesign -dv --verbose=4 "$APP" 2>&1 | grep -q 'Authority=Developer ID Application' && spctl --assess --type execute "$APP" >/dev/null 2>&1; then SIGNING=developer-id-notarized; fi

WORK="$(mktemp -d "/tmp/bridgevm-e2e-$JOB_ID.XXXXXX")"
chmod 700 "$WORK"
EXPECTED=1; [[ "$MODE" == release ]] && EXPECTED=3
previous_inode=""
for (( lane=1; lane<=EXPECTED; lane++ )); do
  if [[ -f "$OUT/cancel.requested" ]]; then emit canceled canceled "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
  lane_root="$WORK/lane-$lane"; mkdir -m 700 "$lane_root"
  inode="$(stat -f '%i' "$lane_root")"
  [[ -z "$previous_inode" || "$inode" != "$previous_inode" ]] || { emit failed internal-error "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; }
  previous_inode="$inode"
  nonce="$(openssl rand -hex 32)"
  request="$lane_root/request.json"; result="$PRIVATE/lane-$lane-result.json"
  python3 "$REQUEST_WRITER" --out "$request" --verified "$VERIFIED" --job-id "$JOB_ID" \
    --commit "$COMMIT" --mode "$MODE" --lane "$lane" --nonce "$nonce" --lane-root "$lane_root"
  ATTEMPTS=$lane
  set +e
  "$HELPER" --windows-product-e2e --request "$request" --result "$result" >"$PRIVATE/lane-$lane-helper.log" 2>&1
  helper_status=$?
  set -e
  if (( helper_status != 0 )) || [[ ! -f "$result" || -L "$result" ]]; then emit failed product-model-failed "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
  if find "$WORK" \( \( ! -type d ! -type f \) -o \( -type f -links +1 \) \) -print -quit | grep -q .; then emit failed integration-failed "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
  if mount | grep -F "$lane_root" >/dev/null 2>&1 || pgrep -f "$lane_root" >/dev/null 2>&1; then emit cleanup-failed cleanup-failed "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
  if ! python3 "$WRITER" --check-lane "$result" --request "$request" --stamp "$PRIVATE/lane-$lane-authenticated.json" --job-id "$JOB_ID" --commit "$COMMIT" --mode "$MODE" --ordinal "$lane"; then emit failed integration-failed "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
  next_verified="$PRIVATE/verified-after-lane-$lane.json"
  if ! python3 "$MANIFEST_TOOL" --manifest "$INPUT_MANIFEST" --out "$next_verified" >/dev/null 2>&1 || ! cmp -s "$VERIFIED" "$next_verified"; then emit failed hash-mismatch "$ATTEMPTS" true "$SIGNING" || exit 1; exit 1; fi
done
emit completed none "$ATTEMPTS" true "$SIGNING" || exit 1
python3 - "$OUT/receipt.json" <<'PY'
import json, sys
raise SystemExit(0 if json.load(open(sys.argv[1]))["pass"] is True else 1)
PY
