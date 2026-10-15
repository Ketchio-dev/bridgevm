"""Strict DTO semantics; raw hashes bind bytes, Swift admission enforces encoding."""
from app_ui_host_v2_diagnostic import protocol_record as read_record


def protocol_record(path, kind, required, optional=()):
    value = read_record(path, kind, required, optional)
    # Swift Codable omits nil optionals. Null is not a valid success record.
    if any(key in value and value[key] is None for key in optional):
        raise ValueError("install protocol optional fields must be omitted, not null")
    return value
