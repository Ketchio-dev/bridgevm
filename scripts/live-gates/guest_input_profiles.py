"""Explicit diagnostic profiles; neither profile is a release gate."""
PROFILE = "no-3d-installed-input-450s"
PRODUCTION_PROFILE = "no-3d-production-driver-input-450s"


def asset_names(profile):
    base = {"image", "vars", "binary", "firmware"}
    if profile == PROFILE:
        return base
    if profile == PRODUCTION_PROFILE:
        return base | {"driver"}
    raise ValueError("unknown guest input diagnostic profile")
