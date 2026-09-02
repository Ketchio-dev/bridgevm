#!/usr/bin/env bash
# Fixed N=10 B7 CoreAudio playback-and-shutdown campaign; no lane replacement.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
MANIFEST_TOOL="$REPO/scripts/live-gates/audio-teardown-manifest.py"
WRITER="$REPO/scripts/live-gates/write-audio-teardown-receipt.py"
LANE_RUNNER="$REPO/scripts/live-gates/run-audio-teardown-lane.sh"
OUT=""; INPUT_MANIFEST=""; SEALED_BINARY=""; JOB_ID=""; VALIDATE_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;; --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-binary) SEALED_BINARY="$2"; shift 2 ;; --job-id) JOB_ID="$2"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    *) echo "unknown B7 tier option $1" >&2; exit 2 ;;
  esac
done
[[ "$OUT" == /* && "$JOB_ID" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || exit 2
mkdir -p "$OUT"
PRIVATE="$OUT/private"; mkdir -m 700 "$PRIVATE"
[[ -z "$(find "$PRIVATE" -mindepth 1 -maxdepth 1 -print -quit)" ]] || exit 1
VERIFIED="$PRIVATE/verified-inputs.json"
COMMIT="$(git -C "$REPO" rev-parse HEAD)"; STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
ATTEMPTS=0; EMITTED=0; MEDIA="$PRIVATE/media"
json_value() {
  python3 - "$VERIFIED" "$1" <<'PY'
import json,sys
value=json.load(open(sys.argv[1],encoding="utf-8"))
for key in sys.argv[2].split("."): value=value[key]
print(value)
PY
}
cleanup_media() {
  [[ ! -e "$MEDIA" ]] && return 0
  [[ -d "$MEDIA" && ! -L "$MEDIA" && "$MEDIA" == "$PRIVATE/media" ]] || return 1
  mount | grep -F "$MEDIA" >/dev/null 2>&1 && return 1
  pgrep -f "$MEDIA" >/dev/null 2>&1 && return 1
  rm -rf -- "$MEDIA"
  [[ ! -e "$MEDIA" ]]
}
emit() {
  local outcome="$1" failure="$2" cleanup=0
  cleanup_media && cleanup=1 || { outcome=cleanup-failed; failure=cleanup-failed; }
  local args=(--out "$OUT/receipt.json" --private "$PRIVATE" --manifest "$INPUT_MANIFEST"
    --verified "$VERIFIED" --sealed-binary "$SEALED_BINARY" --job-id "$JOB_ID"
    --commit "$COMMIT" --started-at "$STARTED" --attempts "$ATTEMPTS"
    --outcome "$outcome" --failure-code "$failure")
  (( cleanup == 1 )) && args+=(--cleanup-verified)
  python3 "$WRITER" "${args[@]}"
  python3 "$REPO/scripts/verify-audio-teardown-receipt.py" "$OUT/receipt.json" \
    --expected-commit "$COMMIT" >/dev/null
  EMITTED=1
}
on_exit() {
  local status=$?
  trap - EXIT
  if (( EMITTED == 0 )); then
    emit failed internal-error || status=1
  fi
  exit "$status"
}
on_signal() {
  trap - INT TERM HUP
  emit canceled canceled || exit 1
  exit 130
}
trap on_exit EXIT; trap on_signal INT TERM HUP
set +e
python3 "$MANIFEST_TOOL" --manifest "$INPUT_MANIFEST" --repo "$REPO" \
  --sealed-binary "$SEALED_BINARY" --out "$VERIFIED"
manifest_status=$?
set -e
if (( manifest_status != 0 )); then emit preflight-blocked invalid-input; exit 1; fi
if (( VALIDATE_ONLY == 1 )); then
  printf '%s\n' 'B7 audio teardown manifest: PASS'; EMITTED=1; trap - EXIT; exit 0
fi
[[ ! -e "$OUT/cancel.requested" ]] || { emit canceled canceled; exit 1; }
IMAGE="$(json_value assets.image.path)"; VARS="$(json_value assets.vars.path)"
BINARY="$(json_value assets.binary.path)"; FIRMWARE="$(json_value assets.firmware.path)"
PLAYBACK="$(json_value assets.playback_script.path)"
output_device="$(stat -f %d "$OUT")"
[[ "$(stat -f %d "$IMAGE")" == "$output_device" && "$(stat -f %d "$VARS")" == "$output_device" ]] \
  || { emit preflight-blocked invalid-input; exit 1; }
mkdir -m 700 "$MEDIA"; inode_file="$PRIVATE/media-inodes.tsv"; : > "$inode_file"
for source in "$IMAGE" "$VARS"; do stat -f '%d\t%i' "$source" >> "$inode_file"; done
for ordinal in {1..10}; do
  lane_media="$MEDIA/lane-$ordinal"; mkdir -m 700 "$lane_media"
  cp -c "$IMAGE" "$lane_media/disk.raw" || { emit preflight-blocked invalid-input; exit 1; }
  cp -c "$VARS" "$lane_media/vars.fd" || { emit preflight-blocked invalid-input; exit 1; }
  chmod 600 "$lane_media/disk.raw" "$lane_media/vars.fd"
  stat -f '%d\t%i' "$lane_media/disk.raw" "$lane_media/vars.fd" >> "$inode_file"
done
[[ "$(wc -l < "$inode_file" | tr -d ' ')" == 22 && -z "$(sort "$inode_file" | uniq -d)" ]] \
  || { emit preflight-blocked invalid-input; exit 1; }
AFTER="$PRIVATE/verified-after-staging.json"
python3 "$MANIFEST_TOOL" --manifest "$INPUT_MANIFEST" --repo "$REPO" \
  --sealed-binary "$SEALED_BINARY" --out "$AFTER" >/dev/null
cmp -s "$VERIFIED" "$AFTER" || { emit failed invalid-input; exit 1; }
for ordinal in {1..10}; do
  [[ ! -e "$OUT/cancel.requested" ]] || { emit canceled canceled; exit 1; }
  ATTEMPTS=$ordinal; nonce="$(openssl rand -hex 32)"; lane="$PRIVATE/lane-$ordinal"
  set +e
  "$LANE_RUNNER" --disk "$MEDIA/lane-$ordinal/disk.raw" --vars "$MEDIA/lane-$ordinal/vars.fd" \
    --binary "$BINARY" --firmware "$FIRMWARE" --playback-script "$PLAYBACK" \
    --out "$lane" --nonce "$nonce" --ordinal "$ordinal"
  lane_status=$?
  set -e
  if (( lane_status != 0 )); then emit failed lane-failed; exit 1; fi
done
emit completed none
python3 - "$OUT/receipt.json" <<'PY'
import json,sys
raise SystemExit(0 if json.load(open(sys.argv[1]))["pass"] is True else 1)
PY
trap - EXIT
