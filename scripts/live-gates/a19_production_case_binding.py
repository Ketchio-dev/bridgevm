"""Production-only T22 completeness and phase3 source binding before receipt commit."""
from a19_interrupt_cases import ADDED_POINTS, CASE_FLAGS, case_count, validate_cases


def validate_production_cases(value: dict) -> None:
    if case_count(value) != 3 or any(not value[prefix + flag]
            for prefix in ADDED_POINTS for flag in CASE_FLAGS):
        raise ValueError("production collection requires all three interruption cases")
    # Later guest boots mutate exports, not the phase3 logical source cloned by both auxiliaries.
    for member in ("disk", "vars"):
        source = value[f"preinterrupt_{member}_sha256"]
        if any(value[f"{prefix}_{member}_sha256"] != source for prefix in
               ("postkill", "swap_preinterrupt", "swap_postkill", "create_source", "create_postretry")):
            raise ValueError("auxiliary pair differs from the first case phase3 source")
    validate_cases({**value, "pass": True})
