"""Profile dispatch keeps observation collection distinct from criterion proof."""
from guest_input_profiles import COHERENCE_PROFILE, PRODUCTION_PROFILE, asset_names


def sink_filename(profile):
    asset_names(profile)
    return "bv-coherence-multiwindow.ps1" if profile == COHERENCE_PROFILE else "bv-input-order-sink.ps1"


def observation_succeeded(profile, receipt):
    asset_names(profile)
    if profile == COHERENCE_PROFILE:
        return receipt.get("coherence", {}).get("collection_complete") is True
    return receipt["guest_application_observed"] and (profile != PRODUCTION_PROFILE or receipt["production_driver_observed"])


def make_controller(control, log, share, paths, profile):
    if set(paths) != asset_names(profile):
        raise ValueError("controller profile asset mismatch")
    if profile == COHERENCE_PROFILE:
        from coherence_inventory_controller import CoherenceController
        return CoherenceController(control, log, share)
    from guest_input_driver_variant import make_controller as input_controller
    return input_controller(control, log, share, paths)
