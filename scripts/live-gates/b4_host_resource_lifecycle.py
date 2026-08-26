#!/usr/bin/env python3
"""Classify B4 host resource failures while replaying the full lifecycle."""

from __future__ import annotations

import json


CREATE_COMMANDS = {"RESOURCE_CREATE_3D", "RESOURCE_CREATE_BLOB"}


def classify_trace(
    lines: list[str], skip_lines: int
) -> tuple[list[dict[str, int]], list[dict[str, int]]]:
    live_resources: set[int] = set()
    never_backed: list[dict[str, int]] = []
    missing_create: list[dict[str, int]] = []
    for line_number, line in enumerate(lines, 1):
        if len(line) > 1024 * 1024:
            raise ValueError(f"oversized JSONL record at line {line_number}")
        record = json.loads(line)
        name = record.get("name")
        response = record.get("response_name")
        resource_id = record.get("resource_id")
        in_window = line_number > skip_lines
        if name in CREATE_COMMANDS and response == "OK_NODATA" and isinstance(resource_id, int):
            live_resources.add(resource_id)
        elif name == "RESOURCE_UNREF" and response == "OK_NODATA" and isinstance(resource_id, int):
            live_resources.discard(resource_id)
        elif (
            in_window and name == "RESOURCE_ATTACH_BACKING" and response == "ERR_UNSPEC"
            and isinstance(resource_id, int) and resource_id not in live_resources
        ):
            missing_create.append(
                {"seq": int(record["seq"]), "resource_id": resource_id,
                 "nr_entries": int(record.get("nr_entries", 0))}
            )
        if (
            in_window and name == "SUBMIT_3D" and response == "ERR_UNSPEC"
            and record.get("renderer_command_id") in (43, 45)
            and record.get("renderer_resource_found") is True
            and record.get("renderer_resource_backed") is False
            and isinstance(record.get("renderer_resource_id"), int)
        ):
            never_backed.append(
                {"seq": int(record["seq"]), "ctx_id": int(record.get("ctx_id", 0)),
                 "command_id": int(record["renderer_command_id"]),
                 "resource_id": int(record["renderer_resource_id"])}
            )
    return never_backed, missing_create
