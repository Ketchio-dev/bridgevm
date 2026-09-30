#!/usr/bin/env python3
"""Report the host CoreAudio continuity record of a run.log. Read-only.

    hda_continuity.py [--json] RUN_LOG

The CoreAudio sink prints `hda CoreAudio continuity:` after its stats record at
teardown; hda_coreaudio_continuity.rs defines each field. This reads the whole
RUN_LOG once the helper has exited, as hvf_terminal_evidence does (a missing,
linked, irregular or oversized log reads as empty), takes the record only from
the host tail after the bound final report's footer, where hvf_host_tail admits
at most one after a stats record, and refuses one whose counters do not
reconcile. It prints the record, or with --json its fields.

Exit 0 with a reconciled record. Exit 1, with the reason on stderr, when no
final report binds, the host tail has no record (a log from before the record
existed) or the record does not reconcile; 2 on usage. It reports only: no gate
requires the record or reads its values.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from hvf_host_tail import CONTINUITY, CONTINUITY_FIELDS, CONTINUITY_PREFIX
from hvf_run_log import read_run_log
from hvf_terminal_evidence import host_tail
from hvf_terminal_report import LOG_LIMIT


class ContinuityError(ValueError):
    """No reconciled continuity record can be reported."""


def parse(line: str) -> dict[str, int]:
    """The record's fields, refused unless every counter reconciles with the others."""
    match = CONTINUITY.fullmatch(line)
    if match is None:
        raise ContinuityError("continuity record is malformed")
    v = dict(zip(CONTINUITY_FIELDS, map(int, match.groups())))
    frames = v["callback_frames"]
    for holds, reason in (
            (frames > 0, "callback_frames is zero"),
            (v["contention_callbacks"] <= v["underrun_callbacks"] <= v["active_callbacks"],
             "contention, underrun and active callbacks do not nest"),
            (v["gaps"] <= v["underrun_callbacks"] <= v["underrun_frames"],
             "each gap needs an underrun callback and each underrun callback a silent frame"),
            (len({v[key] == 0 for key in ("underrun_callbacks", "underrun_frames", "gaps", "max_gap_frames")}) == 1,
             "underrun callbacks, frames and gaps are not all zero or all nonzero"),
            (v["contention_callbacks"] * frames <= v["underrun_frames"] <= v["underrun_callbacks"] * frames,
             "underrun frames do not fit their callbacks' buffers"),
            (v["max_gap_frames"] <= v["underrun_frames"] <= v["gaps"] * v["max_gap_frames"],
             "the longest gap does not reconcile with the gap count and underrun frames")):
        if not holds:
            raise ContinuityError(reason)
    return v


def continuity(raw: bytes) -> dict[str, int]:
    """The reconciled record in the host tail of the whole run.log `raw`."""
    tail = host_tail(raw)
    if tail is None:
        raise ContinuityError("no final host report binds the log")
    records = [line for line in tail if line.startswith(CONTINUITY_PREFIX)]
    if not records:
        raise ContinuityError("the host tail has no continuity record")
    return parse(records[0])


def record(values: dict[str, int]) -> str:
    return CONTINUITY_PREFIX + "".join(f" {field}={values[field]}" for field in CONTINUITY_FIELDS)


def main(argv: list[str]) -> int:
    as_json = argv[:1] == ["--json"]
    if len(argv) != 1 + as_json or argv[-1].startswith("-"):
        print("usage: hda_continuity.py [--json] RUN_LOG", file=sys.stderr)
        return 2
    try:
        values = continuity(read_run_log(Path(argv[-1]), LOG_LIMIT - 1))
    except ContinuityError as error:
        print(f"hda continuity: {error}", file=sys.stderr)
        return 1
    print(json.dumps(values) if as_json else record(values))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
