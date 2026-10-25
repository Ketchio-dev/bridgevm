"""Private lane-result fields: diagnostic evidence kept beside the private T17/T19
lane result. These fields never decide a lane, never reach the public receipt and are
not validated here. The receipt writers judge the lane keys left after these and after
the well-formed first-run host records (product_e2e_host_records.lane_keys)."""

KEYS = frozenset({"tpm_evidence"})
