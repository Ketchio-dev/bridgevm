#!/usr/bin/env python3
"""Report the HDA diagnostic records of a run.log. Read-only.

    hda_diagnostics.py [--json] RUN_LOG

As the platform drops the HDA controller, a controller whose CoreAudio sink
installed host records prints `hda poll:`, `hda stream:` and `hda repeat:`
(crates/bridgevm-hvf/src/hda/diagnostics.rs and repeat_detector.rs define each
field) and the sink's `hda CoreAudio timing:` (hda_coreaudio_timing.rs). This
reads the whole RUN_LOG once the helper has exited, as hvf_terminal_evidence does
(a missing, linked, irregular or oversized log reads as empty), and takes the
records only from the host tail after the bound final report's footer, where
hvf_host_tail admits each at most once (hvf_host_diagnostics.py), so a guest
copy in agent output or the serial tail is never read. It refuses a record whose
counters do not reconcile and prints the records, or with --json their fields;
millisecond buffer lengths with three decimals stay strings.

Exit 0 with at least one reconciled record. Exit 1, with the reason on stderr,
when no final report binds, the host tail has no record (a log from before the
records existed) or a record does not reconcile; 2 on usage. It reports only: no
gate requires the records or reads their values.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from hvf_host_diagnostics import FIELD_VALUES, GRAMMARS, MILLIS, RECORDS
from hvf_run_log import read_run_log
from hvf_terminal_evidence import host_tail
from hvf_terminal_report import LOG_LIMIT

# Exclusive lower and inclusive upper bucket bounds in ms; truncation keeps the longest in its bucket.
GAP_BOUNDS = ((0, 10), (10, 20), (20, 50), (50, 100), (100, None))
INTERVAL_BOUNDS = ((0, 10), (10, 15), (15, 20), (20, 30), (30, 50), (50, None))


class DiagnosticsError(ValueError):
    """No reconciled diagnostic record can be reported."""


def thousandths(millis: str) -> int:
    """A `*_ms` buffer length, which always carries three decimals, exactly."""
    return int(millis.replace(".", ""))


def longest_fits(counts: list[int], longest: int, bounds: tuple) -> bool:
    """The longest gap or interval lies in the highest non-empty bucket, and is 0 with none."""
    filled = [index for index, count in enumerate(counts) if count]
    if not filled:
        return longest == 0
    low, high = bounds[filled[-1]]
    return low <= longest and (high is None or longest <= high)


def checks(prefix: str, v: dict) -> tuple[tuple[bool, str], ...]:
    """Whether each counter agrees with the others, as the host derives them."""
    if prefix == "hda poll:":
        gaps = [v[field] for field in RECORDS[prefix][1:6]]
        return ((sum(gaps) == v["polls"], "gap buckets do not sum to polls"),
                (v["capped_polls"] == v["gap_gt_100ms"], "capped polls are not the gaps over 100 ms"),
                (v["capped_polls"] > 0 or v["capped_lost_ms"] == 0, "time was lost without a capped poll"),
                (longest_fits(gaps, v["max_gap_ms"], GAP_BOUNDS), "the longest gap is outside its bucket"),
                ((v["ioc_completions"] == 0) == (v["max_iocs_per_poll"] == 0)
                 and v["max_iocs_per_poll"] <= v["ioc_completions"], "IOCs per poll do not fit the IOC count"))
    if prefix == "hda stream:":
        shape = [v[field] for field in RECORDS[prefix][1:13]]
        return ((v["runs"] > 0 or all(value in (0, "0.000") for value in shape), "a stream shape without a run"),
                (v["ioc_entries"] <= v["entries"], "more IOC entries than entries"),
                (v["entry_bytes_min"] <= v["entry_bytes_max"], "entry bytes min exceeds max"),
                (thousandths(v["entry_ms_min"]) <= thousandths(v["entry_ms_max"]), "entry ms min exceeds max"))
    if prefix == "hda repeat:":
        return ((v["repeat_passes"] <= v["compared_passes"], "more repeats than compared passes"),
                (v["compared_passes"] + v["silent_passes"] <= v["passes"], "compared and silent passes exceed passes"),
                (v["max_repeat_run"] <= v["repeat_passes"] and (v["max_repeat_run"] == 0) == (v["repeat_passes"] == 0),
                 "the longest repeat run does not fit the repeats"))
    intervals = [v[field] for field in RECORDS[prefix][1:7]]
    ring = (v["ring_min_ms"], v["ring_final_ms"], v["ring_max_ms"])
    return ((sum(intervals) == max(v["callbacks"] - 1, 0), "interval buckets do not count every later callback"),
            (v["late_callbacks"] == v["interval_le_50ms"] + v["interval_gt_50ms"],
             "late callbacks are not the intervals over 30 ms"),
            (longest_fits(intervals, v["max_interval_ms"], INTERVAL_BOUNDS), "the longest interval is outside its bucket"),
            (v["trylock_misses"] <= v["callbacks"], "more try_lock misses than callbacks"),
            (ring[0] <= ring[1] <= ring[2] if v["ring_samples"] else ring == (0, 0, 0),
             "ring levels do not order as min, final, max"))


def parse(prefix: str, line: str) -> dict[str, int | str]:
    """One record's fields, refused unless every counter reconciles with the others."""
    if GRAMMARS[prefix].fullmatch(line) is None:
        raise DiagnosticsError(f"{prefix} record is malformed")
    raw = dict(token.split("=") for token in line[len(prefix) + 1:].split(" "))
    v = {field: raw[field] if FIELD_VALUES.get(field) == MILLIS else int(raw[field]) for field in RECORDS[prefix]}
    for holds, reason in checks(prefix, v):
        if not holds:
            raise DiagnosticsError(f"{prefix} {reason}")
    return v


def diagnostics(raw: bytes) -> dict[str, dict[str, int | str]]:
    """The reconciled records in the host tail of the whole run.log `raw`, by prefix, in print order."""
    tail = host_tail(raw)
    if tail is None:
        raise DiagnosticsError("no final host report binds the log")
    found = {prefix: parse(prefix, records[0]) for prefix in RECORDS
             if (records := [line for line in tail if line.startswith(prefix)])}
    if not found:
        raise DiagnosticsError("the host tail has no HDA diagnostic record")
    return found


def record(prefix: str, values: dict) -> str:
    return prefix + "".join(f" {field}={values[field]}" for field in RECORDS[prefix])


def main(argv: list[str]) -> int:
    as_json = argv[:1] == ["--json"]
    if len(argv) != 1 + as_json or argv[-1].startswith("-"):
        print("usage: hda_diagnostics.py [--json] RUN_LOG", file=sys.stderr)
        return 2
    try:
        found = diagnostics(read_run_log(Path(argv[-1]), LOG_LIMIT - 1))
    except DiagnosticsError as error:
        print(f"hda diagnostics: {error}", file=sys.stderr)
        return 1
    print(json.dumps(found) if as_json else "\n".join(record(prefix, values) for prefix, values in found.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
