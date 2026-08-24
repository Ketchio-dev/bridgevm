#!/usr/bin/env python3
"""Inject B4 press/release and timestamp the real CGL IOSurface change."""
import argparse, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from pointer_iosurface import digest, frame, seed as surface_seed, surface, white_pixels
parser = argparse.ArgumentParser()
for option in ("iosurface", "input-control", "out"):
    parser.add_argument(f"--{option}", required=True, type=Path)
for option, default in (("timeout-ms", 1000), ("hold-ms", 200)): parser.add_argument(f"--{option}", type=int, default=default)
for option in ("hid-x", "hid-y"): parser.add_argument(f"--{option}", required=True, type=int)
args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
ref, ident = surface(args.iosurface); baseline = ""; deadline = time.monotonic_ns() + 120_000_000_000
samples = settled = fill = peak = 0
while time.monotonic_ns() <= deadline:
    before, _, _ = frame(ref); candidate = digest(before); fill = white_pixels(before)
    samples += 1; peak = max(peak, fill); settled = settled + 1 if candidate == baseline else 0
    if fill >= 10_000 and candidate == baseline: break
    baseline = candidate; time.sleep(0.025)
else:
    # A refusal with no record cannot be diagnosed later: "never painted" and
    # "never held still" both raise here. Keep the counts that separate them.
    _, last_frame, _ = frame(ref, True); (args.out / "iosurface-unsettled.xrgb8888").write_bytes(last_frame)
    (args.out / "baseline.env").write_text(f"result=baseline-not-presented\niosurface_id={ident}\nsamples={samples}\nfinal_white_px={fill}\npeak_white_px={peak}\nfinal_settled_samples={settled}\n", encoding="ascii")
    raise RuntimeError(f"baseline not presented white_px={fill} peak={peak} settled={settled} samples={samples}")
before, before_frame, seed = frame(ref, True); baseline = digest(before); (args.out / "iosurface-before.xrgb8888").write_bytes(before_frame)
started = time.monotonic_ns(); start_unix = time.time_ns()
with args.input_control.open("a", buffering=1) as ctl: ctl.write(f"POINTER click:{args.hid_x}x{args.hid_y}\n")
press_unix = time.time_ns()
changed = "none"; first = "none"; changed_frame = b""; change_unix = 0; deadline = started + args.timeout_ms * 1_000_000
while time.monotonic_ns() <= deadline:
    observed_seed = surface_seed(ref)
    if observed_seed != seed:
        region, _, seed = frame(ref); candidate = digest(region)
        if candidate != baseline:
            first = str((time.monotonic_ns() - started + 999_999) // 1_000_000); change_unix = time.time_ns()
            changed = candidate; _, changed_frame, _ = frame(ref, True); break
    time.sleep(0.001)
if changed_frame: (args.out / "iosurface-changed.xrgb8888").write_bytes(changed_frame)
(args.out / "visible.env").write_text(f"source=active-cgl-iosurface\niosurface_id={ident}\nwidth=1600\nheight=900\nhid_x={args.hid_x}\nhid_y={args.hid_y}\nstart_unix_ns={start_unix}\npress_unix_ns={press_unix}\nchange_unix_ns={change_unix}\nbaseline_region_sha256={baseline}\nchanged_region_sha256={changed}\nfirst_changed_ms={first}\n", encoding="ascii")
raise SystemExit(0 if first != "none" else 1)
