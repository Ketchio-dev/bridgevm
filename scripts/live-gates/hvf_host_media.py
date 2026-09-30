"""Stop-time media persistence records, bound to the final host report.

The probe prints `NVMe disk written back: PATH (N bytes)` as each write
completes, before the final report and in the same run.log as guest agent
output. final_report.rs repeats each completed write as a `host media: ` record
(stop_media.rs) on the lines right after the bound stop record, with the path
escaped to printable ASCII; only a record there is host evidence. A failed
write panics before the report, so its log binds no records. The stop record
must be printable ASCII too, as every host stop reason is, so that
scripts/hvf-terminal-report.sh's grep decides exactly as this module does.
"""

from __future__ import annotations

import re
from hvf_terminal_report import terminal_stop

RECORD = re.compile(rb"host media: (UEFI vars|NVMe disk|NVMe target namespace \(NSID 2\)) "
                    rb"(written back|snapshot written): [ -~]+ \([0-9]+ bytes\)")
NVME_WRITE_BACK = re.compile(r"host media: NVMe disk written back: [ -~]+ \([0-9]+ bytes\)")
STOP = re.compile(r"stop: [ -~]*")


def host_media_records(raw: bytes) -> list[tuple[int, str]] | None:
    """Offset and text of each record in the run right after the final report's stop record.

    The first line that is not a whole record ends the run. None when no report
    binds or its stop record is not printable ASCII.
    """
    stop = terminal_stop(raw)
    if stop is None or not STOP.fullmatch(stop[1]):
        return None
    records, start = [], raw.index(b"\n", stop[0]) + 1
    end = raw.find(b"\n", start)
    while end >= 0 and RECORD.fullmatch(raw, start, end):
        records.append((start, raw[start:end].decode("ascii")))
        start, end = end + 1, raw.find(b"\n", end + 1)
    return records


def nvme_write_back_offset(raw: bytes) -> int | None:
    """Offset of the bound records' only NVMe disk write-back, else None."""
    found = [offset for offset, record in host_media_records(raw) or () if NVME_WRITE_BACK.fullmatch(record)]
    return found[0] if len(found) == 1 else None
