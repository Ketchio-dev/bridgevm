"""Domain-work accounting shared by the three native app UI diagnostics.

Every tripwire is fatal in the host and must stay zero. The only domain object a
completed run may contain is one control model over an inert backend for each
seeded overview fixture VM (Amber and Indigo): none before seeding, exactly one per
seeded VM after it. Any other count, type or key is refused; there is no range.
"""
TRIPWIRES = ("runtime_creations", "install_creations", "file_jobs", "backend_calls")
SEEDED_FIXTURE_VMS = 2
MODEL_CREATIONS = {"before_seeding": 0, "after_seeding": SEEDED_FIXTURE_VMS, "seeded_fixture_vms": SEEDED_FIXTURE_VMS}


def verify_domain_work(tripwires, models):
    if not isinstance(tripwires, dict) or set(tripwires) != set(TRIPWIRES):
        raise ValueError("tripwire schema differs")
    if not all(type(tripwires[key]) is int and tripwires[key] == 0 for key in TRIPWIRES):
        raise ValueError("domain work was attempted")
    if not isinstance(models, dict) or set(models) != set(MODEL_CREATIONS):
        raise ValueError("control model accounting schema differs")
    if not all(type(models[key]) is int and models[key] == MODEL_CREATIONS[key] for key in MODEL_CREATIONS):
        raise ValueError("control models differ from exactly one per seeded fixture VM and none before seeding")
