"""Identify B9 sealed-share failures regardless of runner stage."""


class AssetIntegrityError(ValueError):
    """A mutable shared asset no longer matches its sealed source."""


def invalid_evidence_failure(stage: str, error: Exception) -> bool:
    return (isinstance(error, AssetIntegrityError)
            or (stage in ("asset-prelaunch", "asset-final")
                and isinstance(error, (OSError, ValueError)))
            or (stage in ("vlc", "playback") and isinstance(error, ValueError)))
