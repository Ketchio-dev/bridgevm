"""The app's own pacing records: what its live display window assigned to its layer and how its ordered input route kept up.

As a runtime session it observed ends, the app (apps/macos/Sources/BridgeVMControl/HvfEngine/HvfAppPacingRecords.swift)
replaces app-pacing.log in that session's evidence directory with two lines, `app display pacing:`
(HvfDisplayPacingRecord.swift) and `app ordered input:` (HvfOrderedInputRecord.swift), whose fields are unsigned
decimals without leading zeros. The display record counts display-link ticks by outcome, the display's frame period,
skipped vsyncs, each new frame's hold in vsyncs and its content age from the BVFB publish time to the display link's
target vsync (actual presentation and missed Core Animation deadlines are unobserved), frames that
never reached the layer, the longest tick and the main thread's stalls over 50 ms. The input record counts
admissions, queue depth, coalesced pointer moves and send-to-receipt round trips. Buckets hold a value at most their
bound in the unit printed. The T17 journey keeps its first run's file, whose records join the private lane result
under first_run_host_records (product_e2e_host_records.py); app_pacing_diagnostics.py summarises either. Nothing
requires the records or reads their values to decide anything.
"""

from __future__ import annotations

import re

VALUE = "(0|[1-9][0-9]{0,19})"
DISPLAY_PREFIX = "app display pacing:"
DISPLAY_FIELDS = ("ticks", "presented", "repeated", "busy", "unmapped", "iosurface", "first_frames", "skipped",
                  "period_min_us", "period_max_us", "missed_vsyncs", "max_tick_gap_ms",
                  "hold_1", "hold_2", "hold_3", "hold_4", "hold_5", "hold_6", "hold_gt_6",
                  "age_le_8ms", "age_le_17ms", "age_le_33ms", "age_le_50ms", "age_le_100ms", "age_gt_100ms",
                  "age_unknown", "max_age_us", "max_work_us", "stalls_gt_50ms", "max_stall_ms")
INPUT_PREFIX = "app ordered input:"
INPUT_FIELDS = ("queued", "coalesced", "legacy", "refused", "sent", "inserted", "discarded",
                "depth_1", "depth_2", "depth_le_4", "depth_le_16", "depth_gt_16", "max_depth",
                "rtt_le_10ms", "rtt_le_20ms", "rtt_le_50ms", "rtt_le_100ms", "rtt_le_250ms", "rtt_gt_250ms",
                "max_rtt_ms")
RECORDS = {DISPLAY_PREFIX: DISPLAY_FIELDS, INPUT_PREFIX: INPUT_FIELDS}
GRAMMARS = {prefix: re.compile(re.escape(prefix) + "".join(f" {field}={VALUE}" for field in fields))
            for prefix, fields in RECORDS.items()}
# Bucket bounds as the app counts them: a value at most its bound, the last bucket takes the rest.
AGE_BOUNDS_US = (8_000, 17_000, 33_000, 50_000, 100_000)
DEPTH_BOUNDS = (1, 2, 4, 16)
RTT_BOUNDS_MS = (10, 20, 50, 100, 250)
STALL_MS = 50
FILE_LIMIT = 64 * 1024


class PacingError(ValueError):
    """No reconciled app pacing record can be reported."""


def lane_name(prefix: str) -> str:
    """`app display pacing:` as app_display_pacing, the name the lane result uses."""
    return prefix[:-1].lower().replace(" ", "_")


def fits(counts: list[int], longest: int, bounds: tuple) -> bool:
    """The longest value lies in the highest non-empty bucket (lower bound exclusive), and is 0 with none."""
    filled = [index for index, count in enumerate(counts) if count]
    if not filled:
        return longest == 0
    low = 0 if filled[-1] == 0 else bounds[filled[-1] - 1]
    high = bounds[filled[-1]] if filled[-1] < len(bounds) else None
    return (low < longest or (filled[-1] == 0 and longest == 0)) and (high is None or longest <= high)


