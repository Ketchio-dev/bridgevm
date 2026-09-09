#!/usr/bin/env python3
"""Time a B6 scene from a content-changing keystroke to the presented frame.

B6 asks for "frame time within 10% of baseline". Measured against the declared
static scenes that clause reports nothing: DWM composes when something changes,
and a Notepad window that has finished being typed into does not change. Two
live boots on 2026-09-09 recorded 1, 3 and 2 PresentMon rows and then zero rows
at all -- not a thin sample, an absent one.

What a static text scene can be asked is how long it takes to show a change.
This settles the surface, records its seed, writes one keystroke itself so the
clock starts before the input rather than after it, and times the seed advance
that follows. Same mechanism the capture tool already trusts to prove a frame
is fresh, with the elapsed time kept instead of discarded.

The first version of this tool was clocked by the text caret. Classic Notepad's
system caret blinks on about a half-second period and each blink repaints the
surface, so the "next seed advance" after a keystroke was the next blink as
often as the glyph: nine samples spread from 6 ms to 237 ms, six never saw the
surface hold still. bv-b6-caret-still.ps1 stops the caret at the source; this
tool now also measures the ambient repaint cadence before it arms, records it,
and refuses to report a latency when the surface was still repainting on its
own -- a number taken through that noise is not a measurement of the glyph.

It measures; it declares nothing. The threshold this feeds is a separate
decision and has to be recorded before the cells that it judges.
"""
from __future__ import annotations
import argparse
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, os.environ.get("BRIDGEVM_SCRIPTS_DIR", str(Path(__file__).resolve().parent)))
from iosurface_capture import lookup, seed  # noqa: E402


def ambient(ref, window_ms):
    """Count seed changes over window_ms and estimate their period."""
    deadline = time.monotonic_ns() + window_ms * 1_000_000
    current, changes, stamps = seed(ref), 0, []
    while time.monotonic_ns() <= deadline:
        observed = seed(ref)
        if observed != current:
            current, changes = observed, changes + 1
            stamps.append(time.monotonic_ns())
        time.sleep(0.002)
    period = None
    if len(stamps) >= 2:
        period = round((stamps[-1] - stamps[0]) / (len(stamps) - 1) / 1e6, 1)
    return changes, period


def settle(ref, settle_ms, timeout_ms):
    """Wait until the surface has held one seed for settle_ms."""
    deadline = time.monotonic_ns() + timeout_ms * 1_000_000
    held, current = time.monotonic_ns(), seed(ref)
    while time.monotonic_ns() <= deadline:
        observed = seed(ref)
        if observed != current:
            current, held = observed, time.monotonic_ns()
        elif time.monotonic_ns() - held >= settle_ms * 1_000_000:
            return current
        time.sleep(0.002)
    return None


def measure(iosurface, input_control, key_hex, settle_ms, timeout_ms, ambient_ms):
    ref, ident, width, height = lookup(iosurface)
    changes, period = ambient(ref, ambient_ms)
    record = {"iosurface_id": ident, "width": width, "height": height,
              "ambient_window_ms": ambient_ms, "ambient_changes": changes,
              "ambient_period_ms": period if period is not None else "none"}
    if changes:
        # The surface is repainting with nothing being typed. Whatever is doing
        # that will also stop the clock; a latency read here is not the glyph's.
        record["result"] = "ambient-repaint"
        return record
    settled = settle(ref, settle_ms, timeout_ms)
    if settled is None:
        record["result"] = "never-settled"
        return record
    started = time.monotonic_ns()
    with input_control.open("a", buffering=1) as ctl:
        ctl.write(f"KEY text-hex:{key_hex}\n")
    deadline = started + timeout_ms * 1_000_000
    while time.monotonic_ns() <= deadline:
        observed = seed(ref)
        if observed != settled:
            record.update({"result": "presented", "settled_seed": settled,
                           "presented_seed": observed,
                           "latency_ms": round((time.monotonic_ns() - started) / 1e6, 3)})
            return record
        time.sleep(0.001)
    record.update({"result": "no-present", "settled_seed": settled, "timeout_ms": timeout_ms})
    return record


def write_env(out, record):
    out.mkdir(parents=True, exist_ok=True)
    (out / "present-latency.env").write_text(
        "".join(f"{k}={v}\n" for k, v in record.items()), encoding="ascii")


def self_test():
    import tempfile

    class FakeSurface:
        def __init__(self, flips):
            self.value, self.flips, self.reads = 0, flips, 0

        def read(self):
            self.reads += 1
            if self.reads in self.flips:
                self.value += 1
            return self.value

    global lookup, seed
    real_lookup, real_seed = lookup, seed
    try:
        # ambient window ~5ms at 2ms sleeps = ~3 reads; settle ~5ms; then keys.
        cases = (
            (set(range(1, 400)), "ambient-repaint"),   # never quiet
            ({2}, "ambient-repaint"),                   # one blink during ambient
            ({40}, "presented"),                        # quiet, then the glyph
            (set(), "no-present"),                      # quiet forever
        )
        for flips, expected in cases:
            surface = FakeSurface(flips)
            lookup = lambda path: (surface, 7, 1600, 900)  # noqa: E731
            seed = lambda ref: ref.read()  # noqa: E731
            with tempfile.TemporaryDirectory() as root:
                control = Path(root) / "input.ctl"
                control.write_text("")
                record = measure(Path(root) / "fb.iosurface", control, "41", 5, 400, 5)
                assert record["result"] == expected, (expected, record)
                if expected == "presented":
                    assert control.read_text() == "KEY text-hex:41\n"
                    assert record["latency_ms"] >= 0 and record["ambient_changes"] == 0
                if expected == "ambient-repaint":
                    assert control.read_text() == "", "must not type into a noisy surface"
                    assert record["ambient_changes"] >= 1
                write_env(Path(root) / "out", record)
                assert (Path(root) / "out" / "present-latency.env").exists()
    finally:
        lookup, seed = real_lookup, real_seed
    print("glyph present latency self-test: PASS")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--iosurface", type=Path)
    parser.add_argument("--input-control", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--key-hex", default="42")
    parser.add_argument("--settle-ms", type=int, default=750)
    parser.add_argument("--timeout-ms", type=int, default=5000)
    parser.add_argument("--ambient-ms", type=int, default=1500)
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not (args.iosurface and args.input_control and args.out):
        parser.error("--iosurface, --input-control and --out are required")
    record = measure(args.iosurface, args.input_control, args.key_hex,
                     args.settle_ms, args.timeout_ms, args.ambient_ms)
    write_env(args.out, record)
    print(" ".join(f"{k}={v}" for k, v in record.items()))
    return 0 if record["result"] == "presented" else 1


if __name__ == "__main__":
    raise SystemExit(main())
