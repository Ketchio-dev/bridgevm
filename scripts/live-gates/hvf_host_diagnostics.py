"""The HDA diagnostic records the host prints in the post-footer tail.

When the platform drops the HDA controller after the final report, a controller
whose CoreAudio sink installed host records prints `hda poll:`, `hda stream:`,
`hda repeat:` and `hda CoreAudio timing:` once each, in that order, just before
the sink prints its lifecycle, stats and continuity records
(crates/bridgevm-hvf/src/hda/diagnostics_teardown.rs). hvf_host_tail admits them
there at most once each, in that order and before any stats or continuity
record, within its unchanged record and byte bounds. Fields are unsigned
decimals without leading zeros, `*_ms` buffer lengths carry three decimals and
position_buffer is 0 or 1. A log from before the records reads as it did and
nothing requires them; hda_diagnostics.py reports them.
"""

from __future__ import annotations

import re

VALUE = "(0|[1-9][0-9]{0,19})"
MILLIS = VALUE + r"\.[0-9]{3}"
RECORDS = {
    "hda poll:": ("polls", "gap_le_10ms", "gap_le_20ms", "gap_le_50ms", "gap_le_100ms", "gap_gt_100ms", "max_gap_ms",
                  "capped_polls", "capped_lost_ms", "ioc_completions", "max_iocs_per_poll", "msis_sent"),
    "hda stream:": ("runs", "rate_hz", "channels", "bits", "cbl_bytes", "cbl_ms", "entries", "entry_bytes_min",
                    "entry_bytes_max", "entry_ms_min", "entry_ms_max", "ioc_entries", "position_buffer", "lpib_reads",
                    "running_ms"),
    "hda repeat:": ("passes", "silent_passes", "compared_passes", "repeat_passes", "max_repeat_run"),
    "hda CoreAudio timing:": ("callbacks", "interval_le_10ms", "interval_le_15ms", "interval_le_20ms",
                              "interval_le_30ms", "interval_le_50ms", "interval_gt_50ms", "max_interval_ms",
                              "late_callbacks", "trylock_misses", "ring_samples", "ring_min_ms", "ring_max_ms",
                              "ring_final_ms", "ring_trimmed_ms"),
}
FIELD_VALUES = {"cbl_ms": MILLIS, "entry_ms_min": MILLIS, "entry_ms_max": MILLIS, "position_buffer": "([01])"}
GRAMMARS = {prefix: re.compile(prefix + "".join(f" {field}={FIELD_VALUES.get(field, VALUE)}" for field in fields))
            for prefix, fields in RECORDS.items()}
DIAGNOSTIC_LINES = tuple(GRAMMARS.values())
# Placement codes in print order; hvf_host_tail requires them before its S and C.
DIAGNOSTIC_CODES = dict(zip(RECORDS, "PTRM"))
DIAGNOSTIC_ORDER = "".join(f"{code}?" for code in DIAGNOSTIC_CODES.values())