def checks(prefix: str, v: dict) -> tuple[tuple[bool, str], ...]:
    """Whether each counter agrees with the others, as the app derives them."""
    if prefix == DISPLAY_PREFIX:
        holds = [v[f"hold_{n}"] for n in range(1, 7)] + [v["hold_gt_6"]]
        ages = [v[field] for field in DISPLAY_FIELDS[19:25]]
        runs = v["presented"] - v["first_frames"]
        return ((v["ticks"] == sum(v[field] for field in ("presented", "repeated", "busy", "unmapped", "iosurface")),
                 "tick outcomes do not sum to ticks"),
                (v["first_frames"] <= v["presented"], "more first frames than new frames"),
                (sum(holds) == runs, "holds do not count every new frame after a first frame"),
                (sum(ages) + v["age_unknown"] == runs, "ages do not count every new frame after a first frame"),
                (fits(ages, v["max_age_us"], AGE_BOUNDS_US), "the oldest frame is outside its age bucket"),
                (v["period_min_us"] <= v["period_max_us"] and (v["period_min_us"] == 0) == (v["period_max_us"] == 0),
                 "the frame period range is not ordered"),
                (v["missed_vsyncs"] == 0 or v["max_tick_gap_ms"] > 0, "vsyncs were missed without a tick gap"),
                ((v["stalls_gt_50ms"] == 0) == (v["max_stall_ms"] < STALL_MS)
                 or v["max_stall_ms"] == STALL_MS, "stalls do not fit the longest main-thread stretch"))
    depths = [v[field] for field in INPUT_FIELDS[7:12]]
    rtts = [v[field] for field in INPUT_FIELDS[13:19]]
    return ((sum(depths) == v["queued"], "depths do not count every queued event"),
            (v["coalesced"] <= v["queued"], "more coalesced than queued events"),
            (fits(depths, v["max_depth"], DEPTH_BOUNDS), "the deepest queue is outside its depth bucket"),
            (v["inserted"] <= v["sent"], "more inserted than sent events"),
            (sum(rtts) == v["inserted"], "round trips do not count every inserted event"),
            (fits(rtts, v["max_rtt_ms"], RTT_BOUNDS_MS), "the longest round trip is outside its bucket"))


def reconcile(prefix: str, v: object) -> dict:
    """`v` when every field is an unsigned 64-bit integer and every counter agrees, else PacingError."""
    if (not isinstance(v, dict) or set(v) != set(RECORDS[prefix])
            or not all(type(value) is int and 0 <= value < 1 << 64 for value in v.values())):
        raise PacingError(f"{prefix} record is malformed")
    for holds, reason in checks(prefix, v):
        if not holds:
            raise PacingError(f"{prefix} {reason}")
    return {field: v[field] for field in RECORDS[prefix]}


def parse(prefix: str, line: str) -> dict:
    """One printed record's fields, refused unless it is well formed and reconciles."""
    if GRAMMARS[prefix].fullmatch(line) is None:
        raise PacingError(f"{prefix} record is malformed")
    raw = dict(token.split("=") for token in line[len(prefix) + 1:].split(" "))
    return reconcile(prefix, {field: int(raw[field]) for field in RECORDS[prefix]})


def read_file(raw: bytes) -> dict[str, dict]:
    """The records of an app-pacing.log: exactly the two lines the app writes, in order."""
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise PacingError("the file is not the app's pacing records") from None
    lines = text.split("\n")
    if len(lines) != 3 or lines[2] or not all(line.startswith(prefix + " ") for line, prefix in zip(lines, RECORDS)):
        raise PacingError("the file is not the app's pacing records")
    return {prefix: parse(prefix, line) for prefix, line in zip(RECORDS, lines)}


def record(prefix: str, values: dict) -> str:
    return prefix + "".join(f" {field}={values[field]}" for field in RECORDS[prefix])
