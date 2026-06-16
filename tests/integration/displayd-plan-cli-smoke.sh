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

python3 - \
  "$FOREGROUND_JSON" \
  "$BACKGROUND_JSON" \
  "$HIDDEN_JSON" \
  "$RESIZE_CURSOR_JSON" \
  "$COALESCED_JSON" <<'PY'
import json
import sys

foreground_path, background_path, hidden_path, resize_cursor_path, coalesced_path = sys.argv[1:6]

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
