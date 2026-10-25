"""The ramfb display exporter's record in the post-footer host tail.

A running ramfb display exporter counts each tick by how late it woke against
its deadline and each published frame by the interval since the previous one
(crates/bridgevm-hvf/examples/hvf_gic_boot_probe/ramfb_display_timing.rs). The
probe joins the exporter before the final report, so the platform prints its
`ramfb display timing:` record once as it drops (platform_virt/teardown_records.rs),
after the HDA controller's records and the CoreAudio teardown records.
hvf_host_tail admits it there, at most once and after every HDA record, within
its unchanged record and byte bounds. Fields are unsigned decimals without
leading zeros: ticks by lateness (le is at most), the largest lateness in
microseconds, published frames by interval and the longest interval in
milliseconds. A log from before the record reads as it did; nothing requires it.
"""

from __future__ import annotations

import re

VALUE = "(0|[1-9][0-9]{0,19})"
DISPLAY_PREFIX = "ramfb display timing:"
DISPLAY_FIELDS = ("ticks", "late_le_1ms", "late_le_4ms", "late_le_8ms", "late_le_16ms", "late_gt_16ms", "max_late_us",
                  "published", "interval_le_20ms", "interval_le_40ms", "interval_le_70ms", "interval_gt_70ms",
                  "max_interval_ms")
DISPLAY_LINE = re.compile(DISPLAY_PREFIX + "".join(f" {field}={VALUE}" for field in DISPLAY_FIELDS))
# Its placement code; hvf_host_tail requires it after every HDA record's code.
DISPLAY_CODES = {DISPLAY_PREFIX: "D"}
DISPLAY_ORDER = "D?"
