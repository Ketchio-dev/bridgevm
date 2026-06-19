#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-displayd-plan.XXXXXX")"
PRESERVE_STORE=1

displayd() {
  cargo run --quiet -p displayd -- "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

cleanup() {
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for JSON assertions"

FOREGROUND_JSON="$STORE/foreground.json"
BACKGROUND_JSON="$STORE/background.json"
HIDDEN_JSON="$STORE/hidden.json"
RESIZE_CURSOR_JSON="$STORE/resize-cursor.json"
COALESCED_JSON="$STORE/coalesced.json"
FRAME_SAMPLE_JSON="$STORE/frame-sample.json"
FRAME_SAMPLE_FILE="$STORE/frame-samples.json"
BAD_FRAME_SAMPLE_FILE="$STORE/bad-frame-samples.json"
RUNTIME_POLICY_FILE="$STORE/runtime-resources.json"
RUNTIME_POLICY_JSON="$STORE/runtime-policy.json"
WINDOW_REGION_JSON="$STORE/window-region.json"
FRAMEBUFFER_RGBA_FILE="$STORE/framebuffer.rgba"
BAD_FRAMEBUFFER_RGBA_FILE="$STORE/bad-framebuffer.rgba"
WINDOW_CROP_RGBA_FILE="$STORE/window-crop.rgba"
WINDOW_CROP_JSON="$STORE/window-crop-frame.json"

printf '[16000,17000,18000]\n' >"$FRAME_SAMPLE_FILE"
printf '[16000,0]\n' >"$BAD_FRAME_SAMPLE_FILE"
python3 - "$FRAMEBUFFER_RGBA_FILE" "$BAD_FRAMEBUFFER_RGBA_FILE" <<'PY'
import sys

frame_path, bad_frame_path = sys.argv[1:3]
frame = bytearray()
for y in range(3):
    for x in range(4):
        frame.extend([x, y, y * 4 + x, 255])
with open(frame_path, "wb") as handle:
    handle.write(frame)
with open(bad_frame_path, "wb") as handle:
    handle.write(bytes([0, 0, 0, 255]))
PY
cat >"$RUNTIME_POLICY_FILE" <<'JSON'
{
  "vm": "fast-dev",
  "mode": "fast",
  "profile": "automatic",
  "visibility": "background",
  "state": "running",
  "on_battery": true,
  "memory": "2048",
  "cpu": "1",
  "display_fps_cap": "5",
  "rationale": "runtime display pacing smoke",
  "live_applied": false,
  "live_apply_blockers": [],
  "updated_at_unix": 1
}
JSON

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 4 \
  --framebuffer-width 1440 \
  --framebuffer-height 900 \
  --scale 2 \
  --sample-frames 120 \
  --frame-time-micros 16000 \
  >"$FOREGROUND_JSON"

displayd \
  --print-plan \
  --visibility background \
  --dirty-regions 12 \
  --sample-frames 30 \
  --frame-time-micros 150000 \
  >"$BACKGROUND_JSON"

displayd \
  --print-plan \
  --visibility hidden \
  --dirty-regions 12 \
  >"$HIDDEN_JSON"

displayd \
  --print-plan \
  --dirty-regions 0 \
  --resize-width 1680 \
  --resize-height 1050 \
  --scale 2 \
  --cursor-x 5000 \
  --cursor-y 2000 \
  >"$RESIZE_CURSOR_JSON"

displayd \
  --print-plan \
  --dirty-regions 129 \
  >"$COALESCED_JSON"

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 5 \
  --sample-frames 999 \
  --frame-time-micros 99999 \
  --frame-sample-file "$FRAME_SAMPLE_FILE" \
  >"$FRAME_SAMPLE_JSON"

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 12 \
  --runtime-policy-file "$RUNTIME_POLICY_FILE" \
  >"$RUNTIME_POLICY_JSON"

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 4 \
  --framebuffer-width 1440 \
  --framebuffer-height 900 \
  --scale 2 \
  --window-id 0x01200007 \
  --window-title Terminal \
  --window-x 30 \
  --window-y 40 \
  --window-width 800 \
  --window-height 600 \
  --window-host-width 400 \
  --window-host-height 300 \
  >"$WINDOW_REGION_JSON"

displayd \
  --print-plan \
  --visibility foreground \
  --dirty-regions 4 \
  --framebuffer-width 4 \
  --framebuffer-height 3 \
  --scale 1 \
  --window-id 0x02000010 \
  --window-title Crop \
  --window-x 1 \
  --window-y 1 \
  --window-width 2 \
  --window-height 2 \
  --framebuffer-rgba-file "$FRAMEBUFFER_RGBA_FILE" \
  --window-crop-rgba-file "$WINDOW_CROP_RGBA_FILE" \
  >"$WINDOW_CROP_JSON"

if displayd --print-plan --frame-sample-file "$BAD_FRAME_SAMPLE_FILE" >"$STORE/bad-frame-sample.stdout" 2>"$STORE/bad-frame-sample.stderr"; then
  fail "displayd accepted a zero-duration frame sample"
fi
grep -q "zero duration" "$STORE/bad-frame-sample.stderr" \
  || fail "displayd zero-duration frame sample error was not clear"

if displayd --print-plan --window-id 0x01200007 >"$STORE/bad-window.stdout" 2>"$STORE/bad-window.stderr"; then
  fail "displayd accepted incomplete window-region metadata"
fi
grep -q "requires --window-x" "$STORE/bad-window.stderr" \
  || fail "displayd incomplete window-region error was not clear"

if displayd \
  --print-plan \
  --framebuffer-width 4 \
  --framebuffer-height 3 \
  --window-id 0x02000010 \
  --window-x 1 \
  --window-y 1 \
  --window-width 2 \
  --window-height 2 \
  --framebuffer-rgba-file "$BAD_FRAMEBUFFER_RGBA_FILE" \
  --window-crop-rgba-file "$STORE/bad-window-crop.rgba" \
  >"$STORE/bad-window-crop.stdout" 2>"$STORE/bad-window-crop.stderr"; then
  fail "displayd accepted an RGBA framebuffer with the wrong byte count"
fi
grep -q "has 4 bytes, expected 48" "$STORE/bad-window-crop.stderr" \
  || fail "displayd wrong-sized RGBA framebuffer error was not clear"

python3 - \
  "$FOREGROUND_JSON" \
  "$BACKGROUND_JSON" \
  "$HIDDEN_JSON" \
  "$RESIZE_CURSOR_JSON" \
  "$COALESCED_JSON" \
  "$FRAME_SAMPLE_JSON" \
  "$RUNTIME_POLICY_JSON" \
  "$RUNTIME_POLICY_FILE" \
  "$WINDOW_REGION_JSON" \
  "$WINDOW_CROP_JSON" \
  "$FRAMEBUFFER_RGBA_FILE" \
  "$WINDOW_CROP_RGBA_FILE" <<'PY'
import json
import sys

foreground_path, background_path, hidden_path, resize_cursor_path, coalesced_path, frame_sample_path, runtime_policy_path, runtime_policy_file, window_region_path, window_crop_path, framebuffer_rgba_file, window_crop_rgba_file = sys.argv[1:13]

def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)

def require(condition, message):
    if not condition:
        raise AssertionError(message)

foreground = load(foreground_path)
require(
    foreground["pipeline"]
    == [
        "guest-framebuffer",
        "dirty-region-detection",
        "shared-memory-transport",
        "metal-texture-update",
        "coreanimation-layer",
        "host-cursor-overlay",
        "adaptive-frame-pacing",
    ],
    "foreground pipeline contract mismatch",
)
require(foreground["pacing"]["visibility"] == "foreground", "foreground visibility missing")
require(foreground["pacing"]["max_fps"] == 60, "foreground max fps mismatch")
require(foreground["pacing"]["idle_fps"] == 0, "foreground idle fps mismatch")
require(foreground["pacing"]["repaint_when_idle"] is False, "foreground idle repaint mismatch")
require(
    foreground["pacing"]["rationale"]
    == "foreground productivity VMs can burst to smooth interactive FPS",
    "foreground rationale mismatch",
)
require(foreground["dirty_regions"]["tracked_regions"] == 4, "foreground dirty count mismatch")
require(
    foreground["dirty_regions"]["update_strategy"] == "partial-texture-update",
    "foreground dirty strategy mismatch",
)
require(
    foreground["dirty_regions"]["full_frame_fallback"] is False,
    "foreground should not require full-frame fallback",
)
require(foreground["timing"]["sample_frames"] == 120, "foreground sample count mismatch")
require(
    foreground["timing"]["average_frame_time_micros"] == 16000,
    "foreground average frame time mismatch",
)
require(foreground["timing"]["frame_budget_micros"] == 16666, "foreground budget mismatch")
require(foreground["timing"]["estimated_fps"] == 62, "foreground estimated fps mismatch")
require(foreground["timing"]["within_budget"] is True, "foreground budget status mismatch")
require(foreground["timing"]["source"] == "cli-sample", "foreground timing source mismatch")

background = load(background_path)
require(background["pacing"]["visibility"] == "background", "background visibility missing")
require(background["pacing"]["max_fps"] == 10, "background max fps mismatch")
require(background["pacing"]["idle_fps"] == 0, "background idle fps mismatch")
require(background["pacing"]["repaint_when_idle"] is False, "background idle repaint mismatch")
require(
    background["pacing"]["rationale"] == "background VMs are throttled for battery and idle CPU",
    "background rationale mismatch",
)
require(background["timing"]["frame_budget_micros"] == 100000, "background budget mismatch")
require(background["timing"]["estimated_fps"] == 6, "background estimated fps mismatch")
require(background["timing"]["within_budget"] is False, "background budget status mismatch")

hidden = load(hidden_path)
require(hidden["pacing"]["visibility"] == "hidden", "hidden visibility missing")
require(hidden["pacing"]["max_fps"] == 0, "hidden max fps mismatch")
require(hidden["pacing"]["idle_fps"] == 0, "hidden idle fps mismatch")
require(hidden["pacing"]["repaint_when_idle"] is False, "hidden idle repaint mismatch")
require(hidden["pacing"]["rationale"] == "hidden VMs should not repaint", "hidden rationale mismatch")
require(hidden["timing"]["frame_budget_micros"] is None, "hidden frame budget should be absent")
require(hidden["timing"]["source"] == "metadata-only", "hidden timing source mismatch")

resize_cursor = load(resize_cursor_path)
require(resize_cursor["framebuffer"]["width"] == 1680, "resize width mismatch")
require(resize_cursor["framebuffer"]["height"] == 1050, "resize height mismatch")
require(resize_cursor["framebuffer"]["scale"] == 2, "resize scale mismatch")
require(
    resize_cursor["framebuffer"]["retina_backing_width"] == 3360,
    "resize backing width mismatch",
)
require(
    resize_cursor["framebuffer"]["retina_backing_height"] == 2100,
    "resize backing height mismatch",
)
require(
    resize_cursor["dirty_regions"]["tracked_regions"] == 1,
    "resize should mark at least one dirty region",
)
require(
    resize_cursor["dirty_regions"]["update_strategy"] == "partial-texture-update",
    "resize dirty strategy mismatch",
)
require(
    resize_cursor["cursor"]["position"] == {"x": 1679, "y": 1049},
    "cursor position was not clamped to resized framebuffer",
)
require(resize_cursor["cursor"]["host_cursor_overlay"] is True, "cursor overlay mismatch")
require(
    resize_cursor["cursor"]["render_guest_cursor_in_framebuffer"] is False,
    "guest cursor rendering mismatch",
)
require(
    resize_cursor["input_events"]
    == [
        {
            "type": "resize",
            "width": 1680,
            "height": 1050,
            "scale": 2,
            "backing_width": 3360,
            "backing_height": 2100,
        },
        {
            "type": "cursor-moved",
            "x": 1679,
            "y": 1049,
            "overlay": True,
        },
    ],
    "resize/cursor input event contract mismatch",
)
require(
    resize_cursor["metal"]["texture_updates"] == "deferred-until-dirty",
    "metal texture update contract mismatch",
)
require(
    resize_cursor["metal"]["presentation_layer"] == "coreanimation",
    "metal presentation layer mismatch",
)
require(
    resize_cursor["metal"]["vnc_fallback_allowed"] is False,
    "displayd should not allow VNC fallback",
)

coalesced = load(coalesced_path)
require(coalesced["dirty_regions"]["tracked_regions"] == 129, "coalesced dirty count mismatch")
require(
    coalesced["dirty_regions"]["update_strategy"] == "coalesced-texture-update",
    "coalesced dirty strategy mismatch",
)
require(
    coalesced["dirty_regions"]["full_frame_fallback"] is True,
    "coalesced dirty regions should expose full-frame fallback",
)
require(coalesced["timing"]["source"] == "metadata-only", "coalesced timing source mismatch")
require(coalesced["input_events"] == [], "coalesced plan should not synthesize input events")

frame_sample = load(frame_sample_path)
require(frame_sample["timing"]["sample_frames"] == 3, "frame sample count mismatch")
require(
    frame_sample["timing"]["average_frame_time_micros"] == 17000,
    "frame sample average mismatch",
)
require(frame_sample["timing"]["frame_budget_micros"] == 16666, "frame sample budget mismatch")
require(frame_sample["timing"]["estimated_fps"] == 58, "frame sample estimated fps mismatch")
require(frame_sample["timing"]["within_budget"] is False, "frame sample budget status mismatch")
require(frame_sample["timing"]["source"] == "frame-sample-file", "frame sample source mismatch")

runtime_policy = load(runtime_policy_path)
require(runtime_policy["pacing"]["visibility"] == "background", "runtime policy visibility mismatch")
require(runtime_policy["pacing"]["max_fps"] == 5, "runtime policy FPS cap was not consumed")
require(
    "runtime policy caps display pacing at 5 FPS" in runtime_policy["pacing"]["rationale"],
    "runtime policy rationale did not explain cap",
)
require(runtime_policy["timing"]["frame_budget_micros"] == 200000, "runtime policy frame budget mismatch")
require(
    runtime_policy["runtime_policy"]
    == {
        "path": runtime_policy_file,
        "visibility": "background",
        "display_fps_cap": "5",
        "max_fps_override": 5,
        "source": "runtime-resources",
    },
    "runtime policy plan payload mismatch",
)

window_region = load(window_region_path)
require(
    window_region["window_region"]
    == {
        "window_id": "0x01200007",
        "title": "Terminal",
        "source_rect": {"x": 30, "y": 40, "width": 800, "height": 600},
        "clipped_rect": {"x": 30, "y": 40, "width": 800, "height": 600},
        "host_size": {"width": 400, "height": 300},
        "backing_rect": {"x": 60, "y": 80, "width": 1600, "height": 1200},
        "input_mapping": {
            "coordinate_origin": "guest-framebuffer-top-left",
            "host_width": 400,
            "host_height": 300,
            "guest_x": 30,
            "guest_y": 40,
            "guest_width": 800,
            "guest_height": 600,
            "scale_x_numerator": 800,
            "scale_x_denominator": 400,
            "scale_y_numerator": 600,
            "scale_y_denominator": 300,
        },
        "presentation": "proxy-window-crop",
    },
    "window-region proxy crop contract mismatch",
)

window_crop = load(window_crop_path)
require(
    window_crop["window_region"]
    == {
        "window_id": "0x02000010",
        "title": "Crop",
        "source_rect": {"x": 1, "y": 1, "width": 2, "height": 2},
        "clipped_rect": {"x": 1, "y": 1, "width": 2, "height": 2},
        "host_size": {"width": 2, "height": 2},
        "backing_rect": {"x": 1, "y": 1, "width": 2, "height": 2},
        "input_mapping": {
            "coordinate_origin": "guest-framebuffer-top-left",
            "host_width": 2,
            "host_height": 2,
            "guest_x": 1,
            "guest_y": 1,
            "guest_width": 2,
            "guest_height": 2,
            "scale_x_numerator": 2,
            "scale_x_denominator": 2,
            "scale_y_numerator": 2,
            "scale_y_denominator": 2,
        },
        "presentation": "proxy-window-crop",
    },
    "window crop frame region contract mismatch",
)
crop_frame = window_crop["window_crop_frame"]
for dynamic_key in (
    "source_len_bytes",
    "source_modified_unix_nanos",
    "refreshed_at_unix_nanos",
):
    require(dynamic_key in crop_frame, f"window crop frame missing {dynamic_key}")
require(crop_frame["source_len_bytes"] == 48, "window crop source length mismatch")
require(
    isinstance(crop_frame["source_modified_unix_nanos"], int)
    and crop_frame["source_modified_unix_nanos"] > 0,
    "window crop source modified timestamp missing",
)
require(
    isinstance(crop_frame["refreshed_at_unix_nanos"], int)
    and crop_frame["refreshed_at_unix_nanos"] > 0,
    "window crop refreshed timestamp missing",
)
static_crop_frame = {
    key: value
    for key, value in crop_frame.items()
    if key
    not in {
        "source_len_bytes",
        "source_modified_unix_nanos",
        "refreshed_at_unix_nanos",
    }
}
require(
    static_crop_frame
    == {
        "source_path": framebuffer_rgba_file,
        "output_path": window_crop_rgba_file,
        "pixel_format": "rgba8",
        "framebuffer_width": 4,
        "framebuffer_height": 3,
        "crop_rect": {"x": 1, "y": 1, "width": 2, "height": 2},
        "output_width": 2,
        "output_height": 2,
        "expected_input_bytes": 48,
        "output_bytes": 16,
        "presentation": "proxy-window-crop-frame",
    },
    "window crop frame plan mismatch",
)
with open(window_crop_rgba_file, "rb") as handle:
    crop_bytes = list(handle.read())
require(
    crop_bytes == [1, 1, 5, 255, 2, 1, 6, 255, 1, 2, 9, 255, 2, 2, 10, 255],
    "window crop RGBA bytes mismatch",
)
PY

summary="$(displayd \
  --framebuffer-width 1280 \
  --framebuffer-height 720 \
  --scale 1 \
  --dirty-regions 9)"

case "$summary" in
  *"displayd ready: 1280x720@1x, 60 max fps, 9 dirty region(s)"*) ;;
  *) fail "summary output did not match expected contract: $summary" ;;
esac

PRESERVE_STORE=0
echo "PASS: displayd public CLI plan smoke ($STORE)"
