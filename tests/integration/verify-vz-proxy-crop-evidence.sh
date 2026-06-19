#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <evidence-dir>" >&2
  exit 2
fi

python3 - "$1" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])

def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)

if not root.is_dir():
    fail(f"evidence directory not found: {root}")

try:
    root_resolved = root.resolve(strict=True)
except Exception as exc:
    fail(f"evidence directory cannot be resolved: {exc}")

def load_json(name):
    path = root / name
    if not path.is_file():
        fail(f"missing evidence JSON: {name}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        fail(f"{name} is not valid JSON: {exc}")

def require_file(name, label):
    path = root / name
    if not path.is_file():
        fail(f"{label} missing: {name}")
    if path.is_symlink():
        fail(f"{label} must not be a symlink: {name}")
    return path

def artifact_path(value, label):
    if not isinstance(value, str) or not value:
        fail(f"{label} artifact path is missing")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        fail(f"{label} artifact path must be relative and inside evidence: {value}")
    full = root / path
    try:
        resolved = full.resolve(strict=False)
    except Exception as exc:
        fail(f"{label} artifact path cannot resolve: {value}: {exc}")
    if resolved != root_resolved and root_resolved not in resolved.parents:
        fail(f"{label} artifact path escapes evidence: {value}")
    if full.is_symlink():
        fail(f"{label} artifact must not be a symlink: {value}")
    if not full.is_file():
        fail(f"{label} artifact missing: {value}")
    return full

def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def expect_sha(path, expected, label):
    if not isinstance(expected, str) or not expected:
        fail(f"{label} missing SHA-256")
    if sha256(path) != expected:
        fail(f"{label} SHA-256 mismatch")

def positive_int(value, label):
    if not isinstance(value, int) or value <= 0:
        fail(f"{label} must be a positive integer")
    return value

def nonnegative_int(value, label):
    if not isinstance(value, int) or value < 0:
        fail(f"{label} must be a non-negative integer")
    return value

def png_dimensions(path):
    data = path.read_bytes()
    if len(data) < 24 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        fail(f"{path.name} is not a PNG with an IHDR header")
    width = int.from_bytes(data[16:20], "big")
    height = int.from_bytes(data[20:24], "big")
    if width <= 0 or height <= 0:
        fail(f"{path.name} has invalid PNG dimensions")
    return width, height

summary_path = require_file("SUMMARY.txt", "summary")
summary = summary_path.read_text(encoding="utf-8", errors="replace")
if "Fast Mode embedded display window proof: passed" not in summary:
    fail("SUMMARY.txt does not record a passed display proof")
if "App-direct framebuffer:" not in summary:
    fail("SUMMARY.txt does not record the app-direct framebuffer")

viewer = load_json("viewer-evidence.json")
if viewer.get("proven") is not True:
    fail("viewer-evidence.json does not mark evidence as proven")
if viewer.get("kind") != "graphical-viewer":
    fail("viewer-evidence.json kind is not graphical-viewer")
viewer_frame = artifact_path(viewer.get("artifact"), "viewer frame")
expect_sha(viewer_frame, viewer.get("sha256"), "viewer frame")
viewer_width, viewer_height = png_dimensions(viewer_frame)
if viewer_width != positive_int(viewer.get("width"), "viewer width"):
    fail("viewer-evidence.json width does not match PNG")
if viewer_height != positive_int(viewer.get("height"), "viewer height"):
    fail("viewer-evidence.json height does not match PNG")
if not isinstance(viewer.get("observation"), str) or not viewer["observation"]:
    fail("viewer-evidence.json observation is missing")

display = load_json("display-window-proof.json")
if display.get("proven") is not True:
    fail("display-window-proof.json does not mark evidence as proven")
if display.get("mode") != "apple-vz-display-window-proxy-crop":
    fail("display-window-proof.json mode is not proxy-crop")
requested_width = positive_int(display.get("requested_width"), "requested width")
requested_height = positive_int(display.get("requested_height"), "requested height")
captured_width = positive_int(display.get("captured_width"), "captured width")
captured_height = positive_int(display.get("captured_height"), "captured height")
if captured_width != viewer_width or captured_height != viewer_height:
    fail("display-window-proof.json captured size does not match viewer PNG")
proof_seconds = positive_int(display.get("proof_seconds"), "proof seconds")
capture_delay = positive_int(display.get("capture_delay_seconds"), "capture delay seconds")
if capture_delay >= proof_seconds:
    fail("display-window-proof.json capture delay is not less than proof duration")
display_viewer = display.get("viewer_frame") or {}
if display_viewer.get("artifact") != viewer.get("artifact"):
    fail("display-window-proof.json viewer artifact does not match viewer evidence")
if display_viewer.get("sha256") != viewer.get("sha256"):
    fail("display-window-proof.json viewer SHA-256 does not match viewer evidence")
serial = display.get("serial_log") or {}
runner_output = display.get("runner_output") or {}
serial_path = artifact_path(serial.get("artifact"), "serial log")
runner_output_path = artifact_path(runner_output.get("artifact"), "runner output")
if serial.get("sha256"):
    expect_sha(serial_path, serial.get("sha256"), "serial log")
expect_sha(runner_output_path, runner_output.get("sha256"), "runner output")

crop_proof = load_json("app-direct-proxy-crop-proof.json")
if crop_proof.get("proven") is not True:
    fail("app-direct-proxy-crop-proof.json does not mark evidence as proven")
if crop_proof.get("kind") != "app-direct-proxy-crop":
    fail("app-direct-proxy-crop-proof.json kind is not app-direct-proxy-crop")

framebuffer = crop_proof.get("framebuffer") or {}
fb_artifact = artifact_path(framebuffer.get("artifact"), "framebuffer")
fb_width = positive_int(framebuffer.get("width"), "framebuffer width")
fb_height = positive_int(framebuffer.get("height"), "framebuffer height")
fb_bytes = positive_int(framebuffer.get("bytes"), "framebuffer bytes")
if fb_width != requested_width or fb_height != requested_height:
    fail("framebuffer dimensions do not match requested display size")
if fb_bytes != fb_width * fb_height * 4:
    fail("framebuffer byte count is not width*height*4")
if fb_artifact.stat().st_size != fb_bytes:
    fail("framebuffer file size does not match proof byte count")
expect_sha(fb_artifact, framebuffer.get("sha256"), "framebuffer")

crop = crop_proof.get("crop") or {}
crop_summary = artifact_path(crop.get("summary_artifact"), "crop summary")
crop_rgba = artifact_path(crop.get("rgba_artifact"), "crop RGBA")
crop_x = nonnegative_int(crop.get("x"), "crop x")
crop_y = nonnegative_int(crop.get("y"), "crop y")
crop_width = positive_int(crop.get("width"), "crop width")
crop_height = positive_int(crop.get("height"), "crop height")
crop_bytes = positive_int(crop.get("bytes"), "crop bytes")
if crop_x + crop_width > fb_width or crop_y + crop_height > fb_height:
    fail("crop rectangle escapes framebuffer")
if crop_bytes != crop_width * crop_height * 4:
    fail("crop byte count is not width*height*4")
if crop_rgba.stat().st_size != crop_bytes:
    fail("crop RGBA file size does not match proof byte count")
expect_sha(crop_summary, crop.get("summary_sha256"), "crop summary")
expect_sha(crop_rgba, crop.get("rgba_sha256"), "crop RGBA")

crop_plan = load_json(crop.get("summary_artifact"))
framebuffer_plan = crop_plan.get("framebuffer") or {}
if framebuffer_plan.get("width") != fb_width or framebuffer_plan.get("height") != fb_height:
    fail("crop summary framebuffer dimensions do not match proof")
region = crop_plan.get("window_region") or {}
if region.get("presentation") != "proxy-window-crop":
    fail("crop summary window_region presentation is not proxy-window-crop")
clipped = region.get("clipped_rect") or {}
for key, expected in {
    "x": crop_x,
    "y": crop_y,
    "width": crop_width,
    "height": crop_height,
}.items():
    if clipped.get(key) != expected:
        fail(f"crop summary clipped_rect.{key} does not match proof")
crop_frame = crop_plan.get("window_crop_frame") or {}
if crop_frame.get("presentation") != "proxy-window-crop-frame":
    fail("crop summary window_crop_frame presentation is not proxy-window-crop-frame")
if crop_frame.get("pixel_format") != "rgba8":
    fail("crop summary pixel_format is not rgba8")
if crop_frame.get("framebuffer_width") != fb_width or crop_frame.get("framebuffer_height") != fb_height:
    fail("crop summary crop-frame framebuffer dimensions do not match proof")
if crop_frame.get("expected_input_bytes") != fb_bytes:
    fail("crop summary expected_input_bytes does not match framebuffer bytes")
if crop_frame.get("source_len_bytes") != fb_bytes:
    fail("crop summary source_len_bytes does not match framebuffer bytes")
if crop_frame.get("output_bytes") != crop_bytes:
    fail("crop summary output_bytes does not match crop bytes")
rect = crop_frame.get("crop_rect") or {}
for key, expected in {
    "x": crop_x,
    "y": crop_y,
    "width": crop_width,
    "height": crop_height,
}.items():
    if rect.get(key) != expected:
        fail(f"crop summary crop_rect.{key} does not match proof")

print("PASS: VZ proxy crop evidence verified")
PY
