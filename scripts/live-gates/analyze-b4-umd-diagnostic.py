#!/usr/bin/env python3
"""Correlate bounded guest UMD diagnostics with the host virtio-gpu trace."""

from __future__ import annotations

import argparse
import json
import os
import re
import stat
import tempfile
from pathlib import Path

from b4_host_resource_lifecycle import classify_trace

GROW = re.compile(r"BV-VIRGL-ALLOC-LIST-GROW-FAIL alloc_count=(\d+) max_alloc=(\d+) res_id=(\d+)")
SUBMIT = re.compile(
    r"BV-VIRGL-SUBMIT stage=(\S+) cdw=(\d+) driver_length=(\d+) command_length=(\d+) "
    r"allocations=(\d+) max_alloc=(\d+) d3d_list_size=(\d+)"
)
MAX_INPUT_BYTES = 32 * 1024 * 1024


def bounded_bytes(path: Path) -> bytes:
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_INPUT_BYTES:
            raise ValueError(f"unsafe or oversized diagnostic input: {path.name}")
        data = bytearray()
        while block := os.read(descriptor, min(1024 * 1024, MAX_INPUT_BYTES + 1 - len(data))):
            data.extend(block)
            if len(data) > MAX_INPUT_BYTES:
                raise ValueError(f"diagnostic input exceeds {MAX_INPUT_BYTES} bytes: {path.name}")
        return bytes(data)
    finally:
        os.close(descriptor)


def bounded_text(path: Path) -> str:
    return bounded_bytes(path).decode("utf-8", errors="replace")


