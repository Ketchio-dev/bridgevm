#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-vz-proxy-crop-evidence.XXXXXX")"
EVIDENCE_DIR="$STORE/evidence"

fail() {
  echo "FAIL: $*" >&2
  echo "Evidence store preserved at $STORE" >&2
  exit 1
}

json_string() {
  python3 -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$1"
}

file_sha() {
  shasum -a 256 "$1" | awk '{print $1}'
}

file_size() {
  wc -c <"$1" | tr -d ' '
}

expect_verifier_rejects() {
  local label="$1"
  if tests/integration/verify-vz-proxy-crop-evidence.sh "$EVIDENCE_DIR" >/dev/null 2>&1; then
    fail "verifier accepted evidence with $label"
  fi
}

displayd() {
  cargo run --quiet -p displayd -- "$@"
}

mkdir -p "$EVIDENCE_DIR"

FRAMEBUFFER_RGBA="$EVIDENCE_DIR/app-direct-framebuffer.rgba"
WINDOW_CROP_JSON="$EVIDENCE_DIR/app-direct-window-crop.json"
WINDOW_CROP_RGBA="$EVIDENCE_DIR/app-direct-window-crop.rgba"
VIEWER_FRAME="$EVIDENCE_DIR/viewer-frame.png"
SERIAL_LOG="$EVIDENCE_DIR/serial.log"
RUNNER_OUTPUT="$EVIDENCE_DIR/apple-vz-display-runner.output"

python3 - "$FRAMEBUFFER_RGBA" <<'PY'
import sys
path = sys.argv[1]
frame = bytearray()
for y in range(3):
    for x in range(4):
        frame.extend([x * 40, y * 60, (x + y) * 20, 255])
with open(path, "wb") as handle:
    handle.write(frame)
PY

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 4 \
  --framebuffer-width 4 \
  --framebuffer-height 3 \
  --scale 1 \
  --window-id app-direct-proof \
  --window-title "App Direct Crop Proof" \
  --window-x 1 \
  --window-y 1 \
  --window-width 2 \
  --window-height 2 \
  --framebuffer-rgba-file "$FRAMEBUFFER_RGBA" \
  --window-crop-rgba-file "$WINDOW_CROP_RGBA" \
  >"$WINDOW_CROP_JSON"

base64 --decode >"$VIEWER_FRAME" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=
EOF
printf "Run /init\nDebian installer\n" >"$SERIAL_LOG"
printf "AppleVzRunner starting VM: vz-display-demo\nAppleVzRunner VM finished: vz-display-demo\n" >"$RUNNER_OUTPUT"

cat >"$EVIDENCE_DIR/viewer-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-viewer",
  "artifact": "viewer-frame.png",
  "width": 1,
  "height": 1,
  "sha256": "$(file_sha "$VIEWER_FRAME")",
  "observation": "screencapture captured the BridgeVM Apple VZ display window"
}
EOF

cat >"$EVIDENCE_DIR/display-window-proof.json" <<EOF
{
  "proven": true,
  "mode": "apple-vz-display-window-proxy-crop",
  "runner": "/tmp/AppleVzRunner",
  "runner_pid": 1234,
  "window_id": "99",
  "requested_width": 4,
  "requested_height": 3,
  "captured_width": 1,
  "captured_height": 1,
  "proof_seconds": 18,
  "capture_delay_seconds": 6,
  "viewer_frame": {
    "artifact": "viewer-frame.png",
    "sha256": "$(file_sha "$VIEWER_FRAME")"
  },
  "serial_log": {
    "artifact": "serial.log",
    "sha256": "$(file_sha "$SERIAL_LOG")"
  },
  "runner_output": {
    "artifact": "apple-vz-display-runner.output",
    "sha256": "$(file_sha "$RUNNER_OUTPUT")"
  }
}
EOF

cat >"$EVIDENCE_DIR/app-direct-proxy-crop-proof.json" <<EOF
{
  "proven": true,
  "kind": "app-direct-proxy-crop",
  "framebuffer": {
    "artifact": "app-direct-framebuffer.rgba",
    "width": 4,
    "height": 3,
    "bytes": $(file_size "$FRAMEBUFFER_RGBA"),
    "sha256": "$(file_sha "$FRAMEBUFFER_RGBA")"
  },
  "crop": {
    "summary_artifact": "app-direct-window-crop.json",
    "rgba_artifact": "app-direct-window-crop.rgba",
    "x": 1,
    "y": 1,
    "width": 2,
    "height": 2,
    "bytes": $(file_size "$WINDOW_CROP_RGBA"),
    "summary_sha256": "$(file_sha "$WINDOW_CROP_JSON")",
    "rgba_sha256": "$(file_sha "$WINDOW_CROP_RGBA")"
  },
  "observation": "AppleVzRunner exported the app-direct VZVirtualMachineView RGBA file and displayd materialized a crop artifact from it"
}
EOF

cat >"$EVIDENCE_DIR/SUMMARY.txt" <<EOF
Fast Mode embedded display window proof: passed
Evidence directory: $(json_string "$EVIDENCE_DIR")
Window info: window_id=99 owner=AppleVzRunner name=BridgeVM - vz-display-demo width=4 height=3
Captured frame: viewer-frame.png (1x1)
Runner output: apple-vz-display-runner.output
Serial log: serial.log
App-direct framebuffer: app-direct-framebuffer.rgba (4x3, $(file_size "$FRAMEBUFFER_RGBA") bytes)
Displayd crop summary: app-direct-window-crop.json
Displayd crop RGBA: app-direct-window-crop.rgba (2x2, $(file_size "$WINDOW_CROP_RGBA") bytes)
EOF

tests/integration/verify-vz-proxy-crop-evidence.sh "$EVIDENCE_DIR" >/dev/null

cp "$EVIDENCE_DIR/app-direct-proxy-crop-proof.json" "$EVIDENCE_DIR/app-direct-proxy-crop-proof.good.json"
perl -0pi -e 's/"bytes": 16/"bytes": 20/' "$EVIDENCE_DIR/app-direct-proxy-crop-proof.json"
expect_verifier_rejects "wrong crop byte count"
mv "$EVIDENCE_DIR/app-direct-proxy-crop-proof.good.json" "$EVIDENCE_DIR/app-direct-proxy-crop-proof.json"

cp "$EVIDENCE_DIR/viewer-evidence.json" "$EVIDENCE_DIR/viewer-evidence.good.json"
perl -0pi -e 's/"artifact": "viewer-frame[.]png"/"artifact": "..\/viewer-frame.png"/' "$EVIDENCE_DIR/viewer-evidence.json"
expect_verifier_rejects "viewer artifact path traversal"
mv "$EVIDENCE_DIR/viewer-evidence.good.json" "$EVIDENCE_DIR/viewer-evidence.json"

cp "$WINDOW_CROP_RGBA" "$EVIDENCE_DIR/app-direct-window-crop.good.rgba"
printf "bad" >"$WINDOW_CROP_RGBA"
expect_verifier_rejects "crop artifact size mismatch"
mv "$EVIDENCE_DIR/app-direct-window-crop.good.rgba" "$WINDOW_CROP_RGBA"

tests/integration/verify-vz-proxy-crop-evidence.sh "$EVIDENCE_DIR" >/dev/null

echo "PASS: VZ proxy crop evidence verifier smoke ($STORE)"
