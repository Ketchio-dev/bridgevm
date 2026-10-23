"""The first run's host records a T17 or T19 lane result may carry, and the lane-key check that admits them.

BridgeVMProductE2E reads the run log its journey archived from the first boot, where the guest played audio
and took the keyboard and pointer challenge, and copies the HDA diagnostic records (hvf_host_diagnostics.py)
and the display timing record (hvf_host_display.py) of that log's bound post-footer host tail into its private
lane result as `first_run_host_records`: one object per printed record, named after its prefix (`hda poll:` is
hda_poll), holding the record's unsigned fields as integers, with a three-decimal millisecond field as integer
microseconds (cbl_ms is cbl_us). The key is absent when the host printed none. Both receipt writers admit it
only well formed, and no receipt copies it.
"""

from __future__ import annotations

import hvf_host_diagnostics as HDA
import hvf_host_display as DISPLAY

KEY = "first_run_host_records"


def _field(field: str) -> str:
    return field.replace("_ms", "_us", 1) if HDA.FIELD_VALUES.get(field) == HDA.MILLIS else field


LANE_RECORDS = {prefix[:-1].lower().replace(" ", "_"): frozenset(map(_field, fields))
                for prefix, fields in {**HDA.RECORDS, DISPLAY.DISPLAY_PREFIX: DISPLAY.DISPLAY_FIELDS}.items()}


def _well_formed(name: object, fields: object) -> bool:
    return (name in LANE_RECORDS and isinstance(fields, dict) and frozenset(fields) == LANE_RECORDS[name]
            and all(type(value) is int and 0 <= value < 1 << 64 for value in fields.values()))


def lane_keys(value: dict) -> frozenset:
    """The lane's keys besides its host records, which must be well formed when present."""
    if KEY not in value:
        return frozenset(value)
    records = value[KEY]
    if not isinstance(records, dict) or not records or not all(_well_formed(*item) for item in records.items()):
        raise ValueError("lane host records are malformed")
    return frozenset(value) - {KEY}
