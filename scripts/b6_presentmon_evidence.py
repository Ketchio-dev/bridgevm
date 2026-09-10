#!/usr/bin/env python3
"""Authenticate transferred PresentMon bytes against the fresh guest report."""
import json
import pathlib
import re
import sys
import time

from b6_frame_times import NO_CLAIM, authenticated_bytes, summarize

MAX_LOG_BYTES = 16 * 1024 * 1024


def guest_hash(log, offset, filename):
    if offset < 0:
        raise ValueError("negative guest-log offset")
    with pathlib.Path(log).open("rb") as stream:
        stream.seek(offset)
        raw = stream.read(MAX_LOG_BYTES + 1)
    if len(raw) > MAX_LOG_BYTES:
        raise ValueError("guest-log interval exceeds diagnostic limit")
    name = re.compile(r"(?<![A-Za-z0-9._-])" + re.escape(filename) + r"(?![A-Za-z0-9._-])")
    hashes = set()
    for line in raw.replace(b"\r", b"\n").decode("utf-8", "replace").splitlines():
        if not line.startswith("BVPRESENTMON ") or not name.search(line):
            continue
        match = re.search(r"\bsha256=([0-9a-f]{64})\b", line, re.IGNORECASE)
        if match:
            hashes.add(match.group(1).lower())
    if len(hashes) != 1:
        raise ValueError("expected one fresh guest-reported CSV identity")
    return hashes.pop()


def wait_sample(csv, expected, timeout=30.0, interval=0.1):
    deadline = time.monotonic() + timeout
    last_error = "CSV not yet transferred"
    while True:
        if pathlib.Path(csv).is_symlink():
            raise ValueError("refusing symlink CSV")
        try:
            authenticated_bytes(csv, expected)
            break
        except (OSError, ValueError) as error:
            last_error = str(error)
        if time.monotonic() >= deadline:
            raise TimeoutError("guest CSV identity never arrived: " + last_error)
        time.sleep(interval)
    # Reauthenticate inside the parser too; a replacement between checks fails.
    return summarize(csv, expected)[0]


def main(argv):
    try:
        if len(argv) != 5:
            raise ValueError("usage: b6_presentmon_evidence.py LOG OFFSET CSV INPUT_COUNT")
        issued = int(argv[4])
        if issued < 1:
            raise ValueError("no host input commands were issued")
        csv = pathlib.Path(argv[3])
        expected = guest_hash(argv[1], int(argv[2]), csv.name)
        report = wait_sample(csv, expected)
        report.update(valid=True, guest_reported_sha256=expected, host_input_commands=issued)
        print(json.dumps(report, indent=2, allow_nan=False))
        return 0
    except (OSError, ValueError) as error:
        print(json.dumps(dict(valid=False, error=str(error), **NO_CLAIM)))
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