def analyze(
    dbwin_path: Path, trace_path: Path, skip_dbwin_lines: int = 0, skip_trace_lines: int = 0
) -> dict[str, object]:
    if skip_dbwin_lines < 0 or skip_trace_lines < 0:
        raise ValueError("diagnostic window offsets must be nonnegative")
    dbwin_text = "\n".join(bounded_text(dbwin_path).splitlines()[skip_dbwin_lines:])
    grow_events = [
        {
            "alloc_count": int(match.group(1)),
            "max_alloc": int(match.group(2)),
            "resource_id": int(match.group(3)),
        }
        for match in GROW.finditer(dbwin_text)
    ]
    submit_events = [
        {
            "stage": match.group(1),
            "cdw": int(match.group(2)),
            "driver_length": int(match.group(3)),
            "command_length": int(match.group(4)),
            "allocations": int(match.group(5)),
            "max_alloc": int(match.group(6)),
            "d3d_list_size": int(match.group(7)),
        }
        for match in SUBMIT.finditer(dbwin_text)
    ]
    trace_lines = bounded_text(trace_path).splitlines()
    candidates, missing_create = classify_trace(trace_lines, skip_trace_lines)
    first_grow = grow_events[0] if grow_events else None
    first_host = candidates[0] if candidates else None
    correlated = bool(
        first_grow
        and first_host
        and first_grow["resource_id"] == first_host["resource_id"]
    )
    if correlated:
        outcome = "correlated"
    elif not first_grow and not first_host:
        outcome = "nonreproduction"
    elif not first_grow:
        outcome = "host-only"
    elif not first_host:
        outcome = "guest-only"
    else:
        outcome = "resource-mismatch"
    if missing_create:
        if correlated:
            outcome = "correlated-plus-missing-create"
        elif first_grow or first_host:
            outcome = "mixed-with-missing-create"
        else:
            outcome = "missing-create-attach"
    return {
        "schema_version": 2,
        "outcome": outcome,
        "correlated": correlated,
        "dbwin_grow_fail_count": len(grow_events),
        "dbwin_submit_count": len(submit_events),
        "max_submit_allocations": max((event["allocations"] for event in submit_events), default=0),
        "max_submit_capacity": max((event["max_alloc"] for event in submit_events), default=0),
        "max_d3d_list_size": max((event["d3d_list_size"] for event in submit_events), default=0),
        "first_submit": submit_events[0] if submit_events else None,
        "host_never_backed_transfer_count": len(candidates),
        "host_missing_create_attach_count": len(missing_create),
        "first_grow_fail": first_grow,
        "first_never_backed_transfer": first_host,
        "first_missing_create_attach": missing_create[0] if missing_create else None,
    }


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        dbwin = root / "dbwin.log"
        trace = root / "trace.jsonl"
        dbwin.write_text(
            "[dbwin] pid=8 BV-VIRGL-ALLOC-LIST-GROW-FAIL "
            "alloc_count=256 max_alloc=256 res_id=155\n"
            "[dbwin] pid=8 BV-VIRGL-SUBMIT stage=before-render cdw=14 "
            "driver_length=0 command_length=72 allocations=256 max_alloc=512 "
            "d3d_list_size=1024 status=0x00000000\n",
            encoding="utf-8",
        )
        trace.write_text(
            json.dumps(
                {
                    "seq": 1641,
                    "name": "SUBMIT_3D",
                    "ctx_id": 7,
                    "response_name": "ERR_UNSPEC",
                    "renderer_command_id": 43,
                    "renderer_resource_id": 155,
                    "renderer_resource_found": True,
                    "renderer_resource_backed": False,
                }
            )
            + "\n",
            encoding="utf-8",
        )
        result = analyze(dbwin, trace)
        if result["outcome"] != "correlated" or result["correlated"] is not True:
            raise AssertionError("matching resource ids did not correlate")
        if result["dbwin_submit_count"] != 1 or result["max_submit_allocations"] != 256:
            raise AssertionError("bounded submit cardinalities were not retained")
        dbwin.write_text("ignored\n" + dbwin.read_text(), encoding="utf-8")
        trace.write_text(json.dumps({"seq": 1}) + "\n" + trace.read_text(), encoding="utf-8")
        if analyze(dbwin, trace, 1, 1)["outcome"] != "correlated":
            raise AssertionError("diagnostic window offsets lost the correlated events")
        dbwin.write_text("[dbwin] capture_ready\n", encoding="utf-8")
        result = analyze(dbwin, trace)
        if result["outcome"] != "host-only" or result["correlated"] is not False:
            raise AssertionError("missing guest event was interpreted as correlation")
        dbwin.write_text(
            "BV-VIRGL-ALLOC-LIST-GROW-FAIL alloc_count=1 max_alloc=1 res_id=156\n",
            encoding="utf-8",
        )
        if analyze(dbwin, trace)["outcome"] != "resource-mismatch":
            raise AssertionError("mismatched resource ids were accepted")
        dbwin.write_text("[dbwin] capture_ready\n", encoding="utf-8")
        trace.write_text("\n".join(json.dumps(record) for record in (
            {"seq": 1, "name": "RESOURCE_CREATE_3D", "response_name": "OK_NODATA", "resource_id": 10},
            {"seq": 2, "name": "RESOURCE_UNREF", "response_name": "OK_NODATA", "resource_id": 10},
            {"seq": 3, "name": "RESOURCE_CREATE_BLOB", "response_name": "OK_NODATA", "resource_id": 12},
            {"seq": 4, "name": "RESOURCE_ATTACH_BACKING", "response_name": "ERR_UNSPEC", "resource_id": 10},
            {"seq": 5, "name": "RESOURCE_ATTACH_BACKING", "response_name": "ERR_UNSPEC", "resource_id": 12},
        )) + "\n", encoding="utf-8")
        result = analyze(dbwin, trace, skip_trace_lines=3)
        if result["outcome"] != "missing-create-attach" or result["host_missing_create_attach_count"] != 1 or result["first_missing_create_attach"]["resource_id"] != 10:
            raise AssertionError("full resource lifecycle did not isolate the missing-create attach")
    print("PASS: B4 guest/host diagnostic correlation mutations")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dbwin", type=Path)
    parser.add_argument("--trace", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--skip-dbwin-lines", type=int, default=0)
    parser.add_argument("--skip-trace-lines", type=int, default=0)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.dbwin is None or args.trace is None or args.out is None:
        parser.error("--dbwin, --trace, and --out are required")
    result = analyze(args.dbwin, args.trace, args.skip_dbwin_lines, args.skip_trace_lines)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(result["outcome"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
