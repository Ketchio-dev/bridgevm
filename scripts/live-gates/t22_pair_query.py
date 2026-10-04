"""Typed Windows payload validation, independent of host filesystem APIs."""
import re

VOLUME_ID = re.compile(r"\\\\\?\\Volume\{[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\}\\\Z")
METHODS = {"conversion_return", "conversion_status", "encryption_return",
           "encryption_method", "protection_return", "protection_status"}
VOLUME_KEYS = {"device_id", "drive_letter", "filesystem", "drive_type", *METHODS}


def validate_query(value, nonce, script_hash):
    if (type(nonce) is not str or not re.fullmatch(r"[0-9a-f]{32}", nonce)
            or type(script_hash) is not str or not re.fullmatch(r"[0-9a-f]{64}", script_hash)):
        raise ValueError("invalid query nonce or script identity")
    expected = {"schema": "bridgevm.t22-pair-admission.v1", "nonce": nonce,
                "script_sha256": script_hash}
    if (type(value) is not dict or set(value) != {*expected, "system_drive", "volumes"}
            or any(type(value[k]) is not str or value[k] != v for k, v in expected.items())
            or type(value["system_drive"]) is not str or not re.fullmatch(r"[A-Z]:", value["system_drive"])
            or type(value["volumes"]) is not list or not 1 <= len(value["volumes"]) <= 16):
        raise ValueError("invalid Windows encryption observation")
    identities, letters, os_count, ntfs_count = set(), set(), 0, 0
    for entry in value["volumes"]:
        if (type(entry) is not dict or set(entry) != VOLUME_KEYS
                or type(entry["device_id"]) is not str or not VOLUME_ID.fullmatch(entry["device_id"])
                or entry["device_id"].lower() in identities
                or type(entry["drive_type"]) is not int or entry["drive_type"] != 3
                or entry["filesystem"] not in ("NTFS", "FAT", "FAT32")):
            raise ValueError("invalid or duplicated fixed volume")
        identities.add(entry["device_id"].lower())
        letter = entry["drive_letter"]
        if letter is not None:
            if type(letter) is not str or not re.fullmatch(r"[A-Z]:", letter) or letter in letters:
                raise ValueError("invalid or duplicated drive letter")
            letters.add(letter)
        if entry["filesystem"] == "NTFS":
            ntfs_count += 1
            if any(type(entry[k]) is not int or entry[k] != 0 for k in METHODS):
                raise ValueError("volume is not fully decrypted with no encryption method")
            os_count += letter == value["system_drive"]
        elif any(entry[k] is not None for k in METHODS) or letter == value["system_drive"]:
            raise ValueError("invalid non-NTFS observation")
    if os_count != 1:
        raise ValueError("Windows OS volume was not covered")
    return {"fixed_volume_count": len(value["volumes"]), "decrypted_ntfs_volume_count": ntfs_count}
