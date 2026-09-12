"""Stage checkout-pinned fixture dependencies and retain their exact hashes."""
import shutil
from guest_input_live_inputs import digest
from guest_input_profile_dispatch import sink_filename
from guest_input_profiles import COHERENCE_PROFILE
from coherence_fixture_readiness import FILES

def stage(root, share, profile):
    primary = sink_filename(profile)
    names = [primary]
    if profile == COHERENCE_PROFILE:
        names = list(FILES)
    hashes = {}
    for name in names:
        source, target = root / "scripts/win-assets" / name, share / name
        hashes[name] = digest(source)
        with source.open("rb") as incoming, target.open("xb") as outgoing:
            shutil.copyfileobj(incoming, outgoing)
        if digest(target) != hashes[name]:
            raise ValueError("fixture dependency copy mismatch")
    return root / "scripts/win-assets" / primary, hashes
