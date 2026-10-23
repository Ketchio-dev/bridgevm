"""Private lane-result fields: diagnostic evidence kept beside the private T17/T19
lane result. The writers authenticate every other field exactly; these fields never
decide a lane, never reach the public receipt and are not validated here."""

KEYS = frozenset({"tpm_evidence"})


def judged_keys(value: dict) -> frozenset:
    """The lane keys a receipt writer judges: every key except a private diagnostic."""
    return frozenset(value) - KEYS
