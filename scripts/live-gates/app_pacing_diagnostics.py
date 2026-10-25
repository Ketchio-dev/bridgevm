#!/usr/bin/env python3
"""Summarise the app's pacing records: what the live display window showed and how ordered input kept up. Read-only.

    app_pacing_diagnostics.py [--json] FILE

FILE is an app-pacing.log the app wrote as a runtime session ended (logs/hvf in the VM bundle, or the T17 journey's
copy of its first run, metadata/product-e2e/first-run-app-pacing.log), or a private T17 or T19 lane result whose
first_run_host_records carry the records (hvf_app_pacing.py defines both). A lane result's `ramfb display timing:`
record, when present, is summarised beside them, so the exporter's publish cadence and what the window presented read
together. The file is read once through a no-follow descriptor (a missing, linked, irregular or oversized file reads
as empty). It refuses records whose counters do not reconcile and prints the records with a summary, or with --json
their fields by prefix.

Exit 0 with reconciled records. Exit 1, with the reason on stderr, when the file holds no app pacing records (a run
from before them, a session that recorded nothing) or a record does not reconcile; 2 on usage. It reports only: no
gate requires the records or reads their values.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys

import hvf_app_pacing as APP
from hvf_app_pacing import DISPLAY_PREFIX, INPUT_PREFIX, PacingError
import hvf_host_display as EXPORTER
from hvf_run_log import read_run_log

LANE_LIMIT = 1 << 20
EXPORTER_NAME = APP.lane_name(EXPORTER.DISPLAY_PREFIX)


def from_lane(value: object) -> dict[str, dict]:
    """The app records, and the exporter's when present, of a lane result's first_run_host_records."""
    records = value.get("first_run_host_records") if isinstance(value, dict) else None
    if not isinstance(records, dict) or not all(APP.lane_name(prefix) in records for prefix in APP.RECORDS):
        raise PacingError("the lane result carries no app pacing records")
    found = {prefix: APP.reconcile(prefix, records[APP.lane_name(prefix)]) for prefix in APP.RECORDS}
    exporter = records.get(EXPORTER_NAME)
    if exporter is not None:
        if (not isinstance(exporter, dict) or set(exporter) != set(EXPORTER.DISPLAY_FIELDS)
                or not all(type(value) is int and 0 <= value < 1 << 64 for value in exporter.values())):
            raise PacingError(f"{EXPORTER.DISPLAY_PREFIX} record is malformed")
        found[EXPORTER.DISPLAY_PREFIX] = {field: exporter[field] for field in EXPORTER.DISPLAY_FIELDS}
    return found


def records(raw: bytes) -> dict[str, dict]:
    if raw.lstrip()[:1] == b"{":
        try:
            return from_lane(json.loads(raw))
        except ValueError as error:
            raise PacingError(str(error) if isinstance(error, PacingError) else "the lane result is not JSON") from None
    if not raw:
        raise PacingError("no app pacing records (missing, linked, irregular or oversized file)")
    return APP.read_file(raw)


def share(part: int, whole: int) -> str:
    return f"{100 * part / whole:.1f}%" if whole else "-"


def spread(names: tuple, counts: list[int]) -> str:
    total = sum(counts)
    return " ".join(f"{name}:{share(count, total)}" for name, count in zip(names, counts)) + f" of {total}"


def summary(found: dict[str, dict]) -> list[str]:
    d, i = found[DISPLAY_PREFIX], found[INPUT_PREFIX]
    period = (f"frame period {d['period_min_us'] / 1000:.3f}-{d['period_max_us'] / 1000:.3f} ms "
              f"({1e6 / d['period_max_us']:.1f}-{1e6 / d['period_min_us']:.1f} Hz)" if d["period_min_us"] else
              "no frame period")
    holds = [d[f"hold_{n}"] for n in range(1, 7)] + [d["hold_gt_6"]]
    ages = [d[field] for field in APP.DISPLAY_FIELDS[19:25]]
    depths = [i[field] for field in APP.INPUT_FIELDS[7:12]]
    rtts = [i[field] for field in APP.INPUT_FIELDS[13:19]]
    lines = [
        f"display: {d['ticks']} ticks, {period}; {d['missed_vsyncs']} vsyncs missed, longest tick gap "
        f"{d['max_tick_gap_ms']} ms",
        f"display frames: new {d['presented']} ({share(d['presented'], d['ticks'])} of ticks), repeated {d['repeated']}, "
        f"busy {d['busy']}, unmapped {d['unmapped']}, iosurface {d['iosurface']}; first {d['first_frames']}, "
        f"never shown {d['skipped']}",
        "display holds in vsyncs: " + spread(("1", "2", "3", "4", "5", "6", ">6"), holds),
        "display content age at its target vsync: " + spread(("<=8ms", "<=17ms", "<=33ms", "<=50ms", "<=100ms", ">100ms"), ages)
        + f", unknown {d['age_unknown']}, oldest {d['max_age_us'] / 1000:.3f} ms",
        f"main thread: {d['stalls_gt_50ms']} stalls over 50 ms, longest stretch {d['max_stall_ms']} ms; "
        f"longest display tick {d['max_work_us'] / 1000:.3f} ms",
        f"ordered input: {i['queued']} queued ({i['coalesced']} coalesced), {i['legacy']} legacy, {i['refused']} refused; "
        f"{i['sent']} sent, {i['inserted']} inserted, {i['discarded']} discarded",
        "ordered input depth: " + spread(("1", "2", "<=4", "<=16", ">16"), depths) + f", deepest {i['max_depth']}",
        "ordered input round trip: " + spread(("<=10ms", "<=20ms", "<=50ms", "<=100ms", "<=250ms", ">250ms"), rtts)
        + f", longest {i['max_rtt_ms']} ms",
    ]
    exporter = found.get(EXPORTER.DISPLAY_PREFIX)
    if exporter is not None:
        # The exporter counts its whole boot; the app counts only while its display window was open.
        intervals = [exporter[field] for field in EXPORTER.DISPLAY_FIELDS[8:12]]
        lines.append(f"exporter (whole boot): {exporter['ticks']} ticks, {share(exporter['late_le_1ms'], exporter['ticks'])}"
                     f" woke within 1 ms; {exporter['published']} published, intervals "
                     + spread(("<=20ms", "<=40ms", "<=70ms", ">70ms"), intervals))
    return lines


def main(argv: list[str]) -> int:
    as_json = argv[:1] == ["--json"]
    if len(argv) != 1 + as_json or argv[-1].startswith("-"):
        print("usage: app_pacing_diagnostics.py [--json] FILE", file=sys.stderr)
        return 2
    try:
        found = records(read_run_log(Path(argv[-1]), LANE_LIMIT))
    except PacingError as error:
        print(f"app pacing diagnostics: {error}", file=sys.stderr)
        return 1
    if as_json:
        print(json.dumps(found))
    else:
        printed = [APP.record(prefix, values) for prefix, values in found.items() if prefix in APP.RECORDS]
        print("\n".join(printed + summary(found)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
